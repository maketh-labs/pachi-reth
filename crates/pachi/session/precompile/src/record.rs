//! Session record types.

use alloy_primitives::Address;

/// Status of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SessionStatus {
    /// Session is active and usable.
    Active = 0,
    /// Session has been revoked (irreversible).
    Revoked = 1,
    /// Session has expired (`block.timestamp > expires_at`).
    Expired = 2,
}

impl SessionStatus {
    /// Converts a `u8` to a `SessionStatus`.
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Active),
            1 => Some(Self::Revoked),
            2 => Some(Self::Expired),
            _ => None,
        }
    }
}

/// On-chain session record.
///
/// Policy content is NOT stored on-chain; only the session hash and metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    /// Session status.
    pub status: SessionStatus,
    /// Delegating wallet.
    pub authorizer: Address,
    /// Session key address (ephemeral key).
    pub signer: Address,
    /// Expiration (unix timestamp).
    pub expires_at: u64,
    /// Creation time (unix timestamp).
    pub created_at: u64,
}

impl SessionRecord {
    /// Returns the effective status considering expiration.
    pub fn effective_status(&self, block_timestamp: u64) -> SessionStatus {
        // Priority: Revoked > Expired > Active
        if self.status == SessionStatus::Revoked {
            return SessionStatus::Revoked;
        }
        if block_timestamp > self.expires_at {
            return SessionStatus::Expired;
        }
        self.status
    }

    /// Returns `true` if the session is active at the given timestamp.
    pub fn is_active(&self, block_timestamp: u64) -> bool {
        self.effective_status(block_timestamp) == SessionStatus::Active
    }
}
