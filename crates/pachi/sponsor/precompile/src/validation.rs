//! Pre-execution sponsor validation flow.
//!
//! Validates a sponsored transaction before execution.

use alloy_primitives::{Address, Bytes, FixedBytes, U256};
use pachi_oracle_precompile::PachiState;
use pachi_primitives::{ConstraintEngine, LimitEngine, LimitState};

use crate::{
    error::SponsorPrecompileError,
    hub::SponsorHub,
    record::{SponsorCallPolicy, SponsorConfig, SponsorTransferPolicy},
    storage_keys::{sponsor_limit_key, sponsor_sender_limit_key, SPONSOR_HUB_ADDRESS},
};

/// Input for sponsor validation.
#[derive(Debug)]
pub struct SponsorValidationInput<'a> {
    /// Sponsor address.
    pub sponsor: Address,
    /// Transaction sender.
    pub sender: Address,
    /// Transaction target.
    pub to: Address,
    /// Transaction calldata.
    pub calldata: &'a [u8],
    /// Transaction gas limit.
    pub gas_limit: U256,
    /// Current block timestamp.
    pub block_timestamp: u64,
}

/// Validates a sponsored transaction against the sponsor's policy.
#[derive(Debug)]
pub struct SponsorValidator;

impl SponsorValidator {
    /// Validates a sponsored transaction.
    ///
    /// Steps:
    /// 1. Sponsor exists and is active
    /// 2. Policy not expired
    /// 3. Sender in allowed list (if restricted)
    /// 4. Call/transfer policy matching + constraints
    /// 5. Gas and fee limit checks
    /// 6. Tx count limit checks
    pub fn validate(
        state: &impl PachiState,
        input: &SponsorValidationInput<'_>,
        config: &SponsorConfig,
    ) -> Result<(), SponsorPrecompileError> {
        // Step 1: Active check
        if !SponsorHub::is_active(state, input.sponsor) {
            return Err(SponsorPrecompileError::SponsorNotActive { sponsor: input.sponsor });
        }

        // Step 2: Expiration check
        if input.block_timestamp > config.valid_until {
            return Err(SponsorPrecompileError::PolicyExpired { sponsor: input.sponsor });
        }

        // Step 3: Allowed senders check
        if !config.allowed_senders.is_empty() && !config.allowed_senders.contains(&input.sender) {
            return Err(SponsorPrecompileError::SenderNotAllowed { sender: input.sender });
        }

        // Step 4: Call/transfer policy matching
        if input.calldata.len() >= 4 {
            Self::validate_call(state, input, &config.call_policies)?;
        } else if !config.transfer_policies.is_empty() {
            Self::validate_transfer(input, &config.transfer_policies)?;
        }

        // Step 5: Gas limit check
        if input.gas_limit > config.max_gas_per_tx {
            return Err(SponsorPrecompileError::GasExceedsMax {
                max: config.max_gas_per_tx,
                actual: input.gas_limit,
            });
        }

        // Step 6: Fee and tx count limit checks (read-only validation)
        Self::check_limits(state, input, config)?;

        Ok(())
    }

    /// Validates a contract call against sponsor call policies.
    fn validate_call(
        _state: &impl PachiState,
        input: &SponsorValidationInput<'_>,
        call_policies: &[SponsorCallPolicy],
    ) -> Result<(), SponsorPrecompileError> {
        let selector = FixedBytes::<4>::from_slice(&input.calldata[..4]);

        let policy = call_policies
            .iter()
            .find(|p| p.target == input.to && p.selector == selector)
            .ok_or(SponsorPrecompileError::CallNotAllowed { target: input.to, selector })?;

        // Check constraints
        let calldata_bytes = Bytes::copy_from_slice(input.calldata);
        for constraint in &policy.constraints {
            let mut dummy_limit_state = LimitState { used: U256::ZERO, last_window: 0 };
            ConstraintEngine::verify_constraint(
                constraint,
                &calldata_bytes,
                &mut dummy_limit_state,
                input.block_timestamp,
            )?;
        }

        Ok(())
    }

    /// Validates a transfer against sponsor transfer policies.
    fn validate_transfer(
        input: &SponsorValidationInput<'_>,
        transfer_policies: &[SponsorTransferPolicy],
    ) -> Result<(), SponsorPrecompileError> {
        let _policy = transfer_policies
            .iter()
            .find(|p| p.target == input.to)
            .ok_or(SponsorPrecompileError::TransferNotAllowed { target: input.to })?;

        Ok(())
    }

    /// Checks fee and tx count limits (read-only, no state updates).
    fn check_limits(
        state: &impl PachiState,
        input: &SponsorValidationInput<'_>,
        config: &SponsorConfig,
    ) -> Result<(), SponsorPrecompileError> {
        // Global fee limit
        let global_fee_slot = sponsor_limit_key(input.sponsor, b"global_fee");
        let mut global_fee_state = read_limit_state(state, global_fee_slot);
        // We use gas_limit as the estimate for pre-validation
        let estimated_cost = input.gas_limit;
        LimitEngine::check_and_update(
            &config.global_fee_limit,
            &mut global_fee_state,
            estimated_cost,
            input.block_timestamp,
        )?;

        // Per-sender fee limit
        let sender_fee_slot = sponsor_sender_limit_key(input.sponsor, input.sender, b"sender_fee");
        let mut sender_fee_state = read_limit_state(state, sender_fee_slot);
        LimitEngine::check_and_update(
            &config.per_sender_fee_limit,
            &mut sender_fee_state,
            estimated_cost,
            input.block_timestamp,
        )?;

        // Global tx limit
        let global_tx_slot = sponsor_limit_key(input.sponsor, b"global_tx");
        let mut global_tx_state = read_limit_state(state, global_tx_slot);
        LimitEngine::check_and_update(
            &config.global_tx_limit,
            &mut global_tx_state,
            U256::from(1),
            input.block_timestamp,
        )?;

        // Per-sender tx limit
        let sender_tx_slot = sponsor_sender_limit_key(input.sponsor, input.sender, b"sender_tx");
        let mut sender_tx_state = read_limit_state(state, sender_tx_slot);
        LimitEngine::check_and_update(
            &config.per_sender_tx_limit,
            &mut sender_tx_state,
            U256::from(1),
            input.block_timestamp,
        )?;

        Ok(())
    }
}

/// Reads limit state from two consecutive storage slots.
fn read_limit_state(state: &impl PachiState, slot: U256) -> LimitState {
    let used = state.get_storage(SPONSOR_HUB_ADDRESS, slot);
    let last_window_val = state.get_storage(SPONSOR_HUB_ADDRESS, slot + U256::from(1));
    LimitState { used, last_window: last_window_val.as_limbs()[0] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hub::SponsorHub, record::SponsorConfig};
    use pachi_oracle_precompile::MockState;
    use pachi_primitives::Limit;

    fn default_config() -> SponsorConfig {
        SponsorConfig {
            allowed_senders: vec![],
            call_policies: vec![],
            transfer_policies: vec![],
            global_fee_limit: Limit::unlimited(),
            per_sender_fee_limit: Limit::unlimited(),
            max_gas_per_tx: U256::from(1_000_000u64),
            global_tx_limit: Limit::unlimited(),
            per_sender_tx_limit: Limit::unlimited(),
            valid_until: 100_000,
        }
    }

    fn setup(state: &mut MockState) -> Address {
        let sponsor = Address::from([0xAAu8; 20]);
        SponsorHub::register_policy(state, sponsor, &default_config(), 100).unwrap();
        sponsor
    }

    fn test_sender() -> Address {
        Address::from([0xBBu8; 20])
    }

    #[test]
    fn validate_basic_sponsored_tx() {
        let mut state = MockState::new();
        let sponsor = setup(&mut state);

        let input = SponsorValidationInput {
            sponsor,
            sender: test_sender(),
            to: Address::from([0xCCu8; 20]),
            calldata: &[],
            gas_limit: U256::from(100_000u64),
            block_timestamp: 200,
        };

        SponsorValidator::validate(&state, &input, &default_config()).unwrap();
    }

    #[test]
    fn validate_inactive_sponsor_fails() {
        let mut state = MockState::new();
        let sponsor = setup(&mut state);
        SponsorHub::deactivate_policy(&mut state, sponsor).unwrap();

        let input = SponsorValidationInput {
            sponsor,
            sender: test_sender(),
            to: Address::ZERO,
            calldata: &[],
            gas_limit: U256::from(100_000u64),
            block_timestamp: 200,
        };

        let err = SponsorValidator::validate(&state, &input, &default_config()).unwrap_err();
        assert!(matches!(err, SponsorPrecompileError::SponsorNotActive { .. }));
    }

    #[test]
    fn validate_expired_policy_fails() {
        let mut state = MockState::new();
        let sponsor = setup(&mut state);

        let input = SponsorValidationInput {
            sponsor,
            sender: test_sender(),
            to: Address::ZERO,
            calldata: &[],
            gas_limit: U256::from(100_000u64),
            block_timestamp: 200_000, // past valid_until=100_000
        };

        let err = SponsorValidator::validate(&state, &input, &default_config()).unwrap_err();
        assert!(matches!(err, SponsorPrecompileError::PolicyExpired { .. }));
    }

    #[test]
    fn validate_sender_not_allowed() {
        let mut state = MockState::new();
        let sponsor = setup(&mut state);

        let config = SponsorConfig {
            allowed_senders: vec![Address::from([0x01u8; 20])], // only 0x01 allowed
            ..default_config()
        };
        // Re-register with restricted senders
        SponsorHub::register_policy(&mut state, sponsor, &config, 100).unwrap();

        let input = SponsorValidationInput {
            sponsor,
            sender: test_sender(), // 0xBB not in allowed list
            to: Address::ZERO,
            calldata: &[],
            gas_limit: U256::from(100_000u64),
            block_timestamp: 200,
        };

        let err = SponsorValidator::validate(&state, &input, &config).unwrap_err();
        assert!(matches!(err, SponsorPrecompileError::SenderNotAllowed { .. }));
    }

    #[test]
    fn validate_gas_exceeds_max() {
        let mut state = MockState::new();
        let sponsor = setup(&mut state);

        let input = SponsorValidationInput {
            sponsor,
            sender: test_sender(),
            to: Address::ZERO,
            calldata: &[],
            gas_limit: U256::from(2_000_000u64), // exceeds max 1_000_000
            block_timestamp: 200,
        };

        let err = SponsorValidator::validate(&state, &input, &default_config()).unwrap_err();
        assert!(matches!(err, SponsorPrecompileError::GasExceedsMax { .. }));
    }

    #[test]
    fn validate_call_policy_matching() {
        let mut state = MockState::new();
        let sponsor = setup(&mut state);
        let target = Address::from([0xCCu8; 20]);
        let selector = [0xDE, 0xAD, 0xBE, 0xEF];

        let config = SponsorConfig {
            call_policies: vec![SponsorCallPolicy {
                target,
                selector: FixedBytes::from(selector),
                constraints: vec![],
            }],
            ..default_config()
        };

        let input = SponsorValidationInput {
            sponsor,
            sender: test_sender(),
            to: target,
            calldata: &selector,
            gas_limit: U256::from(100_000u64),
            block_timestamp: 200,
        };

        SponsorValidator::validate(&state, &input, &config).unwrap();
    }

    #[test]
    fn validate_call_not_in_policy_fails() {
        let mut state = MockState::new();
        let sponsor = setup(&mut state);

        let config = SponsorConfig {
            call_policies: vec![SponsorCallPolicy {
                target: Address::from([0xCCu8; 20]),
                selector: FixedBytes::from([0xDE, 0xAD, 0xBE, 0xEF]),
                constraints: vec![],
            }],
            ..default_config()
        };

        let input = SponsorValidationInput {
            sponsor,
            sender: test_sender(),
            to: Address::from([0xDDu8; 20]), // different target
            calldata: &[0xDE, 0xAD, 0xBE, 0xEF],
            gas_limit: U256::from(100_000u64),
            block_timestamp: 200,
        };

        let err = SponsorValidator::validate(&state, &input, &config).unwrap_err();
        assert!(matches!(err, SponsorPrecompileError::CallNotAllowed { .. }));
    }
}
