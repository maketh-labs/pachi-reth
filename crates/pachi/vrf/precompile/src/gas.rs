//! Gas cost constants for VRF precompiles.

/// Gas cost for `VRF_COMPUTE` (0x0101). Fixed, ecrecover-level.
pub const GAS_VRF_COMPUTE: u64 = 3_000;

/// Gas cost for `VRF_VERIFY` (0x0102). Fixed.
pub const GAS_VRF_VERIFY: u64 = 1_500;
