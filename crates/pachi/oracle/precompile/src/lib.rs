//! `PriceOracle` precompile state machine for Pachi Chain.
//!
//! Implements storage layout and state operations for the `PriceOracle` at `0x0802`.
//! This crate handles read/write logic independent of the EVM wiring (Layer 3).
//!
//! # Storage Layout
//!
//! Two slots per asset:
//! - `slot(asset_id * 2)` → price: `U256` (8 decimals)
//! - `slot(asset_id * 2 + 1)` → packed `{ timestamp: u64 [0:63], confidence: u8 [64:71] }`

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod circuit_breaker;
mod error;
mod gas;
mod state;
mod storage;

pub use circuit_breaker::CircuitBreaker;
pub use error::OraclePrecompileError;
pub use gas::{
    gas_get_price_batch, GAS_GET_PRICE, GAS_GET_PRICE_BATCH_BASE, GAS_GET_PRICE_BATCH_PER_ASSET,
    GAS_IS_SUPPORTED,
};
#[cfg(any(test, feature = "test-utils"))]
pub use state::MockState;
pub use state::PachiState;
pub use storage::{
    apply_oracle_update, get_price, get_price_batch, is_supported, PriceEntry,
    ORACLE_PRECOMPILE_ADDRESS,
};
