//! Session precompile errors.

use alloy_primitives::{Address, B256};
use pachi_primitives::{ConstraintError, LimitError};

/// Errors returned by session precompile operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionPrecompileError {
    /// Session hash not found.
    #[error("session not found: {hash}")]
    SessionNotFound {
        /// The session hash.
        hash: B256,
    },

    /// Session has been revoked.
    #[error("session revoked: {hash}")]
    SessionRevoked {
        /// The session hash.
        hash: B256,
    },

    /// Session has expired.
    #[error("session expired: {hash}")]
    SessionExpired {
        /// The session hash.
        hash: B256,
    },

    /// Caller is not the session's authorizer.
    #[error("not authorizer: expected {expected}, got {actual}")]
    NotAuthorizer {
        /// Expected authorizer.
        expected: Address,
        /// Actual caller.
        actual: Address,
    },

    /// Maximum sessions per account reached.
    #[error("max sessions reached for {authorizer}")]
    MaxSessionsReached {
        /// The authorizer address.
        authorizer: Address,
    },

    /// Session nonce mismatch.
    #[error("nonce mismatch: expected {expected}, got {actual}")]
    NonceMismatch {
        /// Expected nonce.
        expected: u64,
        /// Actual nonce from tx.
        actual: u64,
    },

    /// Session hash does not match the provided config.
    #[error("session hash mismatch: expected {expected}, got {actual}")]
    HashMismatch {
        /// Expected hash (from on-chain record).
        expected: B256,
        /// Actual hash (computed from provided config).
        actual: B256,
    },

    /// Call target/selector not allowed by the session's call policies.
    #[error("call not allowed: target={target}, selector={selector}")]
    CallNotAllowed {
        /// Target contract.
        target: Address,
        /// Function selector.
        selector: alloy_primitives::FixedBytes<4>,
    },

    /// Transfer target not allowed by the session's transfer policies.
    #[error("transfer not allowed: target={target}")]
    TransferNotAllowed {
        /// Transfer recipient.
        target: Address,
    },

    /// Native value exceeds per-use maximum.
    #[error("value per use exceeded: max={max}, actual={actual}")]
    ValuePerUseExceeded {
        /// Max allowed per use.
        max: alloy_primitives::U256,
        /// Actual value.
        actual: alloy_primitives::U256,
    },

    /// Limit check failed.
    #[error("limit error: {0}")]
    LimitError(#[from] LimitError),

    /// Constraint check failed.
    #[error("constraint error: {0}")]
    ConstraintError(#[from] ConstraintError),

    /// Fee limit exceeded.
    #[error("fee limit exceeded")]
    FeeLimitExceeded,
}
