//! `SponsorHub` governance operations (owner-only).

use alloy_primitives::{Address, B256, U256};
use pachi_oracle_precompile::PachiState;

use crate::{
    error::SponsorPrecompileError,
    record::{SponsorStatus, SponsorType},
    storage_keys::{owner_key, sponsor_record_key, SPONSOR_HUB_ADDRESS},
};

/// Governance operations for the `SponsorHub`.
#[derive(Debug)]
pub struct SponsorGovernance;

/// Slot offsets within a sponsor record.
const STATUS_OFFSET: U256 = U256::from_limbs([0, 0, 0, 0]);
const TYPE_OFFSET: U256 = U256::from_limbs([1, 0, 0, 0]);
const BALANCE_OFFSET: U256 = U256::from_limbs([2, 0, 0, 0]);

impl SponsorGovernance {
    /// Reads the current `SponsorHub` owner.
    pub fn get_owner(state: &impl PachiState) -> Address {
        let val = state.get_storage(SPONSOR_HUB_ADDRESS, owner_key());
        Address::from_word(B256::from(val.to_be_bytes()))
    }

    /// Sets the `SponsorHub` owner.
    pub fn set_owner(state: &mut impl PachiState, owner: Address) {
        state.set_storage(
            SPONSOR_HUB_ADDRESS,
            owner_key(),
            U256::from_be_bytes(owner.into_word().0),
        );
    }

    /// Transfers ownership. Only callable by current owner.
    pub fn transfer_ownership(
        state: &mut impl PachiState,
        caller: Address,
        new_owner: Address,
    ) -> Result<(), SponsorPrecompileError> {
        let current_owner = Self::get_owner(state);
        if caller != current_owner {
            return Err(SponsorPrecompileError::NotOwner {
                expected: current_owner,
                actual: caller,
            });
        }
        Self::set_owner(state, new_owner);
        Ok(())
    }

    /// Approves mint mode for a sponsor (owner-only).
    ///
    /// The sponsor must have a registered policy first.
    pub fn approve_mint(
        state: &mut impl PachiState,
        caller: Address,
        sponsor: Address,
    ) -> Result<(), SponsorPrecompileError> {
        let current_owner = Self::get_owner(state);
        if caller != current_owner {
            return Err(SponsorPrecompileError::NotOwner {
                expected: current_owner,
                actual: caller,
            });
        }

        // Check existence via created_at (set during register_policy)
        let base = sponsor_record_key(sponsor);
        let created_at_offset = U256::from_limbs([3, 0, 0, 0]);
        let created_at = state.get_storage(SPONSOR_HUB_ADDRESS, base + created_at_offset);
        if created_at.is_zero() {
            return Err(SponsorPrecompileError::NoPolicyRegistered { sponsor });
        }

        state.set_storage(
            SPONSOR_HUB_ADDRESS,
            base + TYPE_OFFSET,
            U256::from(SponsorType::Mint as u8),
        );
        Ok(())
    }

    /// Revokes mint mode (owner-only). Converts back to deposit with zero balance.
    pub fn revoke_mint(
        state: &mut impl PachiState,
        caller: Address,
        sponsor: Address,
    ) -> Result<(), SponsorPrecompileError> {
        let current_owner = Self::get_owner(state);
        if caller != current_owner {
            return Err(SponsorPrecompileError::NotOwner {
                expected: current_owner,
                actual: caller,
            });
        }

        let base = sponsor_record_key(sponsor);
        // Set to Deposit mode, zero balance
        state.set_storage(
            SPONSOR_HUB_ADDRESS,
            base + TYPE_OFFSET,
            U256::from(SponsorType::Deposit as u8),
        );
        state.set_storage(SPONSOR_HUB_ADDRESS, base + BALANCE_OFFSET, U256::ZERO);
        Ok(())
    }

    /// Reads the sponsor type from storage.
    pub fn get_sponsor_type(state: &impl PachiState, sponsor: Address) -> Option<SponsorType> {
        let base = sponsor_record_key(sponsor);
        let status_val = state.get_storage(SPONSOR_HUB_ADDRESS, base + STATUS_OFFSET);
        let type_val = state.get_storage(SPONSOR_HUB_ADDRESS, base + TYPE_OFFSET);

        // Check existence: if status and type are both zero, it could be
        // a registered Deposit/Active sponsor (both = 0) OR non-existent.
        // We check balance or other fields to disambiguate.
        let balance = state.get_storage(SPONSOR_HUB_ADDRESS, base + BALANCE_OFFSET);
        if status_val.is_zero() && type_val.is_zero() && balance.is_zero() {
            // Could still exist if created_at is set
            let created_offset = U256::from_limbs([3, 0, 0, 0]);
            let created = state.get_storage(SPONSOR_HUB_ADDRESS, base + created_offset);
            if created.is_zero() {
                return None;
            }
        }

        SponsorType::from_u8(type_val.as_limbs()[0] as u8)
    }

    /// Reads the sponsor status from storage.
    pub fn get_sponsor_status(state: &impl PachiState, sponsor: Address) -> Option<SponsorStatus> {
        let base = sponsor_record_key(sponsor);
        let created_offset = U256::from_limbs([3, 0, 0, 0]);
        let created = state.get_storage(SPONSOR_HUB_ADDRESS, base + created_offset);
        if created.is_zero() {
            return None;
        }
        let status_val = state.get_storage(SPONSOR_HUB_ADDRESS, base + STATUS_OFFSET);
        SponsorStatus::from_u8(status_val.as_limbs()[0] as u8)
    }
}
