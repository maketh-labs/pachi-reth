//! Pre-execution handler for Pachi custom transaction types.
//!
//! Before EVM execution, validates and prepares state for:
//! - `SessionTx (0x04)`: validate session, check policy, verify signature, set msg.sender =
//!   authorizer
//! - `SponsoredTx (0x05)`: validate sponsor, lock gas balance (Deposit) or check limits (Mint)
//! - `SessionSponsoredTx (0x06)`: session validation first, then sponsor validation

use alloy_primitives::{Address, U256};
use pachi_oracle_precompile::PachiState;
use pachi_session_precompile::{SessionPrecompileError, SessionValidationInput, SessionValidator};
use pachi_sponsor_precompile::{
    SponsorPrecompileError, SponsorSettlement, SponsorValidationInput, SponsorValidator,
};

/// Result of pre-execution validation and setup.
#[derive(Debug, Clone)]
pub enum PreExecutionResult {
    /// `SessionTx`: session validated, msg.sender should be set to authorizer.
    Session {
        /// The authorizer whose address becomes msg.sender.
        authorizer: Address,
    },
    /// `SponsoredTx`: sponsor validated, gas balance locked (for Deposit mode).
    Sponsored {
        /// Sponsor address.
        sponsor: Address,
        /// Amount locked from sponsor balance (0 for Mint mode).
        locked_amount: U256,
    },
    /// `SessionSponsoredTx`: both session and sponsor validated.
    SessionSponsored {
        /// The authorizer whose address becomes msg.sender.
        authorizer: Address,
        /// Sponsor address.
        sponsor: Address,
        /// Amount locked from sponsor balance (0 for Mint mode).
        locked_amount: U256,
    },
}

/// Errors from pre-execution validation.
#[derive(Debug, thiserror::Error)]
pub enum PreExecutionError {
    /// Session validation failed.
    #[error("session validation failed: {0}")]
    Session(#[from] SessionPrecompileError),
    /// Sponsor validation failed.
    #[error("sponsor validation failed: {0}")]
    Sponsor(#[from] SponsorPrecompileError),
}

/// Pre-execution handler: validates and prepares state before EVM execution.
#[derive(Debug)]
pub struct PreExecutionHandler;

impl PreExecutionHandler {
    /// Validates a `SessionTx` before execution.
    ///
    /// Steps:
    /// 1. Validate session (existence, status, nonce, policy)
    /// 2. msg.sender = authorizer (caller pays gas)
    pub fn validate_session_tx(
        state: &impl PachiState,
        input: &SessionValidationInput<'_>,
    ) -> Result<PreExecutionResult, PreExecutionError> {
        SessionValidator::validate(state, input)?;
        Ok(PreExecutionResult::Session { authorizer: input.authorizer })
    }

    /// Validates a `SponsoredTx` before execution.
    ///
    /// Steps:
    /// 1. Validate sponsor (active, policy, limits)
    /// 2. For Deposit mode: lock `max_gas_cost` from sponsor balance
    /// 3. For Mint mode: only check limits (no balance lock)
    pub fn validate_sponsored_tx(
        state: &mut impl PachiState,
        validation: &SponsorValidationInput<'_>,
        config: &pachi_sponsor_precompile::SponsorRecord,
        max_gas_cost: U256,
    ) -> Result<PreExecutionResult, PreExecutionError> {
        SponsorValidator::validate(state, validation, &config.config)?;
        let locked_amount = SponsorSettlement::pre_lock(state, validation.sponsor, max_gas_cost)?;

        Ok(PreExecutionResult::Sponsored { sponsor: validation.sponsor, locked_amount })
    }

    /// Validates a `SessionSponsoredTx` before execution.
    ///
    /// Session validation first, then sponsor validation.
    pub fn validate_session_sponsored_tx(
        state: &mut impl PachiState,
        session_input: &SessionValidationInput<'_>,
        sponsor_input: &SponsorValidationInput<'_>,
        sponsor_config: &pachi_sponsor_precompile::SponsorRecord,
        max_gas_cost: U256,
    ) -> Result<PreExecutionResult, PreExecutionError> {
        // Session validation first
        SessionValidator::validate(state, session_input)?;

        // Then sponsor validation
        SponsorValidator::validate(state, sponsor_input, &sponsor_config.config)?;
        let locked_amount =
            SponsorSettlement::pre_lock(state, sponsor_input.sponsor, max_gas_cost)?;

        Ok(PreExecutionResult::SessionSponsored {
            authorizer: session_input.authorizer,
            sponsor: sponsor_input.sponsor,
            locked_amount,
        })
    }
}
