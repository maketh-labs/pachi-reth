//! VRF error types.

use thiserror::Error;

/// Errors produced by VRF operations.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum VrfError {
    /// Failed to hash seed to a valid curve point after max attempts.
    #[error("hash-to-curve failed: no valid point found after {max_attempts} attempts")]
    HashToCurveFailed {
        /// Maximum number of attempts tried.
        max_attempts: u32,
    },
    /// The provided proof failed verification.
    #[error("proof verification failed")]
    InvalidProof,
    /// The secret key bytes are invalid.
    #[error("invalid secret key")]
    InvalidSecretKey,
    /// The public key bytes are invalid or not on curve.
    #[error("invalid public key")]
    InvalidPublicKey,
    /// The proof bytes could not be decoded.
    #[error("malformed proof: {reason}")]
    MalformedProof {
        /// Description of what was wrong.
        reason: &'static str,
    },
}
