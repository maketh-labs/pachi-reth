//! VRF precompile errors.

use pachi_vrf_core::VrfError;

/// Errors returned by VRF precompile operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VrfPrecompileError {
    /// Caller is not the system account.
    #[error("VRF_COMPUTE: caller is not system account")]
    NotSystemCaller,

    /// VRF cryptographic error.
    #[error("VRF crypto error: {0}")]
    CryptoError(String),

    /// Duplicate VRF request (same key already exists).
    #[error("duplicate VRF request: key {key}")]
    DuplicateRequest {
        /// The request key.
        key: alloy_primitives::B256,
    },

    /// VRF request not found.
    #[error("VRF request not found: key {key}")]
    RequestNotFound {
        /// The request key.
        key: alloy_primitives::B256,
    },

    /// VRF request already fulfilled.
    #[error("VRF request already fulfilled: key {key}")]
    AlreadyFulfilled {
        /// The request key.
        key: alloy_primitives::B256,
    },

    /// VRF proof verification failed during fulfill.
    #[error("VRF proof verification failed")]
    ProofVerificationFailed,
}

impl From<VrfError> for VrfPrecompileError {
    fn from(err: VrfError) -> Self {
        Self::CryptoError(err.to_string())
    }
}
