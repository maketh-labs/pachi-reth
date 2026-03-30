//! `SessionRegistry`: session lifecycle management.
//!
//! Handles create, revoke, get, list, and validity checks with slot management.

use alloy_primitives::{Address, B256, U256};
use pachi_oracle_precompile::PachiState;
use pachi_primitives::DEFAULT_MAX_SESSIONS_PER_ACCOUNT;

use crate::{
    error::SessionPrecompileError,
    record::{SessionRecord, SessionStatus},
    slots::{read_session_record, write_session_record, write_session_status},
    storage_keys::{session_slots_key, SESSION_REGISTRY_ADDRESS},
};

/// `SessionRegistry`: manages the full session lifecycle.
#[derive(Debug)]
pub struct SessionRegistry;

impl SessionRegistry {
    /// Creates a new session.
    ///
    /// - Finds an available slot (empty, revoked, or expired).
    /// - Writes the session record and updates the authorizer's slot array.
    ///
    /// Returns `(session_hash, replaced_old_hash)`. If an existing revoked or expired
    /// session slot was reused, `replaced_old_hash` contains the old session hash
    /// (for `SessionReplaced` event emission).
    pub fn create_session(
        state: &mut impl PachiState,
        authorizer: Address,
        signer: Address,
        expires_at: u64,
        session_hash: B256,
        block_timestamp: u64,
        max_sessions: u64,
    ) -> Result<(B256, Option<B256>), SessionPrecompileError> {
        let max = if max_sessions == 0 { DEFAULT_MAX_SESSIONS_PER_ACCOUNT } else { max_sessions };

        // Find a slot
        let slot_index = Self::find_available_slot(state, authorizer, block_timestamp, max)?;

        // Read the old hash before overwriting (for replacement tracking)
        let old_hash = Self::read_slot(state, authorizer, slot_index);
        let replaced = if old_hash.is_zero() { None } else { Some(old_hash) };

        let record = SessionRecord {
            status: SessionStatus::Active,
            authorizer,
            signer,
            expires_at,
            created_at: block_timestamp,
        };

        // Write the session record
        write_session_record(state, &session_hash, &record);

        // Write session hash into the slot
        Self::write_slot(state, authorizer, slot_index, session_hash);

        Ok((session_hash, replaced))
    }

    /// Revokes a session (irreversible).
    pub fn revoke_session(
        state: &mut impl PachiState,
        caller: Address,
        session_hash: &B256,
    ) -> Result<(), SessionPrecompileError> {
        let record = read_session_record(state, session_hash)
            .ok_or(SessionPrecompileError::SessionNotFound { hash: *session_hash })?;

        if record.authorizer != caller {
            return Err(SessionPrecompileError::NotAuthorizer {
                expected: record.authorizer,
                actual: caller,
            });
        }

        write_session_status(state, session_hash, SessionStatus::Revoked);
        Ok(())
    }

    /// Gets a session record.
    pub fn get_session(
        state: &impl PachiState,
        session_hash: &B256,
    ) -> Result<SessionRecord, SessionPrecompileError> {
        read_session_record(state, session_hash)
            .ok_or(SessionPrecompileError::SessionNotFound { hash: *session_hash })
    }

    /// Gets all active sessions for an authorizer.
    pub fn get_active_sessions(
        state: &impl PachiState,
        authorizer: Address,
        block_timestamp: u64,
        max_sessions: u64,
    ) -> Vec<B256> {
        let max = if max_sessions == 0 { DEFAULT_MAX_SESSIONS_PER_ACCOUNT } else { max_sessions };

        let mut active = Vec::new();
        for i in 0..max {
            let hash = Self::read_slot(state, authorizer, i);
            if hash.is_zero() {
                continue;
            }
            if let Some(record) = read_session_record(state, &hash) &&
                record.is_active(block_timestamp)
            {
                active.push(hash);
            }
        }
        active
    }

    /// Checks if a session is currently valid.
    pub fn is_valid(state: &impl PachiState, session_hash: &B256, block_timestamp: u64) -> bool {
        read_session_record(state, session_hash)
            .map(|r| r.is_active(block_timestamp))
            .unwrap_or(false)
    }

    /// Finds an available slot for a new session.
    ///
    /// Priority:
    /// 1. Empty slot (zero hash)
    /// 2. Revoked slot (oldest)
    /// 3. Expired slot (earliest expiry)
    fn find_available_slot(
        state: &impl PachiState,
        authorizer: Address,
        block_timestamp: u64,
        max_sessions: u64,
    ) -> Result<u64, SessionPrecompileError> {
        let mut first_revoked: Option<(u64, u64)> = None; // (index, created_at)
        let mut first_expired: Option<(u64, u64)> = None; // (index, expires_at)

        for i in 0..max_sessions {
            let hash = Self::read_slot(state, authorizer, i);

            // Empty slot
            if hash.is_zero() {
                return Ok(i);
            }

            if let Some(record) = read_session_record(state, &hash) {
                let effective = record.effective_status(block_timestamp);

                match effective {
                    SessionStatus::Revoked => {
                        // Take the oldest revoked (smallest created_at)
                        match first_revoked {
                            None => first_revoked = Some((i, record.created_at)),
                            Some((_, oldest_ts)) if record.created_at < oldest_ts => {
                                first_revoked = Some((i, record.created_at));
                            }
                            _ => {}
                        }
                    }
                    SessionStatus::Expired => {
                        // Take the earliest expired (smallest expires_at)
                        match first_expired {
                            None => first_expired = Some((i, record.expires_at)),
                            Some((_, earliest_ts)) if record.expires_at < earliest_ts => {
                                first_expired = Some((i, record.expires_at));
                            }
                            _ => {}
                        }
                    }
                    SessionStatus::Active => {}
                }
            }
        }

        // Revoked first, then expired
        if let Some((idx, _)) = first_revoked {
            return Ok(idx);
        }
        if let Some((idx, _)) = first_expired {
            return Ok(idx);
        }

        Err(SessionPrecompileError::MaxSessionsReached { authorizer })
    }

    /// Reads a session hash from a slot.
    fn read_slot(state: &impl PachiState, authorizer: Address, index: u64) -> B256 {
        let base = session_slots_key(authorizer);
        let val = state.get_storage(SESSION_REGISTRY_ADDRESS, base + U256::from(index));
        B256::from(val.to_be_bytes())
    }

    /// Writes a session hash to a slot.
    fn write_slot(
        state: &mut impl PachiState,
        authorizer: Address,
        index: u64,
        session_hash: B256,
    ) {
        let base = session_slots_key(authorizer);
        state.set_storage(
            SESSION_REGISTRY_ADDRESS,
            base + U256::from(index),
            U256::from_be_bytes(session_hash.0),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pachi_oracle_precompile::MockState;

    fn test_authorizer() -> Address {
        Address::from([0xAAu8; 20])
    }

    fn test_signer() -> Address {
        Address::from([0xBBu8; 20])
    }

    fn test_hash(n: u8) -> B256 {
        B256::from([n; 32])
    }

    #[test]
    fn create_and_get_session() {
        let mut state = MockState::new();
        let authorizer = test_authorizer();
        let hash = test_hash(1);

        SessionRegistry::create_session(&mut state, authorizer, test_signer(), 1000, hash, 100, 10)
            .unwrap()
            .0;

        let record = SessionRegistry::get_session(&state, &hash).unwrap();
        assert_eq!(record.authorizer, authorizer);
        assert_eq!(record.signer, test_signer());
        assert_eq!(record.expires_at, 1000);
        assert_eq!(record.created_at, 100);
        assert_eq!(record.status, SessionStatus::Active);
    }

    #[test]
    fn revoke_session() {
        let mut state = MockState::new();
        let authorizer = test_authorizer();
        let hash = test_hash(1);

        SessionRegistry::create_session(&mut state, authorizer, test_signer(), 1000, hash, 100, 10)
            .unwrap()
            .0;

        SessionRegistry::revoke_session(&mut state, authorizer, &hash).unwrap();

        let record = SessionRegistry::get_session(&state, &hash).unwrap();
        assert_eq!(record.status, SessionStatus::Revoked);
        assert!(!record.is_active(100));
    }

    #[test]
    fn revoke_by_non_authorizer_fails() {
        let mut state = MockState::new();
        let authorizer = test_authorizer();
        let hash = test_hash(1);

        SessionRegistry::create_session(&mut state, authorizer, test_signer(), 1000, hash, 100, 10)
            .unwrap()
            .0;

        let other = Address::from([0xCCu8; 20]);
        let err = SessionRegistry::revoke_session(&mut state, other, &hash).unwrap_err();
        assert!(matches!(err, SessionPrecompileError::NotAuthorizer { .. }));
    }

    #[test]
    fn get_nonexistent_session() {
        let state = MockState::new();
        let err = SessionRegistry::get_session(&state, &test_hash(99)).unwrap_err();
        assert!(matches!(err, SessionPrecompileError::SessionNotFound { .. }));
    }

    #[test]
    fn is_valid_checks_expiration() {
        let mut state = MockState::new();
        let hash = test_hash(1);
        SessionRegistry::create_session(
            &mut state,
            test_authorizer(),
            test_signer(),
            500,
            hash,
            100,
            10,
        )
        .unwrap();

        assert!(SessionRegistry::is_valid(&state, &hash, 400));
        assert!(SessionRegistry::is_valid(&state, &hash, 500));
        assert!(!SessionRegistry::is_valid(&state, &hash, 501));
    }

    #[test]
    fn is_valid_returns_false_after_revoke() {
        let mut state = MockState::new();
        let hash = test_hash(1);
        let authorizer = test_authorizer();
        SessionRegistry::create_session(&mut state, authorizer, test_signer(), 1000, hash, 100, 10)
            .unwrap()
            .0;

        SessionRegistry::revoke_session(&mut state, authorizer, &hash).unwrap();
        assert!(!SessionRegistry::is_valid(&state, &hash, 200));
    }

    #[test]
    fn get_active_sessions() {
        let mut state = MockState::new();
        let authorizer = test_authorizer();

        // Create 3 sessions
        for i in 1..=3u8 {
            SessionRegistry::create_session(
                &mut state,
                authorizer,
                Address::from([i; 20]),
                1000 + i as u64 * 100,
                test_hash(i),
                100,
                10,
            )
            .unwrap();
        }

        let active = SessionRegistry::get_active_sessions(&state, authorizer, 200, 10);
        assert_eq!(active.len(), 3);

        // Revoke one
        SessionRegistry::revoke_session(&mut state, authorizer, &test_hash(2)).unwrap();
        let active = SessionRegistry::get_active_sessions(&state, authorizer, 200, 10);
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn slot_management_fills_empty_slots() {
        let mut state = MockState::new();
        let authorizer = test_authorizer();

        for i in 1..=3u8 {
            SessionRegistry::create_session(
                &mut state,
                authorizer,
                Address::from([i; 20]),
                1000,
                test_hash(i),
                100,
                3,
            )
            .unwrap();
        }

        // All slots full → should fail
        let err = SessionRegistry::create_session(
            &mut state,
            authorizer,
            test_signer(),
            1000,
            test_hash(4),
            100,
            3,
        )
        .unwrap_err();
        assert!(matches!(err, SessionPrecompileError::MaxSessionsReached { .. }));
    }

    #[test]
    fn slot_replacement_revoked_first() {
        let mut state = MockState::new();
        let authorizer = test_authorizer();

        // Fill 3 slots
        for i in 1..=3u8 {
            SessionRegistry::create_session(
                &mut state,
                authorizer,
                Address::from([i; 20]),
                2000,
                test_hash(i),
                100,
                3,
            )
            .unwrap();
        }

        // Revoke slot 2
        SessionRegistry::revoke_session(&mut state, authorizer, &test_hash(2)).unwrap();

        // Create new session → should replace revoked slot
        SessionRegistry::create_session(
            &mut state,
            authorizer,
            Address::from([0xDD; 20]),
            2000,
            test_hash(10),
            200,
            3,
        )
        .unwrap();

        let active = SessionRegistry::get_active_sessions(&state, authorizer, 300, 3);
        assert_eq!(active.len(), 3);
    }

    #[test]
    fn slot_replacement_expired() {
        let mut state = MockState::new();
        let authorizer = test_authorizer();

        // Fill 3 slots, one expires at 500
        SessionRegistry::create_session(
            &mut state,
            authorizer,
            Address::from([1; 20]),
            500,
            test_hash(1),
            100,
            3,
        )
        .unwrap();
        for i in 2..=3u8 {
            SessionRegistry::create_session(
                &mut state,
                authorizer,
                Address::from([i; 20]),
                2000,
                test_hash(i),
                100,
                3,
            )
            .unwrap();
        }

        // At time 600, hash 1 is expired → new session should take its slot
        SessionRegistry::create_session(
            &mut state,
            authorizer,
            Address::from([0xDD; 20]),
            2000,
            test_hash(10),
            600,
            3,
        )
        .unwrap();

        let active = SessionRegistry::get_active_sessions(&state, authorizer, 600, 3);
        assert_eq!(active.len(), 3);
    }
}
