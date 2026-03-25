//! Pachi-specific constants.

use alloy_primitives::Address;

/// System address used as `from` for system transactions. Zero address.
pub const SYSTEM_ADDRESS: Address = Address::ZERO;

// --- Oracle constants ---

/// Ignore source data older than this (seconds).
pub const STALENESS_THRESHOLD: u64 = 10;

/// Regular aggregation cycle (seconds).
pub const HEARTBEAT_INTERVAL: u64 = 1;

/// Below this many active sources, confidence is Degraded.
pub const MIN_ACTIVE_SOURCES: usize = 3;

/// High confidence upper bound (basis points).
pub const SPREAD_HIGH_BPS: u64 = 10;

/// Medium confidence upper bound (basis points).
pub const SPREAD_MEDIUM_BPS: u64 = 50;

/// Max age of a price before precompile marks it Degraded (seconds).
pub const MAX_PRICE_AGE: u64 = 60;

/// Circuit breaker: price change threshold (basis points) — 10%.
pub const CIRCUIT_BREAKER_BPS: u64 = 1000;

/// Circuit breaker: lookback window in blocks.
pub const CIRCUIT_BREAKER_WINDOW: u64 = 5;

/// Price is stored with 8 decimal places.
pub const PRICE_DECIMALS: u8 = 8;

/// Oracle validation tolerance for inter-node differences (basis points) — 1%.
pub const VALIDATION_TOLERANCE_BPS: u64 = 100;

/// Current oracle engine version. Mismatches cause block rejection.
pub const CURRENT_ENGINE_VERSION: u16 = 1;

// --- Session constants ---

/// Default maximum sessions per account.
pub const DEFAULT_MAX_SESSIONS_PER_ACCOUNT: u64 = 10;

/// Default max session duration (30 days in seconds).
pub const DEFAULT_MAX_SESSION_DURATION: u64 = 2_592_000;

/// Max call policies per session.
pub const DEFAULT_MAX_CALL_POLICIES: u64 = 20;

/// Max constraints per call policy.
pub const DEFAULT_MAX_CONSTRAINTS_PER_POLICY: u64 = 10;

/// Max transfer policies per session.
pub const DEFAULT_MAX_TRANSFER_POLICIES: u64 = 10;

/// Max policy size in bytes (4KB).
pub const MAX_POLICY_SIZE: usize = 4096;

// --- Gas cost constants ---

/// Base overhead for `SessionTx` validation.
pub const SESSION_TX_OVERHEAD_GAS: u64 = 15_000;

/// Per-constraint verification gas.
pub const CONSTRAINT_VERIFICATION_GAS: u64 = 500;

/// VRF compute precompile gas.
pub const VRF_COMPUTE_GAS: u64 = 3_000;

/// VRF verify precompile gas.
pub const VRF_VERIFY_GAS: u64 = 1_500;
