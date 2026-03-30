//! Storage key computation for session precompile.

use alloy_primitives::{address, keccak256, Address, B256, U256};

/// The `SessionRegistry` precompile address.
pub const SESSION_REGISTRY_ADDRESS: Address =
    address!("0x0000000000000000000000000000000000000800");

/// Computes the storage key for a session record.
///
/// `keccak256("session_record", session_hash)`
pub(crate) fn session_record_key(session_hash: &B256) -> U256 {
    let hash = keccak256([b"session_record".as_slice(), session_hash.as_slice()].concat());
    U256::from_be_bytes(hash.0)
}

/// Computes the storage key for session slots (per authorizer).
///
/// `keccak256("session_slots", authorizer)`
pub(crate) fn session_slots_key(authorizer: Address) -> U256 {
    let hash = keccak256([b"session_slots".as_slice(), authorizer.as_slice()].concat());
    U256::from_be_bytes(hash.0)
}

/// Computes the storage key for a session nonce.
///
/// `keccak256("session_nonce", authorizer, session_hash)`
pub(crate) fn session_nonce_key(authorizer: Address, session_hash: &B256) -> U256 {
    let hash = keccak256(
        [b"session_nonce".as_slice(), authorizer.as_slice(), session_hash.as_slice()].concat(),
    );
    U256::from_be_bytes(hash.0)
}

/// Computes a limit state storage key.
///
/// `keccak256("limit_state", owner_hash, limit_context, limit_index)`
pub fn limit_state_key(owner_hash: &B256, limit_context: &[u8], limit_index: u8) -> U256 {
    let hash = keccak256(
        [b"limit_state".as_slice(), owner_hash.as_slice(), limit_context, &[limit_index]].concat(),
    );
    U256::from_be_bytes(hash.0)
}
