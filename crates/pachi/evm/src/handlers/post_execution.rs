//! Post-execution handler for Pachi custom transaction types.
//!
//! After EVM execution completes, handles:
//! - `SessionTx`: increment session nonce, update fee/value/constraint limit states
//! - `SponsoredTx (Deposit)`: deduct actual gas from sponsor, refund excess lock
//! - `SponsoredTx (Mint)`: compute mint amount from actual gas
//! - `SessionSponsoredTx`: session nonce/limits + sponsor settlement

use alloy_primitives::{Address, Bytes, FixedBytes, U256};
use pachi_oracle_precompile::PachiState;
use pachi_primitives::{CallPolicy, LimitEngine, LimitState, TransferPolicy};
use pachi_session_precompile::{SessionNonce, SESSION_REGISTRY_ADDRESS};
use pachi_sponsor_precompile::{SettlementAction, SponsorSettlement};

/// Result of post-execution settlement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostExecutionResult {
    /// Sponsor settlement action (if applicable).
    pub settlement: Option<SettlementAction>,
}

/// Post-execution handler: updates state after EVM execution.
#[derive(Debug)]
pub struct PostExecutionHandler;

impl PostExecutionHandler {
    /// Post-execution for `SessionTx`:
    /// 1. Increment session nonce
    /// 2. Update fee limit state
    /// 3. Update call/transfer value limit states
    /// 4. Update constraint limit states
    #[allow(clippy::too_many_arguments)]
    pub fn finalize_session_tx(
        state: &mut impl PachiState,
        authorizer: Address,
        session_hash: &alloy_primitives::B256,
        actual_gas_cost: U256,
        block_timestamp: u64,
        fee_limit: &pachi_primitives::Limit,
        to: Address,
        calldata: &[u8],
        value: U256,
        call_policies: &[CallPolicy],
        transfer_policies: &[TransferPolicy],
    ) -> PostExecutionResult {
        // 1. Increment session nonce
        SessionNonce::increment(state, authorizer, session_hash);

        // 2. Update fee limit
        let fee_slot =
            pachi_session_precompile::storage_keys::limit_state_key(session_hash, b"fee", 0);
        let mut fee_state = read_limit_state(state, fee_slot);
        let _ = LimitEngine::check_and_update(
            fee_limit,
            &mut fee_state,
            actual_gas_cost,
            block_timestamp,
        );
        write_limit_state(state, fee_slot, &fee_state);

        // 3. Update call/transfer value limits
        if calldata.len() >= 4 {
            let selector = FixedBytes::<4>::from_slice(&calldata[..4]);
            if let Some(policy) =
                call_policies.iter().find(|p| p.target == to && p.selector == selector)
            {
                let value_context =
                    [b"call".as_slice(), to.as_slice(), selector.as_slice()].concat();
                let value_slot = pachi_session_precompile::storage_keys::limit_state_key(
                    session_hash,
                    &value_context,
                    0,
                );
                let mut value_state = read_limit_state(state, value_slot);
                let _ = LimitEngine::check_and_update(
                    &policy.value_limit,
                    &mut value_state,
                    value,
                    block_timestamp,
                );
                write_limit_state(state, value_slot, &value_state);

                // 4. Update constraint limits
                for (idx, constraint) in policy.constraints.iter().enumerate() {
                    let constraint_context = [
                        b"call_constraint".as_slice(),
                        to.as_slice(),
                        selector.as_slice(),
                        &[idx as u8],
                    ]
                    .concat();
                    let constraint_slot = pachi_session_precompile::storage_keys::limit_state_key(
                        session_hash,
                        &constraint_context,
                        0,
                    );
                    let mut constraint_state = read_limit_state(state, constraint_slot);
                    let calldata_bytes = Bytes::copy_from_slice(calldata);
                    if let Ok(arg) = pachi_primitives::ConstraintEngine::extract_argument(
                        &calldata_bytes,
                        constraint.index,
                    ) {
                        let arg_u256 = U256::from_be_bytes(arg.0);
                        let _ = LimitEngine::check_and_update(
                            &constraint.limit,
                            &mut constraint_state,
                            arg_u256,
                            block_timestamp,
                        );
                        write_limit_state(state, constraint_slot, &constraint_state);
                    }
                }
            }
        } else if let Some(policy) = transfer_policies.iter().find(|p| p.target == to) {
            let transfer_context = [b"transfer".as_slice(), to.as_slice()].concat();
            let transfer_slot = pachi_session_precompile::storage_keys::limit_state_key(
                session_hash,
                &transfer_context,
                0,
            );
            let mut transfer_state = read_limit_state(state, transfer_slot);
            let _ = LimitEngine::check_and_update(
                &policy.value_limit,
                &mut transfer_state,
                value,
                block_timestamp,
            );
            write_limit_state(state, transfer_slot, &transfer_state);
        }

        PostExecutionResult { settlement: None }
    }

    /// Post-execution for `SponsoredTx`:
    /// - Deposit mode: deduct actual gas, refund excess, update limits
    /// - Mint mode: return mint amount, update limits
    pub fn finalize_sponsored_tx(
        state: &mut impl PachiState,
        sponsor: Address,
        sender: Address,
        actual_gas_cost: U256,
        locked_amount: U256,
        block_timestamp: u64,
        config: &pachi_sponsor_precompile::SponsorRecord,
    ) -> Result<PostExecutionResult, pachi_sponsor_precompile::SponsorPrecompileError> {
        let action = SponsorSettlement::settle(
            state,
            sponsor,
            sender,
            actual_gas_cost,
            locked_amount,
            block_timestamp,
            &config.config.global_fee_limit,
            &config.config.per_sender_fee_limit,
            &config.config.global_tx_limit,
            &config.config.per_sender_tx_limit,
        )?;

        Ok(PostExecutionResult { settlement: Some(action) })
    }

    /// Post-execution for `SessionSponsoredTx`:
    /// Session nonce/limits + sponsor settlement.
    #[allow(clippy::too_many_arguments)]
    pub fn finalize_session_sponsored_tx(
        state: &mut impl PachiState,
        authorizer: Address,
        session_hash: &alloy_primitives::B256,
        sponsor: Address,
        sender: Address,
        actual_gas_cost: U256,
        locked_amount: U256,
        block_timestamp: u64,
        fee_limit: &pachi_primitives::Limit,
        to: Address,
        calldata: &[u8],
        value: U256,
        call_policies: &[CallPolicy],
        transfer_policies: &[TransferPolicy],
        sponsor_config: &pachi_sponsor_precompile::SponsorRecord,
    ) -> Result<PostExecutionResult, pachi_sponsor_precompile::SponsorPrecompileError> {
        // Session finalization
        Self::finalize_session_tx(
            state,
            authorizer,
            session_hash,
            actual_gas_cost,
            block_timestamp,
            fee_limit,
            to,
            calldata,
            value,
            call_policies,
            transfer_policies,
        );

        // Sponsor settlement
        Self::finalize_sponsored_tx(
            state,
            sponsor,
            sender,
            actual_gas_cost,
            locked_amount,
            block_timestamp,
            sponsor_config,
        )
    }
}

/// Reads limit state from two consecutive storage slots.
fn read_limit_state(state: &impl PachiState, slot: U256) -> LimitState {
    let used = state.get_storage(SESSION_REGISTRY_ADDRESS, slot);
    let last_window_val = state.get_storage(SESSION_REGISTRY_ADDRESS, slot + U256::from(1));
    LimitState { used, last_window: last_window_val.as_limbs()[0] }
}

/// Writes limit state to two consecutive storage slots.
fn write_limit_state(state: &mut impl PachiState, slot: U256, limit_state: &LimitState) {
    state.set_storage(SESSION_REGISTRY_ADDRESS, slot, limit_state.used);
    state.set_storage(
        SESSION_REGISTRY_ADDRESS,
        slot + U256::from(1),
        U256::from(limit_state.last_window),
    );
}
