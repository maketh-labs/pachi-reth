//! Dual-mode gas settlement logic.
//!
//! After transaction execution, settles gas costs based on sponsor type:
//! - Deposit: deduct actual gas from balance, refund excess lock.
//! - Mint: compute mint amount from actual gas.

use alloy_primitives::{Address, U256};
use pachi_oracle_precompile::PachiState;
use pachi_primitives::{LimitEngine, LimitState};

use crate::{
    error::SponsorPrecompileError,
    governance::SponsorGovernance,
    record::SponsorType,
    storage_keys::{
        sponsor_limit_key, sponsor_record_key, sponsor_sender_limit_key, SPONSOR_HUB_ADDRESS,
    },
};

/// Actions the EVM layer should take after settlement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettlementAction {
    /// Deposit mode: actual gas cost deducted from sponsor balance.
    DepositDeducted {
        /// Actual gas cost deducted.
        actual_cost: U256,
    },
    /// Mint mode: protocol should mint this amount to the sequencer.
    MintRequired {
        /// Amount to mint (actual gas cost).
        mint_amount: U256,
    },
}

/// Balance offset in sponsor record.
const BALANCE_OFFSET: U256 = U256::from_limbs([2, 0, 0, 0]);

/// Gas settlement for sponsored transactions.
#[derive(Debug)]
pub struct SponsorSettlement;

impl SponsorSettlement {
    /// Pre-execution: locks max possible gas cost for deposit mode.
    ///
    /// For mint mode, only checks limits (no balance to lock).
    /// Returns the locked amount (deposit) or zero (mint).
    pub fn pre_lock(
        state: &mut impl PachiState,
        sponsor: Address,
        max_gas_cost: U256,
    ) -> Result<U256, SponsorPrecompileError> {
        let sponsor_type =
            SponsorGovernance::get_sponsor_type(state, sponsor).unwrap_or(SponsorType::Deposit);

        match sponsor_type {
            SponsorType::Deposit => {
                let base = sponsor_record_key(sponsor);
                let balance = state.get_storage(SPONSOR_HUB_ADDRESS, base + BALANCE_OFFSET);
                if balance < max_gas_cost {
                    return Err(SponsorPrecompileError::InsufficientBalance {
                        balance,
                        required: max_gas_cost,
                    });
                }
                // Lock by deducting from balance
                state.set_storage(
                    SPONSOR_HUB_ADDRESS,
                    base + BALANCE_OFFSET,
                    balance - max_gas_cost,
                );
                Ok(max_gas_cost)
            }
            SponsorType::Mint => {
                // No balance lock needed
                Ok(U256::ZERO)
            }
        }
    }

    /// Post-execution: settles actual gas cost and updates limits.
    ///
    /// For deposit mode: refunds excess from the lock.
    /// For mint mode: returns the amount that should be minted.
    #[allow(clippy::too_many_arguments)]
    pub fn settle(
        state: &mut impl PachiState,
        sponsor: Address,
        sender: Address,
        actual_gas_cost: U256,
        locked_amount: U256,
        block_timestamp: u64,
        global_fee_limit: &pachi_primitives::Limit,
        per_sender_fee_limit: &pachi_primitives::Limit,
        global_tx_limit: &pachi_primitives::Limit,
        per_sender_tx_limit: &pachi_primitives::Limit,
    ) -> Result<SettlementAction, SponsorPrecompileError> {
        let sponsor_type =
            SponsorGovernance::get_sponsor_type(state, sponsor).unwrap_or(SponsorType::Deposit);

        let action = match sponsor_type {
            SponsorType::Deposit => {
                // Refund excess: locked_amount - actual_gas_cost back to balance
                let refund = locked_amount.saturating_sub(actual_gas_cost);
                if !refund.is_zero() {
                    let base = sponsor_record_key(sponsor);
                    let balance = state.get_storage(SPONSOR_HUB_ADDRESS, base + BALANCE_OFFSET);
                    state.set_storage(SPONSOR_HUB_ADDRESS, base + BALANCE_OFFSET, balance + refund);
                }
                SettlementAction::DepositDeducted { actual_cost: actual_gas_cost }
            }
            SponsorType::Mint => SettlementAction::MintRequired { mint_amount: actual_gas_cost },
        };

        // Update limit states
        Self::update_limits(
            state,
            sponsor,
            sender,
            actual_gas_cost,
            block_timestamp,
            global_fee_limit,
            per_sender_fee_limit,
            global_tx_limit,
            per_sender_tx_limit,
        )?;

        Ok(action)
    }

    /// Updates all limit states after settlement.
    #[allow(clippy::too_many_arguments)]
    fn update_limits(
        state: &mut impl PachiState,
        sponsor: Address,
        sender: Address,
        actual_gas_cost: U256,
        block_timestamp: u64,
        global_fee_limit: &pachi_primitives::Limit,
        per_sender_fee_limit: &pachi_primitives::Limit,
        global_tx_limit: &pachi_primitives::Limit,
        per_sender_tx_limit: &pachi_primitives::Limit,
    ) -> Result<(), SponsorPrecompileError> {
        // Global fee limit
        let global_fee_slot = sponsor_limit_key(sponsor, b"global_fee");
        let mut global_fee_state = read_limit_state(state, global_fee_slot);
        LimitEngine::check_and_update(
            global_fee_limit,
            &mut global_fee_state,
            actual_gas_cost,
            block_timestamp,
        )?;
        write_limit_state(state, global_fee_slot, &global_fee_state);

        // Per-sender fee limit
        let sender_fee_slot = sponsor_sender_limit_key(sponsor, sender, b"sender_fee");
        let mut sender_fee_state = read_limit_state(state, sender_fee_slot);
        LimitEngine::check_and_update(
            per_sender_fee_limit,
            &mut sender_fee_state,
            actual_gas_cost,
            block_timestamp,
        )?;
        write_limit_state(state, sender_fee_slot, &sender_fee_state);

        // Global tx limit (+1)
        let global_tx_slot = sponsor_limit_key(sponsor, b"global_tx");
        let mut global_tx_state = read_limit_state(state, global_tx_slot);
        LimitEngine::check_and_update(
            global_tx_limit,
            &mut global_tx_state,
            U256::from(1),
            block_timestamp,
        )?;
        write_limit_state(state, global_tx_slot, &global_tx_state);

        // Per-sender tx limit (+1)
        let sender_tx_slot = sponsor_sender_limit_key(sponsor, sender, b"sender_tx");
        let mut sender_tx_state = read_limit_state(state, sender_tx_slot);
        LimitEngine::check_and_update(
            per_sender_tx_limit,
            &mut sender_tx_state,
            U256::from(1),
            block_timestamp,
        )?;
        write_limit_state(state, sender_tx_slot, &sender_tx_state);

        Ok(())
    }
}

/// Reads limit state from two consecutive storage slots.
fn read_limit_state(state: &impl PachiState, slot: U256) -> LimitState {
    let used = state.get_storage(SPONSOR_HUB_ADDRESS, slot);
    let last_window_val = state.get_storage(SPONSOR_HUB_ADDRESS, slot + U256::from(1));
    LimitState { used, last_window: last_window_val.as_limbs()[0] }
}

/// Writes limit state to two consecutive storage slots.
fn write_limit_state(state: &mut impl PachiState, slot: U256, limit_state: &LimitState) {
    state.set_storage(SPONSOR_HUB_ADDRESS, slot, limit_state.used);
    state.set_storage(
        SPONSOR_HUB_ADDRESS,
        slot + U256::from(1),
        U256::from(limit_state.last_window),
    );
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

    fn setup_deposit_sponsor(state: &mut MockState) -> Address {
        let sponsor = Address::from([0xAAu8; 20]);
        SponsorHub::register_policy(state, sponsor, &default_config(), 100).unwrap();
        SponsorHub::deposit(state, sponsor, U256::from(10_000u64)).unwrap();
        sponsor
    }

    fn setup_mint_sponsor(state: &mut MockState) -> Address {
        let sponsor = Address::from([0xBBu8; 20]);
        let owner = Address::from([0x01u8; 20]);
        SponsorGovernance::set_owner(state, owner);
        SponsorHub::register_policy(state, sponsor, &default_config(), 100).unwrap();
        SponsorGovernance::approve_mint(state, owner, sponsor).unwrap();
        sponsor
    }

    #[test]
    fn deposit_pre_lock_and_settle() {
        let mut state = MockState::new();
        let sponsor = setup_deposit_sponsor(&mut state);
        let sender = Address::from([0xCCu8; 20]);

        // Pre-lock 5000
        let locked =
            SponsorSettlement::pre_lock(&mut state, sponsor, U256::from(5_000u64)).unwrap();
        assert_eq!(locked, U256::from(5_000u64));
        assert_eq!(SponsorHub::get_balance(&state, sponsor), U256::from(5_000u64));

        // Settle with actual gas 3000 → refund 2000
        let action = SponsorSettlement::settle(
            &mut state,
            sponsor,
            sender,
            U256::from(3_000u64),
            locked,
            200,
            &Limit::unlimited(),
            &Limit::unlimited(),
            &Limit::unlimited(),
            &Limit::unlimited(),
        )
        .unwrap();

        assert_eq!(action, SettlementAction::DepositDeducted { actual_cost: U256::from(3_000u64) });
        assert_eq!(SponsorHub::get_balance(&state, sponsor), U256::from(7_000u64)); // 5000 + 2000
                                                                                    // refund
    }

    #[test]
    fn deposit_insufficient_balance() {
        let mut state = MockState::new();
        let sponsor = setup_deposit_sponsor(&mut state);

        let err =
            SponsorSettlement::pre_lock(&mut state, sponsor, U256::from(20_000u64)).unwrap_err();
        assert!(matches!(err, SponsorPrecompileError::InsufficientBalance { .. }));
    }

    #[test]
    fn mint_settle_returns_mint_action() {
        let mut state = MockState::new();
        let sponsor = setup_mint_sponsor(&mut state);
        let sender = Address::from([0xCCu8; 20]);

        // No pre-lock needed for mint
        let locked =
            SponsorSettlement::pre_lock(&mut state, sponsor, U256::from(5_000u64)).unwrap();
        assert_eq!(locked, U256::ZERO);

        let action = SponsorSettlement::settle(
            &mut state,
            sponsor,
            sender,
            U256::from(3_000u64),
            locked,
            200,
            &Limit::unlimited(),
            &Limit::unlimited(),
            &Limit::unlimited(),
            &Limit::unlimited(),
        )
        .unwrap();

        assert_eq!(action, SettlementAction::MintRequired { mint_amount: U256::from(3_000u64) });
    }

    #[test]
    fn settle_with_lifetime_limit() {
        let mut state = MockState::new();
        let sponsor = setup_deposit_sponsor(&mut state);
        let sender = Address::from([0xCCu8; 20]);
        let fee_limit = Limit::lifetime(U256::from(5_000u64));

        // First tx: 3000 gas → OK (3000 < 5000)
        let locked =
            SponsorSettlement::pre_lock(&mut state, sponsor, U256::from(3_000u64)).unwrap();
        SponsorSettlement::settle(
            &mut state,
            sponsor,
            sender,
            U256::from(3_000u64),
            locked,
            200,
            &fee_limit,
            &Limit::unlimited(),
            &Limit::unlimited(),
            &Limit::unlimited(),
        )
        .unwrap();

        // Second tx: 3000 gas → exceeds limit (6000 > 5000)
        let locked =
            SponsorSettlement::pre_lock(&mut state, sponsor, U256::from(3_000u64)).unwrap();
        let err = SponsorSettlement::settle(
            &mut state,
            sponsor,
            sender,
            U256::from(3_000u64),
            locked,
            200,
            &fee_limit,
            &Limit::unlimited(),
            &Limit::unlimited(),
            &Limit::unlimited(),
        )
        .unwrap_err();
        assert!(matches!(err, SponsorPrecompileError::LimitError(_)));
    }
}
