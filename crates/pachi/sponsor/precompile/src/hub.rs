//! `SponsorHub`: sponsor lifecycle management.
//!
//! Handles registration, deactivation, deposit, withdraw, and view operations.

use alloy_primitives::{Address, U256};
use pachi_oracle_precompile::PachiState;

use alloy_rlp::{Decodable, Encodable};

use crate::{
    error::SponsorPrecompileError,
    governance::SponsorGovernance,
    record::{SponsorConfig, SponsorStatus, SponsorType},
    storage_keys::{sponsor_config_key, sponsor_record_key, SPONSOR_HUB_ADDRESS},
};

/// Slot offsets within a sponsor record.
const STATUS_OFFSET: U256 = U256::from_limbs([0, 0, 0, 0]);
const TYPE_OFFSET: U256 = U256::from_limbs([1, 0, 0, 0]);
const BALANCE_OFFSET: U256 = U256::from_limbs([2, 0, 0, 0]);
const CREATED_AT_OFFSET: U256 = U256::from_limbs([3, 0, 0, 0]);
const VALID_UNTIL_OFFSET: U256 = U256::from_limbs([4, 0, 0, 0]);

/// `SponsorHub`: manages sponsor lifecycle.
#[derive(Debug)]
pub struct SponsorHub;

impl SponsorHub {
    /// Registers a new sponsor policy.
    ///
    /// Always registers as Deposit type. Mint must be approved by governance.
    /// Replaces previous policy if one exists.
    pub fn register_policy(
        state: &mut impl PachiState,
        sponsor: Address,
        config: &SponsorConfig,
        block_timestamp: u64,
    ) -> Result<(), SponsorPrecompileError> {
        let base = sponsor_record_key(sponsor);

        // Preserve existing balance if re-registering
        let existing_balance = state.get_storage(SPONSOR_HUB_ADDRESS, base + BALANCE_OFFSET);
        let existing_type = state.get_storage(SPONSOR_HUB_ADDRESS, base + TYPE_OFFSET);

        state.set_storage(
            SPONSOR_HUB_ADDRESS,
            base + STATUS_OFFSET,
            U256::from(SponsorStatus::Active as u8),
        );
        // Preserve sponsor type if already set (might be Mint)
        let created_at = state.get_storage(SPONSOR_HUB_ADDRESS, base + CREATED_AT_OFFSET);
        if created_at.is_zero() {
            // New sponsor
            state.set_storage(
                SPONSOR_HUB_ADDRESS,
                base + TYPE_OFFSET,
                U256::from(SponsorType::Deposit as u8),
            );
            state.set_storage(
                SPONSOR_HUB_ADDRESS,
                base + CREATED_AT_OFFSET,
                U256::from(block_timestamp),
            );
        } else {
            // Re-registration: keep existing type and balance
            state.set_storage(SPONSOR_HUB_ADDRESS, base + TYPE_OFFSET, existing_type);
            state.set_storage(SPONSOR_HUB_ADDRESS, base + BALANCE_OFFSET, existing_balance);
        }

        state.set_storage(
            SPONSOR_HUB_ADDRESS,
            base + VALID_UNTIL_OFFSET,
            U256::from(config.valid_until),
        );

        // Store full config as RLP blob
        Self::store_config(state, sponsor, config);

        Ok(())
    }

    /// Deactivates a sponsor's policy. Deposit is preserved.
    pub fn deactivate_policy(
        state: &mut impl PachiState,
        sponsor: Address,
    ) -> Result<(), SponsorPrecompileError> {
        Self::ensure_exists(state, sponsor)?;

        let base = sponsor_record_key(sponsor);
        state.set_storage(
            SPONSOR_HUB_ADDRESS,
            base + STATUS_OFFSET,
            U256::from(SponsorStatus::Deactivated as u8),
        );
        Ok(())
    }

    /// Deposits funds for a deposit-mode sponsor.
    pub fn deposit(
        state: &mut impl PachiState,
        sponsor: Address,
        amount: U256,
    ) -> Result<(), SponsorPrecompileError> {
        Self::ensure_exists(state, sponsor)?;

        let sponsor_type =
            SponsorGovernance::get_sponsor_type(state, sponsor).unwrap_or(SponsorType::Deposit);
        if sponsor_type == SponsorType::Mint {
            return Err(SponsorPrecompileError::MintModeNotAllowed);
        }

        let base = sponsor_record_key(sponsor);
        let current_balance = state.get_storage(SPONSOR_HUB_ADDRESS, base + BALANCE_OFFSET);
        state.set_storage(SPONSOR_HUB_ADDRESS, base + BALANCE_OFFSET, current_balance + amount);
        Ok(())
    }

    /// Withdraws funds from a deposit-mode sponsor.
    pub fn withdraw(
        state: &mut impl PachiState,
        sponsor: Address,
        amount: U256,
    ) -> Result<(), SponsorPrecompileError> {
        Self::ensure_exists(state, sponsor)?;

        let sponsor_type =
            SponsorGovernance::get_sponsor_type(state, sponsor).unwrap_or(SponsorType::Deposit);
        if sponsor_type == SponsorType::Mint {
            return Err(SponsorPrecompileError::MintModeNotAllowed);
        }

        let base = sponsor_record_key(sponsor);
        let current_balance = state.get_storage(SPONSOR_HUB_ADDRESS, base + BALANCE_OFFSET);
        if amount > current_balance {
            return Err(SponsorPrecompileError::WithdrawExceedsBalance {
                balance: current_balance,
                amount,
            });
        }
        state.set_storage(SPONSOR_HUB_ADDRESS, base + BALANCE_OFFSET, current_balance - amount);
        Ok(())
    }

    // --- View functions ---

    /// Gets the deposit balance for a sponsor.
    pub fn get_balance(state: &impl PachiState, sponsor: Address) -> U256 {
        let base = sponsor_record_key(sponsor);
        state.get_storage(SPONSOR_HUB_ADDRESS, base + BALANCE_OFFSET)
    }

    /// Checks if a sponsor is active.
    pub fn is_active(state: &impl PachiState, sponsor: Address) -> bool {
        SponsorGovernance::get_sponsor_status(state, sponsor)
            .map(|s| s == SponsorStatus::Active)
            .unwrap_or(false)
    }

    /// Gets the `valid_until` timestamp for a sponsor.
    pub fn get_valid_until(state: &impl PachiState, sponsor: Address) -> u64 {
        let base = sponsor_record_key(sponsor);
        state.get_storage(SPONSOR_HUB_ADDRESS, base + VALID_UNTIL_OFFSET).as_limbs()[0]
    }

    /// Stores the full `SponsorConfig` as an RLP-encoded blob across storage slots.
    ///
    /// Layout at `keccak256("sponsor_config", sponsor)`:
    /// - slot+0: byte length of the RLP data
    /// - slot+1..N: RLP bytes (32 bytes per slot, zero-padded)
    fn store_config(state: &mut impl PachiState, sponsor: Address, config: &SponsorConfig) {
        let base = sponsor_config_key(sponsor);
        let mut rlp_buf = Vec::new();
        config.encode(&mut rlp_buf);

        // Store length
        state.set_storage(SPONSOR_HUB_ADDRESS, base, U256::from(rlp_buf.len()));

        // Store data in 32-byte chunks
        let num_slots = rlp_buf.len().div_ceil(32);
        for i in 0..num_slots {
            let start = i * 32;
            let end = (start + 32).min(rlp_buf.len());
            let mut word = [0u8; 32];
            word[..end - start].copy_from_slice(&rlp_buf[start..end]);
            state.set_storage(
                SPONSOR_HUB_ADDRESS,
                base + U256::from(i + 1),
                U256::from_be_bytes(word),
            );
        }
    }

    /// Loads the full `SponsorConfig` from on-chain RLP blob.
    ///
    /// Returns `None` if no config is stored or RLP decoding fails.
    pub fn load_config(state: &impl PachiState, sponsor: Address) -> Option<SponsorConfig> {
        let base = sponsor_config_key(sponsor);
        let len_val = state.get_storage(SPONSOR_HUB_ADDRESS, base);
        let len: usize = len_val.try_into().ok()?;
        if len == 0 {
            return None;
        }

        let num_slots = len.div_ceil(32);
        let mut rlp_buf = Vec::with_capacity(num_slots * 32);
        for i in 0..num_slots {
            let word = state.get_storage(SPONSOR_HUB_ADDRESS, base + U256::from(i + 1));
            rlp_buf.extend_from_slice(&word.to_be_bytes::<32>());
        }
        rlp_buf.truncate(len);

        SponsorConfig::decode(&mut rlp_buf.as_slice()).ok()
    }

    /// Ensures a sponsor exists.
    fn ensure_exists(
        state: &impl PachiState,
        sponsor: Address,
    ) -> Result<(), SponsorPrecompileError> {
        let base = sponsor_record_key(sponsor);
        let created = state.get_storage(SPONSOR_HUB_ADDRESS, base + CREATED_AT_OFFSET);
        if created.is_zero() {
            return Err(SponsorPrecompileError::SponsorNotFound { sponsor });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn test_sponsor() -> Address {
        Address::from([0xAAu8; 20])
    }

    #[test]
    fn register_and_check_active() {
        let mut state = MockState::new();
        SponsorHub::register_policy(&mut state, test_sponsor(), &default_config(), 100).unwrap();

        assert!(SponsorHub::is_active(&state, test_sponsor()));
        assert_eq!(SponsorHub::get_balance(&state, test_sponsor()), U256::ZERO);
    }

    #[test]
    fn deposit_and_withdraw() {
        let mut state = MockState::new();
        let sponsor = test_sponsor();
        SponsorHub::register_policy(&mut state, sponsor, &default_config(), 100).unwrap();

        SponsorHub::deposit(&mut state, sponsor, U256::from(1000u64)).unwrap();
        assert_eq!(SponsorHub::get_balance(&state, sponsor), U256::from(1000u64));

        SponsorHub::withdraw(&mut state, sponsor, U256::from(400u64)).unwrap();
        assert_eq!(SponsorHub::get_balance(&state, sponsor), U256::from(600u64));
    }

    #[test]
    fn withdraw_exceeds_balance() {
        let mut state = MockState::new();
        let sponsor = test_sponsor();
        SponsorHub::register_policy(&mut state, sponsor, &default_config(), 100).unwrap();
        SponsorHub::deposit(&mut state, sponsor, U256::from(100u64)).unwrap();

        let err = SponsorHub::withdraw(&mut state, sponsor, U256::from(200u64)).unwrap_err();
        assert!(matches!(err, SponsorPrecompileError::WithdrawExceedsBalance { .. }));
    }

    #[test]
    fn deactivate_preserves_balance() {
        let mut state = MockState::new();
        let sponsor = test_sponsor();
        SponsorHub::register_policy(&mut state, sponsor, &default_config(), 100).unwrap();
        SponsorHub::deposit(&mut state, sponsor, U256::from(500u64)).unwrap();

        SponsorHub::deactivate_policy(&mut state, sponsor).unwrap();
        assert!(!SponsorHub::is_active(&state, sponsor));
        assert_eq!(SponsorHub::get_balance(&state, sponsor), U256::from(500u64));
    }

    #[test]
    fn mint_sponsor_cannot_deposit() {
        let mut state = MockState::new();
        let sponsor = test_sponsor();
        let owner = Address::from([0x01u8; 20]);
        SponsorGovernance::set_owner(&mut state, owner);

        SponsorHub::register_policy(&mut state, sponsor, &default_config(), 100).unwrap();
        SponsorGovernance::approve_mint(&mut state, owner, sponsor).unwrap();

        let err = SponsorHub::deposit(&mut state, sponsor, U256::from(100u64)).unwrap_err();
        assert!(matches!(err, SponsorPrecompileError::MintModeNotAllowed));
    }

    #[test]
    fn nonexistent_sponsor_fails() {
        let mut state = MockState::new();
        let err = SponsorHub::deposit(&mut state, test_sponsor(), U256::from(100u64)).unwrap_err();
        assert!(matches!(err, SponsorPrecompileError::SponsorNotFound { .. }));
    }

    #[test]
    fn re_register_preserves_balance() {
        let mut state = MockState::new();
        let sponsor = test_sponsor();
        SponsorHub::register_policy(&mut state, sponsor, &default_config(), 100).unwrap();
        SponsorHub::deposit(&mut state, sponsor, U256::from(500u64)).unwrap();

        // Re-register with new config
        let mut new_config = default_config();
        new_config.valid_until = 200_000;
        SponsorHub::register_policy(&mut state, sponsor, &new_config, 200).unwrap();

        assert_eq!(SponsorHub::get_balance(&state, sponsor), U256::from(500u64));
        assert_eq!(SponsorHub::get_valid_until(&state, sponsor), 200_000);
    }

    #[test]
    fn governance_transfer_ownership() {
        let mut state = MockState::new();
        let owner = Address::from([0x01u8; 20]);
        let new_owner = Address::from([0x02u8; 20]);

        SponsorGovernance::set_owner(&mut state, owner);
        assert_eq!(SponsorGovernance::get_owner(&state), owner);

        SponsorGovernance::transfer_ownership(&mut state, owner, new_owner).unwrap();
        assert_eq!(SponsorGovernance::get_owner(&state), new_owner);
    }

    #[test]
    fn governance_not_owner_fails() {
        let mut state = MockState::new();
        let owner = Address::from([0x01u8; 20]);
        let other = Address::from([0x02u8; 20]);

        SponsorGovernance::set_owner(&mut state, owner);

        let err =
            SponsorGovernance::transfer_ownership(&mut state, other, Address::ZERO).unwrap_err();
        assert!(matches!(err, SponsorPrecompileError::NotOwner { .. }));
    }

    #[test]
    fn approve_and_revoke_mint() {
        let mut state = MockState::new();
        let owner = Address::from([0x01u8; 20]);
        let sponsor = test_sponsor();

        SponsorGovernance::set_owner(&mut state, owner);
        SponsorHub::register_policy(&mut state, sponsor, &default_config(), 100).unwrap();

        // Approve mint
        SponsorGovernance::approve_mint(&mut state, owner, sponsor).unwrap();
        assert_eq!(SponsorGovernance::get_sponsor_type(&state, sponsor), Some(SponsorType::Mint));

        // Revoke mint → deposit with zero balance
        SponsorGovernance::revoke_mint(&mut state, owner, sponsor).unwrap();
        assert_eq!(
            SponsorGovernance::get_sponsor_type(&state, sponsor),
            Some(SponsorType::Deposit)
        );
        assert_eq!(SponsorHub::get_balance(&state, sponsor), U256::ZERO);
    }
}
