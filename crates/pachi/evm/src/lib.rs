//! Pachi Chain EVM configuration.
//!
//! This crate wires the Layer 1 precompile state machines and Layer 2 transaction types
//! into a complete EVM configuration via [`PachiEvmConfig`].
//!
//! # Components
//!
//! - [`PachiEvmFactory`]: Custom [`EvmFactory`] that creates EVM instances with all 5 Pachi
//!   precompiles registered.
//! - [`PachiEvmConfig`]: Implements [`ConfigureEvm`] by wrapping the standard Ethereum block
//!   executor with [`PachiEvmFactory`].
//! - Precompile dispatch: ABI routing for `SessionRegistry`, `SponsorHub`, `PriceOracle`,
//!   `VRF_COMPUTE`, `VRF_VERIFY`.
//! - Pre/post execution handlers: Session validation, sponsor settlement, nonce/limit updates.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod config;
mod error;
pub(crate) mod events;
mod factory;
pub mod handlers;
mod precompiles;
pub(crate) mod state_bridge;

pub use config::PachiEvmConfig;
pub use error::PachiEvmError;
pub use factory::PachiEvmFactory;
pub use handlers::{PostExecutionHandler, PreExecutionHandler};
pub use precompiles::pachi_precompiles;

#[cfg(test)]
mod tests;
