//! Custom transaction validation for Pachi tx types in the mempool.

use alloy_primitives::{Address, B256, U256};
use pachi_oracle_precompile::PachiState;
use pachi_session_precompile::{read_session_record, SessionNonce, SessionStatus};
use pachi_sponsor_precompile::{
    storage_keys as sponsor_keys, SponsorStatus, SponsorType, SPONSOR_HUB_ADDRESS,
};
use pachi_tx::PachiTxType;

/// Errors from pool-level validation of Pachi custom tx types.
#[derive(Debug, thiserror::Error)]
pub enum PachiTxValidationError {
    /// Session does not exist or is not active.
    #[error("session not found or inactive: {0}")]
    SessionInvalid(String),

    /// Session has expired.
    #[error("session expired")]
    SessionExpired,

    /// Session nonce mismatch.
    #[error("session nonce mismatch: expected {expected}, got {got}")]
    SessionNonceMismatch {
        /// Expected nonce.
        expected: u64,
        /// Actual nonce in tx.
        got: u64,
    },

    /// Sponsor is not active or policy check failed.
    #[error("sponsor validation failed: {0}")]
    SponsorInvalid(String),

    /// Sponsor has insufficient balance (deposit mode).
    #[error("sponsor has insufficient balance")]
    SponsorInsufficientBalance,

    /// Unknown or unsupported custom tx type.
    #[error("unsupported Pachi tx type: 0x{0:02x}")]
    UnsupportedTxType(u8),
}

/// Pool-level validator for Pachi custom transaction types.
///
/// Performs lightweight validation before transactions enter the mempool.
/// Full validation (EVM execution, handler logic) happens at block building time.
#[derive(Debug)]
pub struct PachiPoolValidator;

impl PachiPoolValidator {
    /// Validates a `SessionTx` for mempool acceptance.
    ///
    /// Checks:
    /// 1. Session exists and is Active
    /// 2. Session not expired
    /// 3. Authorizer matches
    /// 4. Session nonce correct
    pub fn validate_session_tx(
        state: &impl PachiState,
        session_hash: &B256,
        authorizer: Address,
        tx_nonce: u64,
        block_timestamp: u64,
    ) -> Result<(), PachiTxValidationError> {
        // 1. Session exists
        let record = read_session_record(state, session_hash)
            .ok_or_else(|| PachiTxValidationError::SessionInvalid("not found".to_string()))?;

        // 2. Status check using effective_status (handles revoked + expired)
        let effective = record.effective_status(block_timestamp);
        match effective {
            SessionStatus::Active => {}
            SessionStatus::Revoked => {
                return Err(PachiTxValidationError::SessionInvalid("revoked".to_string()));
            }
            SessionStatus::Expired => {
                return Err(PachiTxValidationError::SessionExpired);
            }
        }

        // 3. Authorizer matches
        if record.authorizer != authorizer {
            return Err(PachiTxValidationError::SessionInvalid("authorizer mismatch".to_string()));
        }

        // 4. Nonce check
        let current_nonce = SessionNonce::get(state, authorizer, session_hash);
        if tx_nonce != current_nonce {
            return Err(PachiTxValidationError::SessionNonceMismatch {
                expected: current_nonce,
                got: tx_nonce,
            });
        }

        Ok(())
    }

    /// Validates a `SponsoredTx` for mempool acceptance.
    ///
    /// Checks:
    /// 1. Sponsor is active
    /// 2. Sponsor policy not expired
    /// 3. Balance sufficient (deposit mode)
    pub fn validate_sponsored_tx(
        state: &impl PachiState,
        sponsor: Address,
        block_timestamp: u64,
        estimated_gas_cost: U256,
    ) -> Result<(), PachiTxValidationError> {
        // Read sponsor record (5 slots: status, type, balance, created_at, valid_until)
        let base = sponsor_keys::sponsor_record_key(sponsor);
        let status = state.get_storage(SPONSOR_HUB_ADDRESS, base).as_limbs()[0] as u8;
        let sponsor_type =
            state.get_storage(SPONSOR_HUB_ADDRESS, base + U256::from(1)).as_limbs()[0] as u8;
        let balance = state.get_storage(SPONSOR_HUB_ADDRESS, base + U256::from(2));
        let created_at = state.get_storage(SPONSOR_HUB_ADDRESS, base + U256::from(3));
        let valid_until =
            state.get_storage(SPONSOR_HUB_ADDRESS, base + U256::from(4)).as_limbs()[0];

        // 0. Existence check: created_at == 0 means no record
        if created_at.is_zero() {
            return Err(PachiTxValidationError::SponsorInvalid("not found".to_string()));
        }

        // 1. Active check (Active=0, Deactivated=1)
        if status != SponsorStatus::Active as u8 {
            return Err(PachiTxValidationError::SponsorInvalid(format!("status={}", status)));
        }

        // 2. Expiry
        if valid_until != 0 && block_timestamp > valid_until {
            return Err(PachiTxValidationError::SponsorInvalid("expired".to_string()));
        }

        // 3. Balance (deposit mode only)
        if sponsor_type == SponsorType::Deposit as u8 && balance < estimated_gas_cost {
            return Err(PachiTxValidationError::SponsorInsufficientBalance);
        }

        Ok(())
    }

    /// Validates a `SessionSponsoredTx` for mempool acceptance.
    ///
    /// Validates session first, then sponsor.
    pub fn validate_session_sponsored_tx(
        state: &impl PachiState,
        session_hash: &B256,
        authorizer: Address,
        tx_nonce: u64,
        sponsor: Address,
        block_timestamp: u64,
        estimated_gas_cost: U256,
    ) -> Result<(), PachiTxValidationError> {
        Self::validate_session_tx(state, session_hash, authorizer, tx_nonce, block_timestamp)?;
        Self::validate_sponsored_tx(state, sponsor, block_timestamp, estimated_gas_cost)?;
        Ok(())
    }

    /// Returns `true` if the given tx type byte is a Pachi custom type that
    /// requires pool-level validation.
    pub const fn is_pachi_type(ty: u8) -> bool {
        PachiTxType::is_pachi_type(ty)
    }
}
