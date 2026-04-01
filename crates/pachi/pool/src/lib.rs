//! Pachi Chain transaction pool extensions.
//!
//! Provides custom mempool validation for Pachi transaction types:
//! - `SessionTx (0x04)`: session existence, expiry, signer, nonce, policy
//! - `SponsoredTx (0x05)`: sponsor active, policy valid, balance/limits
//! - `SessionSponsoredTx (0x06)`: combined session + sponsor validation

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod state_bridge;
mod validator;

pub use state_bridge::ProviderStateBridge;
pub use validator::{PachiPoolValidator, PachiTxValidationError};

#[cfg(test)]
mod tests;
