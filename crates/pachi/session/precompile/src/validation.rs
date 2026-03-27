//! Full session validation flow (Steps 1-7 from spec).
//!
//! Used during `SessionTx` execution to validate the transaction
//! against the session's policy.

use alloy_primitives::{Address, Bytes, FixedBytes, B256, U256};
use pachi_oracle_precompile::PachiState;
use pachi_primitives::{CallPolicy, ConstraintEngine, LimitEngine, LimitState, TransferPolicy};

use crate::{
    error::SessionPrecompileError,
    nonce::SessionNonce,
    record::SessionStatus,
    registry::SessionRegistry,
    storage_keys::{limit_state_key, SESSION_REGISTRY_ADDRESS},
};

/// Session validation context.
#[derive(Debug)]
pub struct SessionValidationInput<'a> {
    /// The session hash.
    pub session_hash: &'a B256,
    /// The authorizer address.
    pub authorizer: Address,
    /// Transaction nonce (session nonce).
    pub nonce: u64,
    /// Transaction target address.
    pub to: Address,
    /// Transaction calldata.
    pub calldata: &'a [u8],
    /// Transaction value (native token).
    pub value: U256,
    /// Estimated gas cost for fee limit checking.
    pub estimated_gas_cost: U256,
    /// Current block timestamp.
    pub block_timestamp: u64,
    /// Call policies from the session config.
    pub call_policies: &'a [CallPolicy],
    /// Transfer policies from the session config.
    pub transfer_policies: &'a [TransferPolicy],
    /// Fee limit from the session config.
    pub fee_limit: &'a pachi_primitives::Limit,
}

/// Session validator implementing the spec's 7-step flow.
#[derive(Debug)]
pub struct SessionValidator;

impl SessionValidator {
    /// Validates a session transaction against the session's policy.
    ///
    /// Steps from spec:
    /// 1. Session hash existence check
    /// 2. Status check (revoked/expired)
    /// 3. Signer verification is done at the caller level (not here)
    /// 4. Nonce check
    /// 5. Policy hash verification is done at the caller level (not here)
    /// 6. Fee limit check
    /// 7. Call/transfer policy validation
    pub fn validate(
        state: &impl PachiState,
        input: &SessionValidationInput<'_>,
    ) -> Result<(), SessionPrecompileError> {
        // Step 1 + 2: Session existence and status
        let record = SessionRegistry::get_session(state, input.session_hash)?;
        let effective = record.effective_status(input.block_timestamp);
        match effective {
            SessionStatus::Active => {}
            SessionStatus::Revoked => {
                return Err(SessionPrecompileError::SessionRevoked { hash: *input.session_hash });
            }
            SessionStatus::Expired => {
                return Err(SessionPrecompileError::SessionExpired { hash: *input.session_hash });
            }
        }

        // Step 4: Nonce check
        if !SessionNonce::check(state, input.authorizer, input.session_hash, input.nonce) {
            let expected = SessionNonce::get(state, input.authorizer, input.session_hash);
            return Err(SessionPrecompileError::NonceMismatch { expected, actual: input.nonce });
        }

        // Step 6: Fee limit check
        Self::check_fee_limit(state, input)?;

        // Step 7: Call type branching
        if input.calldata.len() >= 4 {
            Self::validate_call(state, input)?;
        } else {
            Self::validate_transfer(state, input)?;
        }

        Ok(())
    }

    /// Checks fee limit.
    fn check_fee_limit(
        state: &impl PachiState,
        input: &SessionValidationInput<'_>,
    ) -> Result<(), SessionPrecompileError> {
        let context = b"fee";
        let slot = limit_state_key(input.session_hash, context, 0);
        let mut limit_state = read_limit_state(state, slot);

        LimitEngine::check_and_update(
            input.fee_limit,
            &mut limit_state,
            input.estimated_gas_cost,
            input.block_timestamp,
        )
        .map_err(|_| SessionPrecompileError::FeeLimitExceeded)?;

        // Note: actual limit state update happens in post-execution
        Ok(())
    }

    /// Validates a contract call against call policies.
    fn validate_call(
        state: &impl PachiState,
        input: &SessionValidationInput<'_>,
    ) -> Result<(), SessionPrecompileError> {
        let selector = FixedBytes::<4>::from_slice(&input.calldata[..4]);

        // Find matching call policy
        let policy = input
            .call_policies
            .iter()
            .find(|p| p.target == input.to && p.selector == selector)
            .ok_or(SessionPrecompileError::CallNotAllowed { target: input.to, selector })?;

        // Check per-use value limit
        if input.value > policy.max_value_per_use {
            return Err(SessionPrecompileError::ValuePerUseExceeded {
                max: policy.max_value_per_use,
                actual: input.value,
            });
        }

        // Check value limit
        let value_context = [b"call".as_slice(), input.to.as_slice(), selector.as_slice()].concat();
        let value_slot = limit_state_key(input.session_hash, &value_context, 0);
        let mut value_limit_state = read_limit_state(state, value_slot);
        LimitEngine::check_and_update(
            &policy.value_limit,
            &mut value_limit_state,
            input.value,
            input.block_timestamp,
        )?;

        // Check constraints
        for (idx, constraint) in policy.constraints.iter().enumerate() {
            let constraint_context = [
                b"call_constraint".as_slice(),
                input.to.as_slice(),
                selector.as_slice(),
                &[idx as u8],
            ]
            .concat();
            let constraint_slot = limit_state_key(input.session_hash, &constraint_context, 0);
            let mut constraint_limit_state = read_limit_state(state, constraint_slot);

            let calldata_bytes = Bytes::copy_from_slice(input.calldata);
            ConstraintEngine::verify_constraint(
                constraint,
                &calldata_bytes,
                &mut constraint_limit_state,
                input.block_timestamp,
            )?;
        }

        Ok(())
    }

    /// Validates a native transfer against transfer policies.
    fn validate_transfer(
        state: &impl PachiState,
        input: &SessionValidationInput<'_>,
    ) -> Result<(), SessionPrecompileError> {
        let policy = input
            .transfer_policies
            .iter()
            .find(|p| p.target == input.to)
            .ok_or(SessionPrecompileError::TransferNotAllowed { target: input.to })?;

        // Check per-use value limit
        if input.value > policy.max_value_per_use {
            return Err(SessionPrecompileError::ValuePerUseExceeded {
                max: policy.max_value_per_use,
                actual: input.value,
            });
        }

        // Check value limit
        let transfer_context = [b"transfer".as_slice(), input.to.as_slice()].concat();
        let transfer_slot = limit_state_key(input.session_hash, &transfer_context, 0);
        let mut transfer_limit_state = read_limit_state(state, transfer_slot);
        LimitEngine::check_and_update(
            &policy.value_limit,
            &mut transfer_limit_state,
            input.value,
            input.block_timestamp,
        )?;

        Ok(())
    }
}

/// Reads limit state from storage.
fn read_limit_state(state: &impl PachiState, slot: U256) -> LimitState {
    let addr = SESSION_REGISTRY_ADDRESS;
    let packed = state.get_storage(addr, slot);
    if packed.is_zero() {
        return LimitState { used: U256::ZERO, last_window: 0 };
    }
    // Packed: used in first slot, last_window in second slot
    let used = state.get_storage(addr, slot);
    let last_window_val = state.get_storage(addr, slot + U256::from(1));
    LimitState { used, last_window: last_window_val.as_limbs()[0] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pachi_oracle_precompile::MockState;
    use pachi_primitives::{ConditionType, Constraint, Limit};

    fn setup_session(state: &mut MockState) -> (Address, B256) {
        let authorizer = Address::from([0xAAu8; 20]);
        let signer = Address::from([0xBBu8; 20]);
        let hash = B256::from([0x11u8; 32]);

        SessionRegistry::create_session(state, authorizer, signer, 10_000, hash, 100, 10).unwrap();

        (authorizer, hash)
    }

    fn make_call_policy(target: Address, selector: [u8; 4]) -> CallPolicy {
        CallPolicy {
            target,
            selector: FixedBytes::from(selector),
            value_limit: Limit::unlimited(),
            max_value_per_use: U256::MAX,
            constraints: vec![],
        }
    }

    fn make_transfer_policy(target: Address) -> TransferPolicy {
        TransferPolicy {
            target,
            max_value_per_use: U256::from(1_000_000u64),
            value_limit: Limit::unlimited(),
        }
    }

    #[test]
    fn validate_active_session_call() {
        let mut state = MockState::new();
        let (authorizer, hash) = setup_session(&mut state);
        let target = Address::from([0xCCu8; 20]);
        let selector = [0xDE, 0xAD, 0xBE, 0xEF];

        let calldata = selector.to_vec();
        let input = SessionValidationInput {
            session_hash: &hash,
            authorizer,
            nonce: 0,
            to: target,
            calldata: &calldata,
            value: U256::ZERO,
            estimated_gas_cost: U256::from(100u64),
            block_timestamp: 200,
            call_policies: &[make_call_policy(target, selector)],
            transfer_policies: &[],
            fee_limit: &Limit::unlimited(),
        };

        SessionValidator::validate(&state, &input).unwrap();
    }

    #[test]
    fn validate_expired_session_fails() {
        let mut state = MockState::new();
        let (authorizer, hash) = setup_session(&mut state);

        let input = SessionValidationInput {
            session_hash: &hash,
            authorizer,
            nonce: 0,
            to: Address::ZERO,
            calldata: &[],
            value: U256::ZERO,
            estimated_gas_cost: U256::ZERO,
            block_timestamp: 20_000, // past expires_at=10_000
            call_policies: &[],
            transfer_policies: &[make_transfer_policy(Address::ZERO)],
            fee_limit: &Limit::unlimited(),
        };

        let err = SessionValidator::validate(&state, &input).unwrap_err();
        assert!(matches!(err, SessionPrecompileError::SessionExpired { .. }));
    }

    #[test]
    fn validate_revoked_session_fails() {
        let mut state = MockState::new();
        let (authorizer, hash) = setup_session(&mut state);

        SessionRegistry::revoke_session(&mut state, authorizer, &hash).unwrap();

        let input = SessionValidationInput {
            session_hash: &hash,
            authorizer,
            nonce: 0,
            to: Address::ZERO,
            calldata: &[],
            value: U256::ZERO,
            estimated_gas_cost: U256::ZERO,
            block_timestamp: 200,
            call_policies: &[],
            transfer_policies: &[],
            fee_limit: &Limit::unlimited(),
        };

        let err = SessionValidator::validate(&state, &input).unwrap_err();
        assert!(matches!(err, SessionPrecompileError::SessionRevoked { .. }));
    }

    #[test]
    fn validate_wrong_nonce_fails() {
        let mut state = MockState::new();
        let (authorizer, hash) = setup_session(&mut state);
        let target = Address::from([0xCCu8; 20]);

        let input = SessionValidationInput {
            session_hash: &hash,
            authorizer,
            nonce: 5, // wrong nonce
            to: target,
            calldata: &[0xDE, 0xAD, 0xBE, 0xEF],
            value: U256::ZERO,
            estimated_gas_cost: U256::ZERO,
            block_timestamp: 200,
            call_policies: &[make_call_policy(target, [0xDE, 0xAD, 0xBE, 0xEF])],
            transfer_policies: &[],
            fee_limit: &Limit::unlimited(),
        };

        let err = SessionValidator::validate(&state, &input).unwrap_err();
        assert!(matches!(err, SessionPrecompileError::NonceMismatch { expected: 0, actual: 5 }));
    }

    #[test]
    fn validate_call_not_allowed_fails() {
        let mut state = MockState::new();
        let (authorizer, hash) = setup_session(&mut state);
        let target = Address::from([0xCCu8; 20]);

        let input = SessionValidationInput {
            session_hash: &hash,
            authorizer,
            nonce: 0,
            to: target,
            calldata: &[0xDE, 0xAD, 0xBE, 0xEF],
            value: U256::ZERO,
            estimated_gas_cost: U256::ZERO,
            block_timestamp: 200,
            call_policies: &[], // no policies
            transfer_policies: &[],
            fee_limit: &Limit::unlimited(),
        };

        let err = SessionValidator::validate(&state, &input).unwrap_err();
        assert!(matches!(err, SessionPrecompileError::CallNotAllowed { .. }));
    }

    #[test]
    fn validate_transfer() {
        let mut state = MockState::new();
        let (authorizer, hash) = setup_session(&mut state);
        let target = Address::from([0xCCu8; 20]);

        let input = SessionValidationInput {
            session_hash: &hash,
            authorizer,
            nonce: 0,
            to: target,
            calldata: &[], // no calldata = transfer
            value: U256::from(500u64),
            estimated_gas_cost: U256::ZERO,
            block_timestamp: 200,
            call_policies: &[],
            transfer_policies: &[make_transfer_policy(target)],
            fee_limit: &Limit::unlimited(),
        };

        SessionValidator::validate(&state, &input).unwrap();
    }

    #[test]
    fn validate_transfer_value_per_use_exceeded() {
        let mut state = MockState::new();
        let (authorizer, hash) = setup_session(&mut state);
        let target = Address::from([0xCCu8; 20]);

        let input = SessionValidationInput {
            session_hash: &hash,
            authorizer,
            nonce: 0,
            to: target,
            calldata: &[],
            value: U256::from(2_000_000u64), // exceeds max 1_000_000
            estimated_gas_cost: U256::ZERO,
            block_timestamp: 200,
            call_policies: &[],
            transfer_policies: &[make_transfer_policy(target)],
            fee_limit: &Limit::unlimited(),
        };

        let err = SessionValidator::validate(&state, &input).unwrap_err();
        assert!(matches!(err, SessionPrecompileError::ValuePerUseExceeded { .. }));
    }

    #[test]
    fn validate_with_constraints() {
        let mut state = MockState::new();
        let (authorizer, hash) = setup_session(&mut state);
        let target = Address::from([0xCCu8; 20]);
        let selector = [0xDE, 0xAD, 0xBE, 0xEF];

        // Constraint: arg[0] must equal 0x05
        let constraint = Constraint {
            index: 0,
            condition: ConditionType::Equal,
            ref_value: B256::left_padding_from(&[0x05]),
            limit: Limit::unlimited(),
        };
        let policy = CallPolicy {
            target,
            selector: FixedBytes::from(selector),
            value_limit: Limit::unlimited(),
            max_value_per_use: U256::MAX,
            constraints: vec![constraint],
        };

        // Calldata: selector + arg0 = 5 (32 bytes padded)
        let mut calldata = selector.to_vec();
        calldata.extend_from_slice(&B256::left_padding_from(&[0x05]).0);

        let input = SessionValidationInput {
            session_hash: &hash,
            authorizer,
            nonce: 0,
            to: target,
            calldata: &calldata,
            value: U256::ZERO,
            estimated_gas_cost: U256::ZERO,
            block_timestamp: 200,
            call_policies: &[policy],
            transfer_policies: &[],
            fee_limit: &Limit::unlimited(),
        };

        SessionValidator::validate(&state, &input).unwrap();
    }

    #[test]
    fn validate_constraint_fails() {
        let mut state = MockState::new();
        let (authorizer, hash) = setup_session(&mut state);
        let target = Address::from([0xCCu8; 20]);
        let selector = [0xDE, 0xAD, 0xBE, 0xEF];

        // Constraint: arg[0] must equal 0x05
        let constraint = Constraint {
            index: 0,
            condition: ConditionType::Equal,
            ref_value: B256::left_padding_from(&[0x05]),
            limit: Limit::unlimited(),
        };
        let policy = CallPolicy {
            target,
            selector: FixedBytes::from(selector),
            value_limit: Limit::unlimited(),
            max_value_per_use: U256::MAX,
            constraints: vec![constraint],
        };

        // Calldata: selector + arg0 = 7 (not 5!)
        let mut calldata = selector.to_vec();
        calldata.extend_from_slice(&B256::left_padding_from(&[0x07]).0);

        let input = SessionValidationInput {
            session_hash: &hash,
            authorizer,
            nonce: 0,
            to: target,
            calldata: &calldata,
            value: U256::ZERO,
            estimated_gas_cost: U256::ZERO,
            block_timestamp: 200,
            call_policies: &[policy],
            transfer_policies: &[],
            fee_limit: &Limit::unlimited(),
        };

        let err = SessionValidator::validate(&state, &input).unwrap_err();
        assert!(matches!(err, SessionPrecompileError::ConstraintError(_)));
    }
}
