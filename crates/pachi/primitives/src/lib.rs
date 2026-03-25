//! Shared primitive types for Pachi Chain.
//!
//! This crate defines types and engines shared across all Pachi features:
//! - Limit model (rate limiting for sessions and sponsors)
//! - Constraint model (argument-level call policy enforcement)
//! - Session and sponsor policy types
//! - Oracle types (asset identifiers, confidence levels, price snapshots)
//! - System-level constants

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod asset;
mod confidence;
mod constants;
mod constraint;
mod limit;
mod policy;
mod snapshot;

pub use asset::AssetId;
pub use confidence::Confidence;
pub use constants::*;
pub use constraint::{ConditionType, Constraint, ConstraintEngine};
pub use limit::{Limit, LimitEngine, LimitState, LimitType};
pub use policy::{CallPolicy, TransferPolicy};
pub use snapshot::{AssetState, PriceSnapshot, SnapshotEntry};
