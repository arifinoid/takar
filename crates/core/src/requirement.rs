//! Material requirements for a production run.
//!
//! One recipe version describes a batch: `yield_qty` units of product from a
//! set of material quantities. Producing `production_qty` units scales every
//! line by `production_qty / yield_qty`, and waste is added on top:
//!
//! ```text
//! requirement = qty × (1 + waste_pct / 100) × (production_qty / yield_qty)
//! ```
//!
//! The result rounds **up** to four decimals. Rounding down would plan less
//! material than the recipe needs, and a shortage found halfway through
//! production is worse than a fraction of a gram left over.

use std::fmt;

use rust_decimal::{Decimal, RoundingStrategy};

/// Quantities are stored as `numeric(18,4)` in Postgres.
const SCALE: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequirementError {
    /// A recipe must produce something; dividing by zero has no meaning.
    InvalidYield,
    /// Negative material or production quantities cannot be planned.
    NegativeQuantity,
    /// Waste is a percentage between 0 and 100.
    InvalidWaste,
}

impl fmt::Display for RequirementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RequirementError::InvalidYield => {
                formatter.write_str("yield quantity must be positive")
            }
            RequirementError::NegativeQuantity => {
                formatter.write_str("quantity cannot be negative")
            }
            RequirementError::InvalidWaste => {
                formatter.write_str("waste percentage must be between 0 and 100")
            }
        }
    }
}

impl std::error::Error for RequirementError {}

/// How much of one material a production run needs.
///
/// `qty` is the amount in the recipe for one batch (base unit), `waste_pct` is
/// the expected loss for that material, `production_qty` is how much product is
/// being made, and `yield_qty` is how much one batch of the recipe produces.
pub fn material_requirement(
    qty: Decimal,
    waste_pct: Decimal,
    production_qty: Decimal,
    yield_qty: Decimal,
) -> Result<Decimal, RequirementError> {
    if yield_qty <= Decimal::ZERO {
        return Err(RequirementError::InvalidYield);
    }

    if qty.is_sign_negative() || production_qty.is_sign_negative() {
        return Err(RequirementError::NegativeQuantity);
    }

    if waste_pct.is_sign_negative() || waste_pct > Decimal::from(100) {
        return Err(RequirementError::InvalidWaste);
    }

    let with_waste = qty * (Decimal::ONE + waste_pct / Decimal::from(100));
    let scaled = with_waste * production_qty / yield_qty;

    Ok(scaled.round_dp_with_strategy(SCALE, RoundingStrategy::ToPositiveInfinity))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(value: &str) -> Decimal {
        value.parse().expect("decimal")
    }

    #[test]
    fn a_plain_recipe_scales_with_the_production_quantity() {
        let requirement =
            material_requirement(dec("2.5"), dec("0"), dec("10"), dec("10")).expect("requirement");

        assert_eq!(requirement, dec("2.5"));
    }

    #[test]
    fn waste_is_added_on_top_of_the_recipe_quantity() {
        let requirement =
            material_requirement(dec("2"), dec("10"), dec("1"), dec("1")).expect("requirement");

        assert_eq!(requirement, dec("2.2"));
    }

    #[test]
    fn a_hundred_percent_waste_doubles_the_requirement() {
        let requirement =
            material_requirement(dec("1"), dec("100"), dec("1"), dec("1")).expect("requirement");

        assert_eq!(requirement, dec("2"));
    }

    #[test]
    fn production_over_yield_scales_the_line() {
        let requirement =
            material_requirement(dec("1"), dec("0"), dec("5"), dec("2")).expect("requirement");

        assert_eq!(requirement, dec("2.5"));
    }

    #[test]
    fn rounding_never_plans_less_than_the_recipe_needs() {
        let third =
            material_requirement(dec("1"), dec("0"), dec("1"), dec("3")).expect("requirement");
        let two_thirds =
            material_requirement(dec("1"), dec("0"), dec("2"), dec("3")).expect("requirement");

        assert_eq!(third, dec("0.3334"), "0.3333... rounds up");
        assert_eq!(two_thirds, dec("0.6667"), "0.6666... rounds up");
    }

    #[test]
    fn a_tiny_fraction_still_rounds_up_to_the_smallest_unit() {
        let requirement =
            material_requirement(dec("1"), dec("0"), dec("1"), dec("100000")).expect("requirement");

        assert_eq!(
            requirement,
            dec("0.0001"),
            "never round a requirement to zero"
        );
    }

    #[test]
    fn zero_production_needs_nothing() {
        let requirement =
            material_requirement(dec("2"), dec("5"), dec("0"), dec("10")).expect("requirement");

        assert_eq!(requirement, Decimal::ZERO);
    }

    #[test]
    fn a_zero_or_negative_yield_is_rejected() {
        assert_eq!(
            material_requirement(dec("1"), dec("0"), dec("1"), Decimal::ZERO),
            Err(RequirementError::InvalidYield)
        );
        assert_eq!(
            material_requirement(dec("1"), dec("0"), dec("1"), dec("-1")),
            Err(RequirementError::InvalidYield)
        );
    }

    #[test]
    fn negative_quantities_are_rejected() {
        assert_eq!(
            material_requirement(dec("-1"), dec("0"), dec("1"), dec("1")),
            Err(RequirementError::NegativeQuantity)
        );
        assert_eq!(
            material_requirement(dec("1"), dec("0"), dec("-1"), dec("1")),
            Err(RequirementError::NegativeQuantity)
        );
    }

    #[test]
    fn waste_outside_zero_to_hundred_is_rejected() {
        assert_eq!(
            material_requirement(dec("1"), dec("-1"), dec("1"), dec("1")),
            Err(RequirementError::InvalidWaste)
        );
        assert_eq!(
            material_requirement(dec("1"), dec("101"), dec("1"), dec("1")),
            Err(RequirementError::InvalidWaste)
        );
    }
}
