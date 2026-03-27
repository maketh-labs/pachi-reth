//! State abstraction for precompile storage access.
//!
//! Layer 3 (pachi-evm) bridges this trait to revm's `Database`.

use alloy_primitives::{Address, U256};

/// Abstraction over EVM account storage for precompile state access.
///
/// Each precompile operates on its own contract address. The EVM captures
/// all reads/writes and applies them to the state trie.
pub trait PachiState {
    /// Read a storage slot from a contract address.
    fn get_storage(&self, address: Address, slot: U256) -> U256;

    /// Write a storage slot to a contract address.
    fn set_storage(&mut self, address: Address, slot: U256, value: U256);
}

/// In-memory state implementation for testing.
#[cfg(any(test, feature = "test-utils"))]
pub use mock::MockState;

#[cfg(any(test, feature = "test-utils"))]
mod mock {
    use super::*;
    use alloy_primitives::map::HashMap;

    /// Simple in-memory state for unit tests.
    #[derive(Debug, Default)]
    pub struct MockState {
        storage: HashMap<(Address, U256), U256>,
    }

    impl MockState {
        /// Creates a new empty mock state.
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl PachiState for MockState {
        fn get_storage(&self, address: Address, slot: U256) -> U256 {
            self.storage.get(&(address, slot)).copied().unwrap_or(U256::ZERO)
        }

        fn set_storage(&mut self, address: Address, slot: U256, value: U256) {
            self.storage.insert((address, slot), value);
        }
    }
}
