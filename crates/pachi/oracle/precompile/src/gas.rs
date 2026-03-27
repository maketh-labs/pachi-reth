//! Gas cost constants for oracle precompile functions.

/// Gas cost for `getPrice(uint8)`.
pub const GAS_GET_PRICE: u64 = 200;

/// Base gas cost for `getPriceBatch(uint8[])`.
pub const GAS_GET_PRICE_BATCH_BASE: u64 = 200;

/// Per-asset gas cost for `getPriceBatch`.
pub const GAS_GET_PRICE_BATCH_PER_ASSET: u64 = 100;

/// Gas cost for `isSupported(uint8)`.
pub const GAS_IS_SUPPORTED: u64 = 100;

/// Computes total gas for a batch price query.
pub const fn gas_get_price_batch(asset_count: u64) -> u64 {
    GAS_GET_PRICE_BATCH_BASE + GAS_GET_PRICE_BATCH_PER_ASSET * asset_count
}
