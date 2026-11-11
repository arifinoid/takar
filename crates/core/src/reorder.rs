//! Purchase recommendations.
//!
//! A material is replenished so that it lasts through the supplier's lead time
//! plus the tenant's safety stock:
//!
//! ```text
//! target      = average_daily_usage × lead_time_days + min_stock
//! recommended = max(0, target - on_hand)
//! ```
//!
//! When the material has a shelf life, the order is capped at what can be used
//! before it expires, so a long lead time cannot push a tenant into buying
//! stock that will rot:
//!
//! ```text
//! cap = max(0, average_daily_usage × shelf_life_days - on_hand)
//! ```
//!
//! The result rounds **up** to three decimals: buying slightly more is cheaper
//! than running out mid-service.
//!
//! A material with no usage history has no forecast. The caller flags it; the
//! plan still closes the gap to `min_stock`, and the shelf-life cap is skipped
//! because a zero usage would otherwise cap every order to zero.

use std::fmt;

use rust_decimal::{Decimal, RoundingStrategy};

/// Quantities are stored as `numeric(18,3)` in the batch ledger.
const SCALE: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReorderError {
    /// On-hand, minimum stock, and average usage cannot be negative.
    NegativeQuantity,
    /// Lead time is a number of days, never negative.
    NegativeLeadTime,
    /// Shelf life is a number of days, never negative.
    InvalidShelfLife,
    /// The usage window must cover at least one day.
    InvalidWindow,
}

impl fmt::Display for ReorderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReorderError::NegativeQuantity => formatter.write_str("quantity cannot be negative"),
            ReorderError::NegativeLeadTime => formatter.write_str("lead time cannot be negative"),
            ReorderError::InvalidShelfLife => formatter.write_str("shelf life cannot be negative"),
            ReorderError::InvalidWindow => formatter.write_str("usage window must be positive"),
        }
    }
}

impl std::error::Error for ReorderError {}

/// Everything the plan needs about one material.
#[derive(Debug, Clone, Copy)]
pub struct ReorderInput {
    pub on_hand: Decimal,
    pub min_stock: Decimal,
    pub average_daily_usage: Decimal,
    pub lead_time_days: i32,
    pub shelf_life_days: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReorderPlan {
    /// On-hand level the order aims for, before the shelf-life cap.
    pub target_qty: Decimal,
    /// How much to buy now.
    pub recommended_qty: Decimal,
    /// True when shelf life, not the target, decided the quantity.
    pub capped_by_shelf_life: bool,
}

/// Total consumption spread over the window it was measured in.
pub fn average_daily_usage(total_used: Decimal, window_days: u32) -> Result<Decimal, ReorderError> {
    if window_days == 0 {
        return Err(ReorderError::InvalidWindow);
    }

    if total_used.is_sign_negative() {
        return Err(ReorderError::NegativeQuantity);
    }

    Ok(total_used / Decimal::from(window_days))
}

pub fn reorder_plan(input: ReorderInput) -> Result<ReorderPlan, ReorderError> {
    if input.on_hand.is_sign_negative()
        || input.min_stock.is_sign_negative()
        || input.average_daily_usage.is_sign_negative()
    {
        return Err(ReorderError::NegativeQuantity);
    }

    if input.lead_time_days < 0 {
        return Err(ReorderError::NegativeLeadTime);
    }

    if let Some(days) = input.shelf_life_days {
        if days < 0 {
            return Err(ReorderError::InvalidShelfLife);
        }
    }

    let target = input.average_daily_usage * Decimal::from(input.lead_time_days) + input.min_stock;
    let mut recommended = (target - input.on_hand).max(Decimal::ZERO);
    let mut capped_by_shelf_life = false;

    if input.average_daily_usage > Decimal::ZERO {
        if let Some(days) = input.shelf_life_days {
            let cover = input.average_daily_usage * Decimal::from(days);
            let limit = (cover - input.on_hand).max(Decimal::ZERO);

            if recommended > limit {
                recommended = limit;
                capped_by_shelf_life = true;
            }
        }
    }

    Ok(ReorderPlan {
        target_qty: round_up(target),
        recommended_qty: round_up(recommended),
        capped_by_shelf_life,
    })
}

fn round_up(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(SCALE, RoundingStrategy::ToPositiveInfinity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(value: &str) -> Decimal {
        value.parse().expect("decimal")
    }

    fn input(
        on_hand: &str,
        min_stock: &str,
        usage: &str,
        lead: i32,
        shelf: Option<i32>,
    ) -> ReorderInput {
        ReorderInput {
            on_hand: dec(on_hand),
            min_stock: dec(min_stock),
            average_daily_usage: dec(usage),
            lead_time_days: lead,
            shelf_life_days: shelf,
        }
    }

    #[test]
    fn the_order_covers_the_lead_time_plus_the_safety_stock() {
        // 1.5 kg a day for 4 days, plus 2 kg kept in reserve, minus 5 on hand.
        let plan = reorder_plan(input("5", "2", "1.5", 4, None)).expect("plan");

        assert_eq!(plan.target_qty, dec("8"));
        assert_eq!(plan.recommended_qty, dec("3"));
        assert!(!plan.capped_by_shelf_life);
    }

    #[test]
    fn stock_that_already_covers_the_target_needs_no_order() {
        let plan = reorder_plan(input("20", "2", "1.5", 4, None)).expect("plan");

        assert_eq!(plan.recommended_qty, Decimal::ZERO);
    }

    #[test]
    fn a_zero_lead_time_still_refills_the_safety_stock() {
        let plan = reorder_plan(input("1", "5", "2", 0, None)).expect("plan");

        assert_eq!(plan.target_qty, dec("5"));
        assert_eq!(plan.recommended_qty, dec("4"));
    }

    #[test]
    fn shelf_life_caps_the_order_at_what_will_be_used() {
        // 2 a day with a 3-day shelf life covers 6; 1 is on hand, so 5 is the
        // most worth buying even though the lead-time target asks for 21.
        let plan = reorder_plan(input("1", "2", "2", 10, Some(3))).expect("plan");

        assert_eq!(plan.target_qty, dec("22"));
        assert_eq!(plan.recommended_qty, dec("5"));
        assert!(plan.capped_by_shelf_life);
    }

    #[test]
    fn a_cap_that_does_not_bite_leaves_the_flag_clear() {
        let plan = reorder_plan(input("0", "0", "2", 1, Some(30))).expect("plan");

        assert_eq!(plan.recommended_qty, dec("2"));
        assert!(!plan.capped_by_shelf_life);
    }

    #[test]
    fn no_history_falls_back_to_the_minimum_and_skips_the_cap() {
        let plan = reorder_plan(input("0", "5", "0", 3, Some(2))).expect("plan");

        assert_eq!(plan.recommended_qty, dec("5"));
        assert!(!plan.capped_by_shelf_life);
    }

    #[test]
    fn quantities_round_up_to_the_ledger_scale() {
        let plan = reorder_plan(input("0", "0", "0.1234", 1, None)).expect("plan");

        assert_eq!(plan.recommended_qty, dec("0.124"));
    }

    #[test]
    fn an_empty_window_is_rejected() {
        assert_eq!(
            average_daily_usage(dec("10"), 0),
            Err(ReorderError::InvalidWindow)
        );
    }

    #[test]
    fn the_average_spreads_usage_over_the_window() {
        let average = average_daily_usage(dec("45"), 30).expect("average");

        assert_eq!(average, dec("1.5"));
    }

    #[test]
    fn negative_inputs_are_rejected() {
        assert_eq!(
            average_daily_usage(dec("-1"), 30),
            Err(ReorderError::NegativeQuantity)
        );
        assert_eq!(
            reorder_plan(input("-1", "0", "1", 1, None)),
            Err(ReorderError::NegativeQuantity)
        );
        assert_eq!(
            reorder_plan(input("0", "0", "1", -1, None)),
            Err(ReorderError::NegativeLeadTime)
        );
        assert_eq!(
            reorder_plan(input("0", "0", "1", 1, Some(-1))),
            Err(ReorderError::InvalidShelfLife)
        );
    }
}
