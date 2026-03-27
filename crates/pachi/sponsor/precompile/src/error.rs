//! Sponsor precompile errors.

use alloy_primitives::{Address, U256};

/// Errors returned by sponsor precompile operations.
#[derive(Debug, thiserror::Error)]
pub enum SponsorPrecompileError {
    /// Sponsor not found.
    #[error("sponsor not found: {sponsor}")]
    SponsorNotFound {
        /// The sponsor address.
        sponsor: Address,
    },

    /// Sponsor is not active.
    #[error("sponsor not active: {sponsor}")]
    SponsorNotActive {
        /// The sponsor address.
        sponsor: Address,
    },

    /// Sponsor policy has expired.
    #[error("sponsor policy expired: {sponsor}")]
    PolicyExpired {
        /// The sponsor address.
        sponsor: Address,
    },

    /// Insufficient balance for deposit-mode sponsor.
    #[error("insufficient balance: have {balance}, need {required}")]
    InsufficientBalance {
        /// Current balance.
        balance: U256,
        /// Required amount.
        required: U256,
    },

    /// Mint-mode sponsor cannot deposit/withdraw.
    #[error("operation not allowed for mint-mode sponsor")]
    MintModeNotAllowed,

    /// Deposit-mode sponsor cannot withdraw more than available.
    #[error("withdraw exceeds balance: have {balance}, requested {amount}")]
    WithdrawExceedsBalance {
        /// Current balance.
        balance: U256,
        /// Requested withdrawal.
        amount: U256,
    },

    /// Caller is not the `SponsorHub` owner.
    #[error("not owner: expected {expected}, got {actual}")]
    NotOwner {
        /// Expected owner.
        expected: Address,
        /// Actual caller.
        actual: Address,
    },

    /// Sender not in `allowed_senders` list.
    #[error("sender not allowed: {sender}")]
    SenderNotAllowed {
        /// The rejected sender.
        sender: Address,
    },

    /// Call target/selector not in policy.
    #[error("call not allowed: target={target}, selector={selector}")]
    CallNotAllowed {
        /// Target contract.
        target: Address,
        /// Function selector.
        selector: alloy_primitives::FixedBytes<4>,
    },

    /// Transfer target not in policy.
    #[error("transfer not allowed: target={target}")]
    TransferNotAllowed {
        /// Transfer recipient.
        target: Address,
    },

    /// Gas exceeds `max_gas_per_tx`.
    #[error("gas exceeds max per tx: max={max}, actual={actual}")]
    GasExceedsMax {
        /// Max gas per tx.
        max: U256,
        /// Actual gas.
        actual: U256,
    },

    /// Limit check failed.
    #[error("limit error: {0}")]
    LimitError(#[from] pachi_primitives::LimitError),

    /// Constraint check failed.
    #[error("constraint error: {0}")]
    ConstraintError(#[from] pachi_primitives::ConstraintError),

    /// Sponsor must register a policy first before mint approval.
    #[error("sponsor has no policy: {sponsor}")]
    NoPolicyRegistered {
        /// The sponsor address.
        sponsor: Address,
    },
}
