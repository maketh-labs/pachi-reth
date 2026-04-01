//! Session record storage slot layout.
//!
//! A session record occupies 5 storage slots starting from the base slot.

use alloy_primitives::{Address, B256, U256};
use pachi_oracle_precompile::PachiState;

use crate::{
    record::{SessionRecord, SessionStatus},
    storage_keys::{session_record_key, SESSION_REGISTRY_ADDRESS},
};

/// Slot offsets within a session record.
const STATUS: U256 = U256::from_limbs([0, 0, 0, 0]);
const AUTHORIZER: U256 = U256::from_limbs([1, 0, 0, 0]);
const SIGNER: U256 = U256::from_limbs([2, 0, 0, 0]);
const EXPIRES_AT: U256 = U256::from_limbs([3, 0, 0, 0]);
const CREATED_AT: U256 = U256::from_limbs([4, 0, 0, 0]);

/// Writes a session record to state.
pub(crate) fn write_session_record(
    state: &mut impl PachiState,
    session_hash: &B256,
    record: &SessionRecord,
) {
    let base = session_record_key(session_hash);
    let addr = SESSION_REGISTRY_ADDRESS;

    state.set_storage(addr, base + STATUS, U256::from(record.status as u8));
    state.set_storage(
        addr,
        base + AUTHORIZER,
        U256::from_be_bytes(record.authorizer.into_word().0),
    );
    state.set_storage(addr, base + SIGNER, U256::from_be_bytes(record.signer.into_word().0));
    state.set_storage(addr, base + EXPIRES_AT, U256::from(record.expires_at));
    state.set_storage(addr, base + CREATED_AT, U256::from(record.created_at));
}

/// Reads a session record from state.
///
/// Returns `None` if the session doesn't exist (all zero storage).
pub fn read_session_record(state: &impl PachiState, session_hash: &B256) -> Option<SessionRecord> {
    let base = session_record_key(session_hash);
    let addr = SESSION_REGISTRY_ADDRESS;

    let authorizer_val = state.get_storage(addr, base + AUTHORIZER);
    if authorizer_val.is_zero() {
        return None;
    }

    let status_val = state.get_storage(addr, base + STATUS);
    let status =
        SessionStatus::from_u8(status_val.as_limbs()[0] as u8).unwrap_or(SessionStatus::Active);

    let authorizer = Address::from_word(B256::from(authorizer_val.to_be_bytes()));
    let signer_val = state.get_storage(addr, base + SIGNER);
    let signer = Address::from_word(B256::from(signer_val.to_be_bytes()));

    let expires_at = state.get_storage(addr, base + EXPIRES_AT).as_limbs()[0];
    let created_at = state.get_storage(addr, base + CREATED_AT).as_limbs()[0];

    Some(SessionRecord { status, authorizer, signer, expires_at, created_at })
}

/// Updates only the status field of a session record.
pub(crate) fn write_session_status(
    state: &mut impl PachiState,
    session_hash: &B256,
    status: SessionStatus,
) {
    let base = session_record_key(session_hash);
    state.set_storage(SESSION_REGISTRY_ADDRESS, base + STATUS, U256::from(status as u8));
}
