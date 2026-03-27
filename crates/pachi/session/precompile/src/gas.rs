//! Gas cost constants for session precompile functions.

/// Gas cost for `createSession`.
pub const GAS_SESSION_CREATE: u64 = 60_000;

/// Gas cost for `revokeSession`.
pub const GAS_SESSION_REVOKE: u64 = 10_000;

/// Gas cost for `getSession` (view).
pub const GAS_SESSION_GET: u64 = 2_600;

/// Base gas cost for `getActiveSessions` (view).
pub const GAS_SESSION_LIST_BASE: u64 = 2_600;

/// Per-session gas cost for `getActiveSessions`.
pub const GAS_SESSION_LIST_PER: u64 = 200;

/// Gas cost for `isValid` (view).
pub const GAS_SESSION_IS_VALID: u64 = 2_600;

/// Per-limit-state update gas cost (SSTORE warm).
pub const GAS_LIMIT_STATE_UPDATE: u64 = 5_000;
