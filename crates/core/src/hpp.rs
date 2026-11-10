//! Cost of a production run (HPP).
//!
//! HPP per unit is what the run cost divided by what it actually produced:
//!
//! ```text
//! unit_cost = (material + labor + overhead) / actual_qty
//! ```
//!
//! Material cost is the sum of the consumption snapshots, so a purchase at a
//! different price later cannot change a past run. Components round to two
//! decimals (the money columns) before the division, so the stored components
//! always add up to the stored unit cost. The unit cost keeps four decimals
//! (D7).

use std::fmt;

use rust_decimal::Decimal;

/// Money is stored as `numeric(18,2)`.
const MONEY_SCALE: u32 = 2;
/// HPP per unit is stored as `numeric(18,4)`.
const UNIT_COST_SCALE: u32 = 4;

/// One batch consumed by the run, with the cost it had at that moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Consumption {
    pub qty: Decimal,
    pub unit_cost: Decimal,
}

/// The cost snapshot of one production run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductionCost {
    pub material_cost: Decimal,
    pub labor_cost: Decimal,
    pub overhead_cost: Decimal,
    /// Cost per finished unit.
    pub unit_cost: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostError {
    /// A run must produce something; dividing by zero has no meaning.
    InvalidQuantity,
    /// Costs and consumed quantities cannot be negative.
    NegativeCost,
}

impl fmt::Display for CostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CostError::InvalidQuantity => {
                formatter.write_str("production quantity must be positive")
            }
            CostError::NegativeCost => formatter.write_str("cost cannot be negative"),
        }
    }
}

impl std::error::Error for CostError {}

/// Snapshots the cost of a run from its consumptions, labor, and overhead.
///
/// `actual_qty` is what the run really produced, not what was planned.
pub fn production_cost(
    consumptions: &[Consumption],
    labor_cost: Decimal,
    overhead_cost: Decimal,
    actual_qty: Decimal,
) -> Result<ProductionCost, CostError> {
    if actual_qty <= Decimal::ZERO {
        return Err(CostError::InvalidQuantity);
    }

    if labor_cost.is_sign_negative() || overhead_cost.is_sign_negative() {
        return Err(CostError::NegativeCost);
    }

    let mut material = Decimal::ZERO;

    for consumption in consumptions {
        if consumption.qty.is_sign_negative() || consumption.unit_cost.is_sign_negative() {
            return Err(CostError::NegativeCost);
        }

        material += consumption.qty * consumption.unit_cost;
    }

    let material_cost = material.round_dp(MONEY_SCALE);
    let labor_cost = labor_cost.round_dp(MONEY_SCALE);
    let overhead_cost = overhead_cost.round_dp(MONEY_SCALE);
    let total = material_cost + labor_cost + overhead_cost;
    let unit_cost = (total / actual_qty).round_dp(UNIT_COST_SCALE);

    Ok(ProductionCost {
        material_cost,
        labor_cost,
        overhead_cost,
        unit_cost,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(value: &str) -> Decimal {
        value.parse().expect("decimal")
    }

    fn consumption(qty: &str, unit_cost: &str) -> Consumption {
        Consumption {
            qty: dec(qty),
            unit_cost: dec(unit_cost),
        }
    }

    #[test]
    fn material_cost_sums_the_consumption_snapshots() {
        let cost = production_cost(
            &[consumption("1", "12000"), consumption("1.625", "9000")],
            Decimal::ZERO,
            Decimal::ZERO,
            dec("10"),
        )
        .expect("cost");

        assert_eq!(cost.material_cost, dec("26625.00"));
        assert_eq!(cost.unit_cost, dec("2662.5000"));
    }

    #[test]
    fn labor_and_overhead_are_added_before_dividing() {
        let cost = production_cost(
            &[consumption("1", "10000")],
            dec("5000"),
            dec("2500"),
            dec("10"),
        )
        .expect("cost");

        assert_eq!(cost.material_cost, dec("10000.00"));
        assert_eq!(cost.labor_cost, dec("5000.00"));
        assert_eq!(cost.overhead_cost, dec("2500.00"));
        assert_eq!(cost.unit_cost, dec("1750.0000"));
    }

    #[test]
    fn unit_cost_keeps_four_decimals() {
        let cost = production_cost(
            &[consumption("1", "10000")],
            Decimal::ZERO,
            Decimal::ZERO,
            dec("3"),
        )
        .expect("cost");

        assert_eq!(cost.unit_cost, dec("3333.3333"));
    }

    #[test]
    fn components_round_to_money_before_dividing() {
        let cost = production_cost(
            &[consumption("0.333", "0.333")],
            Decimal::ZERO,
            Decimal::ZERO,
            dec("1"),
        )
        .expect("cost");

        assert_eq!(cost.material_cost, dec("0.11"));
        assert_eq!(cost.unit_cost, dec("0.1100"));
    }

    #[test]
    fn zero_labor_and_overhead_leave_the_material_cost_alone() {
        let cost = production_cost(
            &[consumption("2.5", "8000")],
            Decimal::ZERO,
            Decimal::ZERO,
            dec("5"),
        )
        .expect("cost");

        assert_eq!(cost.unit_cost, dec("4000.0000"));
    }

    #[test]
    fn a_zero_or_negative_quantity_is_rejected() {
        for qty in ["0", "-1"] {
            assert_eq!(
                production_cost(&[], Decimal::ZERO, Decimal::ZERO, dec(qty)),
                Err(CostError::InvalidQuantity)
            );
        }
    }

    #[test]
    fn negative_costs_are_rejected() {
        assert_eq!(
            production_cost(&[], dec("-1"), Decimal::ZERO, dec("1")),
            Err(CostError::NegativeCost)
        );

        assert_eq!(
            production_cost(&[], Decimal::ZERO, dec("-1"), dec("1")),
            Err(CostError::NegativeCost)
        );

        assert_eq!(
            production_cost(
                &[consumption("-1", "1000")],
                Decimal::ZERO,
                Decimal::ZERO,
                dec("1")
            ),
            Err(CostError::NegativeCost)
        );

        assert_eq!(
            production_cost(
                &[consumption("1", "-1000")],
                Decimal::ZERO,
                Decimal::ZERO,
                dec("1")
            ),
            Err(CostError::NegativeCost)
        );
    }
}
