//! Error types for the Pachi EVM layer.

use alloy_primitives::{Address, Bytes, FixedBytes};

/// Errors that can occur during Pachi EVM operations.
#[derive(Debug, thiserror::Error)]
pub enum PachiEvmError {
    /// Oracle precompile error.
    #[error("oracle: {0}")]
    Oracle(#[from] pachi_oracle_precompile::OraclePrecompileError),

    /// VRF precompile error.
    #[error("vrf: {0}")]
    Vrf(#[from] pachi_vrf_precompile::VrfPrecompileError),

    /// Session precompile error.
    #[error("session: {0}")]
    Session(#[from] pachi_session_precompile::SessionPrecompileError),

    /// Sponsor precompile error.
    #[error("sponsor: {0}")]
    Sponsor(#[from] pachi_sponsor_precompile::SponsorPrecompileError),

    /// Unknown function selector for a precompile.
    #[error("unknown selector {selector} for precompile at {address}")]
    UnknownSelector {
        /// The precompile address.
        address: Address,
        /// The unrecognized function selector.
        selector: FixedBytes<4>,
    },

    /// Input too short to contain a function selector.
    #[error("input too short: expected >= 4 bytes, got {len}")]
    InputTooShort {
        /// Actual input length.
        len: usize,
    },

    /// ABI decoding error.
    #[error("abi decode error: {0}")]
    AbiDecode(String),

    /// Static call attempted on a state-modifying function.
    #[error("state-modifying call in static context")]
    StaticCallViolation,
}

impl PachiEvmError {
    /// Encodes the error as ABI-encoded revert data.
    pub fn to_revert_bytes(&self) -> Bytes {
        Bytes::copy_from_slice(self.to_string().as_bytes())
    }
}
