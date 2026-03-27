//! Session nonce management.
//!
//! Each session has an independent nonce space from the authorizer's main nonce.

use alloy_primitives::{Address, B256, U256};
use pachi_oracle_precompile::PachiState;

use crate::storage_keys::{session_nonce_key, SESSION_REGISTRY_ADDRESS};

/// Session nonce operations.
#[derive(Debug)]
pub struct SessionNonce;

impl SessionNonce {
    /// Reads the current session nonce.
    pub fn get(state: &impl PachiState, authorizer: Address, session_hash: &B256) -> u64 {
        let slot = session_nonce_key(authorizer, session_hash);
        state.get_storage(SESSION_REGISTRY_ADDRESS, slot).as_limbs()[0]
    }

    /// Increments the session nonce by 1.
    pub fn increment(state: &mut impl PachiState, authorizer: Address, session_hash: &B256) {
        let slot = session_nonce_key(authorizer, session_hash);
        let current = state.get_storage(SESSION_REGISTRY_ADDRESS, slot);
        state.set_storage(SESSION_REGISTRY_ADDRESS, slot, current + U256::from(1));
    }

    /// Checks that the provided nonce matches the current nonce.
    pub fn check(
        state: &impl PachiState,
        authorizer: Address,
        session_hash: &B256,
        expected: u64,
    ) -> bool {
        Self::get(state, authorizer, session_hash) == expected
    }
}
