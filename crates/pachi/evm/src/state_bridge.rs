//! Bridge between `PachiState` (Layer 1 abstraction) and revm's `EvmInternals`.
//!
//! Allows precompile state machines to read/write EVM storage through the journaled state.

use alloy_evm::EvmInternals;
use alloy_primitives::{Address, Log, LogData, B256, U256};
use core::cell::RefCell;
use pachi_oracle_precompile::PachiState;

/// Wraps `EvmInternals` to implement `PachiState`.
///
/// Uses `RefCell` for interior mutability because `PachiState::get_storage` takes `&self`
/// but revm's `sload` requires `&mut` for warm/cold tracking. This is safe because
/// precompile execution is single-threaded.
pub(crate) struct EvmStateBridge<'a> {
    internals: RefCell<EvmInternals<'a>>,
}

impl<'a> EvmStateBridge<'a> {
    /// Creates a new bridge, taking ownership of the EVM internals.
    pub(crate) const fn new(internals: EvmInternals<'a>) -> Self {
        Self { internals: RefCell::new(internals) }
    }

    /// Returns the block timestamp from the EVM environment.
    pub(crate) fn block_timestamp(&self) -> u64 {
        self.internals.borrow().block_timestamp().saturating_to()
    }

    /// Emits an EVM log (event) with the given address, topics, and data.
    pub(crate) fn emit_log(&self, address: Address, topics: Vec<B256>, data: Vec<u8>) {
        let log = Log { address, data: LogData::new_unchecked(topics, data.into()) };
        self.internals.borrow_mut().log(log);
    }
}

impl PachiState for EvmStateBridge<'_> {
    fn get_storage(&self, address: Address, slot: U256) -> U256 {
        self.internals.borrow_mut().sload(address, slot).map(|v| v.data).unwrap_or(U256::ZERO)
    }

    fn set_storage(&mut self, address: Address, slot: U256, value: U256) {
        // Errors from sstore (e.g., DB errors) are non-recoverable at the precompile level.
        // The EVM catches these and reverts the transaction.
        let _ = self.internals.borrow_mut().sstore(address, slot, value);
    }
}
