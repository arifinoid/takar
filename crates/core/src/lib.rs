//! Pure calculation core for Takar.
//!
//! This crate owns every number that must be identical on the client and the
//! server: FEFO allocation, material requirements, HPP, unit conversion, and
//! reorder planning.
//!
//! Rules for anything added here:
//! - no IO, no async, no database access;
//! - deterministic input to output;
//! - `rust_decimal` for money and quantities, never `f64`.

pub mod fefo;
pub mod hpp;
pub mod reorder;
pub mod requirement;

pub use fefo::{allocate, Allocation, AllocationLine, FefoError, StockBatch};
pub use hpp::{production_cost, Consumption, CostError, ProductionCost};
pub use reorder::{average_daily_usage, reorder_plan, ReorderError, ReorderInput, ReorderPlan};
pub use requirement::{material_requirement, RequirementError};
