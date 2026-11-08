//! FEFO allocation.
//!
//! First-expired, first-out: consume the batch with the earliest expiry, treat
//! a missing expiry as the last resort, and break ties by the earliest receipt.
//! The function sorts its own input, so a caller cannot consume out of order.
//!
//! Allocation is a pure decision about quantities. It does not judge whether a
//! batch is still fit to use and it does not touch stock; the caller applies
//! the lines to the ledger.

use std::cmp::Ordering;
use std::fmt;

use chrono::NaiveDate;
use rust_decimal::Decimal;

/// A batch as the allocator sees it. Only the fields that drive FEFO and the
/// available quantity are needed, so any id type can ride along.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockBatch<T> {
    pub id: T,
    pub received_at: NaiveDate,
    pub expires_at: Option<NaiveDate>,
    pub qty_remaining: Decimal,
}

/// One batch and the quantity taken from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocationLine<T> {
    pub id: T,
    pub qty: Decimal,
}

/// The result of an allocation: lines in FEFO order plus the explicit gap
/// between what was asked for and what the batches could cover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocation<T> {
    pub lines: Vec<AllocationLine<T>>,
    /// Sum of the lines. Never greater than the required quantity.
    pub allocated: Decimal,
    /// Required quantity the batches could not cover. Zero on success.
    pub shortfall: Decimal,
}

impl<T> Allocation<T> {
    pub fn is_fully_allocated(&self) -> bool {
        self.shortfall.is_zero()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FefoError {
    /// A negative requirement has no meaning for consumption.
    NegativeQuantity,
}

impl fmt::Display for FefoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FefoError::NegativeQuantity => formatter.write_str("quantity cannot be negative"),
        }
    }
}

impl std::error::Error for FefoError {}

/// Allocates `required` across `batches` in FEFO order.
///
/// Ordering: `expires_at` ascending with missing expiry last, then
/// `received_at` ascending. Batches that tie on both keep their input order.
/// Batches with no quantity left are ignored. An insufficient total is not an
/// error: the shortfall is returned explicitly so the caller can show it.
pub fn allocate<T: Clone>(
    batches: &[StockBatch<T>],
    required: Decimal,
) -> Result<Allocation<T>, FefoError> {
    if required.is_sign_negative() {
        return Err(FefoError::NegativeQuantity);
    }

    let mut ordered: Vec<&StockBatch<T>> = batches
        .iter()
        .filter(|batch| batch.qty_remaining > Decimal::ZERO)
        .collect();

    // `sort_by` is stable, so full ties keep the caller's order.
    ordered.sort_by(|left, right| {
        expiry_order(left.expires_at, right.expires_at)
            .then_with(|| left.received_at.cmp(&right.received_at))
    });

    let mut remaining = required;
    let mut lines = Vec::new();

    for batch in ordered {
        if remaining.is_zero() {
            break;
        }

        let qty = batch.qty_remaining.min(remaining);
        lines.push(AllocationLine {
            id: batch.id.clone(),
            qty,
        });
        remaining -= qty;
    }

    Ok(Allocation {
        allocated: required - remaining,
        shortfall: remaining,
        lines,
    })
}

fn expiry_order(left: Option<NaiveDate>, right: Option<NaiveDate>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
    }

    fn dec(value: &str) -> Decimal {
        value.parse().expect("decimal")
    }

    fn batch(
        id: &'static str,
        received_at: NaiveDate,
        expires_at: Option<NaiveDate>,
        qty_remaining: &str,
    ) -> StockBatch<&'static str> {
        StockBatch {
            id,
            received_at,
            expires_at,
            qty_remaining: dec(qty_remaining),
        }
    }

    fn ids<T: Clone>(allocation: &Allocation<T>) -> Vec<T> {
        allocation
            .lines
            .iter()
            .map(|line| line.id.clone())
            .collect()
    }

    #[test]
    fn consumes_the_earliest_expiry_first() {
        let batches = vec![
            batch("march", date(2026, 1, 1), Some(date(2026, 3, 1)), "5"),
            batch("january", date(2026, 1, 1), Some(date(2026, 1, 1)), "5"),
            batch("february", date(2026, 1, 1), Some(date(2026, 2, 1)), "5"),
        ];

        let allocation = allocate(&batches, dec("12")).expect("allocation");

        assert_eq!(ids(&allocation), vec!["january", "february", "march"]);
        assert_eq!(
            allocation
                .lines
                .iter()
                .map(|line| line.qty)
                .collect::<Vec<_>>(),
            vec![dec("5"), dec("5"), dec("2")]
        );
        assert_eq!(allocation.allocated, dec("12"));
        assert!(allocation.is_fully_allocated());
    }

    #[test]
    fn consumes_a_missing_expiry_last() {
        let batches = vec![
            batch("no_expiry", date(2026, 1, 1), None, "5"),
            batch("dated", date(2026, 1, 1), Some(date(2027, 1, 1)), "5"),
        ];

        let allocation = allocate(&batches, dec("6")).expect("allocation");

        assert_eq!(ids(&allocation), vec!["dated", "no_expiry"]);
        assert_eq!(allocation.lines[1].qty, dec("1"));
    }

    #[test]
    fn breaks_expiry_ties_by_the_earliest_receipt() {
        let expiry = Some(date(2026, 6, 1));
        let batches = vec![
            batch("late_receipt", date(2026, 1, 20), expiry, "5"),
            batch("early_receipt", date(2026, 1, 10), expiry, "5"),
        ];

        let allocation = allocate(&batches, dec("5")).expect("allocation");

        assert_eq!(ids(&allocation), vec!["early_receipt"]);
    }

    #[test]
    fn keeps_input_order_when_expiry_and_receipt_tie() {
        let received = date(2026, 1, 10);
        let expiry = Some(date(2026, 6, 1));
        let batches = vec![
            batch("first", received, expiry, "1"),
            batch("second", received, expiry, "1"),
        ];

        let allocation = allocate(&batches, dec("1")).expect("allocation");

        assert_eq!(ids(&allocation), vec!["first"]);
    }

    #[test]
    fn reports_an_explicit_shortfall() {
        let batches = vec![
            batch("a", date(2026, 1, 1), Some(date(2026, 2, 1)), "5"),
            batch("b", date(2026, 1, 2), Some(date(2026, 3, 1)), "2"),
        ];

        let allocation = allocate(&batches, dec("10")).expect("allocation");

        assert_eq!(ids(&allocation), vec!["a", "b"]);
        assert_eq!(allocation.allocated, dec("7"));
        assert_eq!(allocation.shortfall, dec("3"));
        assert!(!allocation.is_fully_allocated());
    }

    #[test]
    fn a_missing_batch_is_all_shortfall() {
        let allocation = allocate::<&'static str>(&[], dec("2.5")).expect("allocation");

        assert!(allocation.lines.is_empty());
        assert_eq!(allocation.allocated, Decimal::ZERO);
        assert_eq!(allocation.shortfall, dec("2.5"));
    }

    #[test]
    fn zero_demand_allocates_nothing() {
        let batches = vec![batch("a", date(2026, 1, 1), None, "5")];

        let allocation = allocate(&batches, Decimal::ZERO).expect("allocation");

        assert!(allocation.lines.is_empty());
        assert!(allocation.is_fully_allocated());
    }

    #[test]
    fn negative_demand_is_rejected() {
        let batches = vec![batch("a", date(2026, 1, 1), None, "5")];

        assert_eq!(
            allocate(&batches, dec("-1")),
            Err(FefoError::NegativeQuantity)
        );
    }

    #[test]
    fn ignores_batches_with_no_quantity_left() {
        let batches = vec![
            batch("depleted", date(2026, 1, 1), Some(date(2026, 1, 1)), "0"),
            batch("active", date(2026, 1, 2), Some(date(2026, 2, 1)), "5"),
        ];

        let allocation = allocate(&batches, dec("1")).expect("allocation");

        assert_eq!(ids(&allocation), vec!["active"]);
    }

    #[test]
    fn the_result_does_not_depend_on_input_order() {
        let batches = vec![
            batch("march", date(2026, 1, 1), Some(date(2026, 3, 1)), "5"),
            batch("january", date(2026, 1, 1), Some(date(2026, 1, 1)), "5"),
            batch("february", date(2026, 1, 1), Some(date(2026, 2, 1)), "5"),
        ];
        let mut reversed = batches.clone();
        reversed.reverse();

        let forward = allocate(&batches, dec("12")).expect("allocation");
        let backward = allocate(&reversed, dec("12")).expect("allocation");

        assert_eq!(forward, backward);
    }
}
