//! Provider-based state bridge for pool-level validation.
//!
//! Unlike the EVM-level [`EvmStateBridge`] in `pachi-evm`, this bridge reads
//! state from the reth storage provider for mempool validation. It does NOT
//! need `EvmInternals` and is read-only.

use alloy_primitives::{Address, U256};
use pachi_oracle_precompile::PachiState;
use reth_storage_api::StateProviderBox;

/// A read-only [`PachiState`] implementation backed by a reth storage provider.
///
/// Used for mempool-level validation of custom tx types (session, sponsor).
/// State writes are no-ops since pool validation doesn't modify state.
// StateProviderBox doesn't impl Debug; manual impl avoids derive constraint.
pub struct ProviderStateBridge<'a> {
    provider: &'a StateProviderBox,
}

impl std::fmt::Debug for ProviderStateBridge<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderStateBridge").finish()
    }
}

impl<'a> ProviderStateBridge<'a> {
    /// Creates a new bridge over the given state provider.
    pub const fn new(provider: &'a StateProviderBox) -> Self {
        Self { provider }
    }
}

impl PachiState for ProviderStateBridge<'_> {
    fn get_storage(&self, address: Address, slot: U256) -> U256 {
        self.provider.storage(address, slot.into()).ok().flatten().unwrap_or(U256::ZERO)
    }

    fn set_storage(&mut self, _address: Address, _slot: U256, _value: U256) {
        // Pool validation is read-only — no state writes
    }
}
