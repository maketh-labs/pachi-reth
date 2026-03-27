//! Gas cost constants for sponsor precompile functions.

/// Gas overhead for sponsored tx (deposit mode).
pub const GAS_SPONSOR_OVERHEAD_DEPOSIT: u64 = 20_000;

/// Gas overhead for sponsored tx (mint mode).
pub const GAS_SPONSOR_OVERHEAD_MINT: u64 = 15_000;

/// Gas cost for policy registration.
pub const GAS_SPONSOR_REGISTER: u64 = 100_000;

/// Gas cost for deposit.
pub const GAS_SPONSOR_DEPOSIT: u64 = 20_000;

/// Gas cost for withdraw.
pub const GAS_SPONSOR_WITHDRAW: u64 = 20_000;

/// Gas cost for approveMint/revokeMint.
pub const GAS_SPONSOR_MINT_TOGGLE: u64 = 10_000;

/// Gas cost for transferOwnership.
pub const GAS_SPONSOR_TRANSFER_OWNERSHIP: u64 = 10_000;

/// Gas cost for view functions.
pub const GAS_SPONSOR_VIEW: u64 = 2_600;

/// Gas cost for canSponsor (more complex view).
pub const GAS_SPONSOR_CAN_SPONSOR: u64 = 5_000;
