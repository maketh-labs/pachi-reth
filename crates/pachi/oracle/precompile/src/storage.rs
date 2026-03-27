//! Oracle precompile storage operations.
//!
//! Storage layout (at address `0x0802`):
//! - `slot(asset_id * 2)`     → price: `U256` (8 decimals)
//! - `slot(asset_id * 2 + 1)` → packed `{ timestamp: u64 [0:63], confidence: u8 [64:71] }`

use alloy_primitives::{address, Address, U256};
use pachi_primitives::{AssetId, Confidence, PriceSnapshot, SnapshotEntry, MAX_PRICE_AGE};

use crate::{error::OraclePrecompileError, state::PachiState};

/// The `PriceOracle` precompile address.
pub const ORACLE_PRECOMPILE_ADDRESS: Address =
    address!("0x0000000000000000000000000000000000000802");

/// A price entry as stored on-chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceEntry {
    /// USD price with 8 decimal places.
    pub price: U256,
    /// Unix timestamp of the price observation.
    pub timestamp: u64,
    /// Confidence level.
    pub confidence: Confidence,
}

impl PriceEntry {
    /// Returns the storage slot for the price of the given asset.
    fn price_slot(asset_id: AssetId) -> U256 {
        U256::from(asset_id.as_u8()) * U256::from(2)
    }

    /// Returns the storage slot for the packed timestamp/confidence of the given asset.
    fn packed_slot(asset_id: AssetId) -> U256 {
        U256::from(asset_id.as_u8()) * U256::from(2) + U256::from(1)
    }

    /// Packs timestamp and confidence into a single U256.
    fn pack_timestamp_confidence(timestamp: u64, confidence: Confidence) -> U256 {
        U256::from(timestamp) | (U256::from(confidence.as_u8()) << 64)
    }

    /// Unpacks timestamp and confidence from a single U256.
    fn unpack_timestamp_confidence(packed: U256) -> (u64, Confidence) {
        let timestamp = packed.as_limbs()[0];
        let masked: U256 = (packed >> 64) & U256::from(0xFFu64);
        let confidence_byte = masked.as_limbs()[0] as u8;
        let confidence = Confidence::from_u8(confidence_byte).unwrap_or(Confidence::Unavailable);
        (timestamp, confidence)
    }

    /// Reads a price entry from state for the given asset.
    pub fn read(state: &impl PachiState, asset_id: AssetId) -> Self {
        let price = state.get_storage(ORACLE_PRECOMPILE_ADDRESS, Self::price_slot(asset_id));
        let packed = state.get_storage(ORACLE_PRECOMPILE_ADDRESS, Self::packed_slot(asset_id));
        let (timestamp, confidence) = Self::unpack_timestamp_confidence(packed);
        Self { price, timestamp, confidence }
    }

    /// Writes this price entry to state for the given asset.
    pub fn write(&self, state: &mut impl PachiState, asset_id: AssetId) {
        state.set_storage(ORACLE_PRECOMPILE_ADDRESS, Self::price_slot(asset_id), self.price);
        state.set_storage(
            ORACLE_PRECOMPILE_ADDRESS,
            Self::packed_slot(asset_id),
            Self::pack_timestamp_confidence(self.timestamp, self.confidence),
        );
    }
}

/// Reads the current price for an asset, applying staleness degradation.
///
/// - Unsupported `asset_id` → returns `Err(UnsupportedAsset)`.
/// - Unavailable asset → returns `(price=last_valid, timestamp=last_valid,
///   confidence=Unavailable)`. Does NOT error; gatekeeping is the consumer's responsibility.
/// - Staleness: if `block_timestamp - entry.timestamp > MAX_PRICE_AGE`, confidence is degraded to
///   at least `Degraded`.
pub fn get_price(
    state: &impl PachiState,
    asset_id: u8,
    block_timestamp: u64,
) -> Result<(U256, u64, Confidence), OraclePrecompileError> {
    let asset =
        AssetId::from_u8(asset_id).ok_or(OraclePrecompileError::UnsupportedAsset { asset_id })?;

    let entry = PriceEntry::read(state, asset);
    let mut confidence = entry.confidence;

    // Never-set state (genesis): treat as Unavailable
    if entry.price.is_zero() && entry.timestamp == 0 {
        confidence = Confidence::Unavailable;
    } else if entry.timestamp > 0 && block_timestamp.saturating_sub(entry.timestamp) > MAX_PRICE_AGE
    {
        // Staleness degradation at query time
        confidence = confidence.worse(Confidence::Degraded);
    }

    Ok((entry.price, entry.timestamp, confidence))
}

/// Reads prices for multiple assets in a single call.
pub fn get_price_batch(
    state: &impl PachiState,
    asset_ids: &[u8],
    block_timestamp: u64,
) -> Result<Vec<(U256, u64, Confidence)>, OraclePrecompileError> {
    asset_ids.iter().map(|&id| get_price(state, id, block_timestamp)).collect()
}

/// Checks whether an asset ID is supported.
pub const fn is_supported(asset_id: u8) -> bool {
    AssetId::from_u8(asset_id).is_some()
}

/// Applies an oracle update snapshot to state.
///
/// - `Updated { price, timestamp, confidence }` → overwrites price, timestamp, confidence.
/// - `Unavailable` → keeps existing price/timestamp, sets confidence to `Unavailable`.
///
/// Validates that the snapshot contains exactly the supported assets.
pub fn apply_oracle_update(
    state: &mut impl PachiState,
    snapshot: &PriceSnapshot,
) -> Result<(), OraclePrecompileError> {
    // Validate: all supported assets present
    for asset in AssetId::ALL {
        if !snapshot.entries.contains_key(&asset) {
            return Err(OraclePrecompileError::MissingAsset(asset));
        }
    }

    // Validate: no unsupported assets
    for asset in snapshot.entries.keys() {
        if !AssetId::ALL.contains(asset) {
            return Err(OraclePrecompileError::SnapshotUnsupportedAsset { asset_id: asset.as_u8() });
        }
    }

    // Apply updates
    for (asset, entry) in &snapshot.entries {
        match entry {
            SnapshotEntry::Updated { price, timestamp, confidence } => {
                let new_entry =
                    PriceEntry { price: *price, timestamp: *timestamp, confidence: *confidence };
                new_entry.write(state, *asset);
            }
            SnapshotEntry::Unavailable => {
                // Keep price/timestamp, only change confidence to Unavailable
                let mut existing = PriceEntry::read(state, *asset);
                existing.confidence = Confidence::Unavailable;
                existing.write(state, *asset);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MockState;
    use alloy_primitives::U256;
    use pachi_primitives::AssetId;
    use std::collections::BTreeMap;

    fn btc_price() -> U256 {
        U256::from(50_000_00000000u64) // $50,000.00
    }

    fn eth_price() -> U256 {
        U256::from(3_000_00000000u64) // $3,000.00
    }

    fn full_snapshot(price: U256, timestamp: u64) -> PriceSnapshot {
        let mut entries = BTreeMap::new();
        for asset in AssetId::ALL {
            entries.insert(
                asset,
                SnapshotEntry::Updated { price, timestamp, confidence: Confidence::High },
            );
        }
        PriceSnapshot { entries, engine_version: 1 }
    }

    #[test]
    fn write_and_read_price_entry() {
        let mut state = MockState::new();
        let entry =
            PriceEntry { price: btc_price(), timestamp: 1_000_000, confidence: Confidence::High };
        entry.write(&mut state, AssetId::BTC);

        let read = PriceEntry::read(&state, AssetId::BTC);
        assert_eq!(read, entry);
    }

    #[test]
    fn pack_unpack_timestamp_confidence() {
        for conf in
            [Confidence::High, Confidence::Medium, Confidence::Degraded, Confidence::Unavailable]
        {
            let ts = 1_710_000_000u64;
            let packed = PriceEntry::pack_timestamp_confidence(ts, conf);
            let (ts2, conf2) = PriceEntry::unpack_timestamp_confidence(packed);
            assert_eq!(ts, ts2);
            assert_eq!(conf, conf2);
        }
    }

    #[test]
    fn get_price_unsupported_asset() {
        let state = MockState::new();
        let err = get_price(&state, 0xFF, 1_000_000).unwrap_err();
        assert!(matches!(err, OraclePrecompileError::UnsupportedAsset { asset_id: 0xFF }));
    }

    #[test]
    fn get_price_never_set_returns_zeros_unavailable() {
        let state = MockState::new();
        let (price, ts, conf) = get_price(&state, AssetId::BTC.as_u8(), 1_000_000).unwrap();
        assert_eq!(price, U256::ZERO);
        assert_eq!(ts, 0);
        assert_eq!(conf, Confidence::Unavailable);
    }

    #[test]
    fn get_price_fresh() {
        let mut state = MockState::new();
        let entry =
            PriceEntry { price: btc_price(), timestamp: 1_000_000, confidence: Confidence::High };
        entry.write(&mut state, AssetId::BTC);

        // Query within MAX_PRICE_AGE
        let (price, ts, conf) =
            get_price(&state, AssetId::BTC.as_u8(), 1_000_000 + MAX_PRICE_AGE - 1).unwrap();
        assert_eq!(price, btc_price());
        assert_eq!(ts, 1_000_000);
        assert_eq!(conf, Confidence::High);
    }

    #[test]
    fn get_price_stale_degrades() {
        let mut state = MockState::new();
        let entry =
            PriceEntry { price: btc_price(), timestamp: 1_000_000, confidence: Confidence::High };
        entry.write(&mut state, AssetId::BTC);

        // Query beyond MAX_PRICE_AGE
        let (price, ts, conf) =
            get_price(&state, AssetId::BTC.as_u8(), 1_000_000 + MAX_PRICE_AGE + 1).unwrap();
        assert_eq!(price, btc_price());
        assert_eq!(ts, 1_000_000);
        assert_eq!(conf, Confidence::Degraded);
    }

    #[test]
    fn get_price_stale_does_not_improve_unavailable() {
        let mut state = MockState::new();
        let entry = PriceEntry {
            price: btc_price(),
            timestamp: 1_000_000,
            confidence: Confidence::Unavailable,
        };
        entry.write(&mut state, AssetId::BTC);

        // Staleness degradation: Unavailable.worse(Degraded) = Unavailable
        let (_, _, conf) =
            get_price(&state, AssetId::BTC.as_u8(), 1_000_000 + MAX_PRICE_AGE + 1).unwrap();
        assert_eq!(conf, Confidence::Unavailable);
    }

    #[test]
    fn get_price_batch_success() {
        let mut state = MockState::new();
        PriceEntry { price: btc_price(), timestamp: 1_000, confidence: Confidence::High }
            .write(&mut state, AssetId::BTC);
        PriceEntry { price: eth_price(), timestamp: 1_000, confidence: Confidence::Medium }
            .write(&mut state, AssetId::ETH);

        let results =
            get_price_batch(&state, &[AssetId::BTC.as_u8(), AssetId::ETH.as_u8()], 1_000 + 10)
                .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, btc_price());
        assert_eq!(results[0].2, Confidence::High);
        assert_eq!(results[1].0, eth_price());
        assert_eq!(results[1].2, Confidence::Medium);
    }

    #[test]
    fn get_price_batch_with_unsupported() {
        let state = MockState::new();
        let err = get_price_batch(&state, &[0x01, 0xFF], 1_000).unwrap_err();
        assert!(matches!(err, OraclePrecompileError::UnsupportedAsset { asset_id: 0xFF }));
    }

    #[test]
    fn is_supported_valid() {
        assert!(is_supported(0x01));
        assert!(is_supported(0x06));
    }

    #[test]
    fn is_supported_invalid() {
        assert!(!is_supported(0x00));
        assert!(!is_supported(0x07));
        assert!(!is_supported(0xFF));
    }

    #[test]
    fn apply_oracle_update_full() {
        let mut state = MockState::new();
        let snapshot = full_snapshot(btc_price(), 1_000);

        apply_oracle_update(&mut state, &snapshot).unwrap();

        for asset in AssetId::ALL {
            let entry = PriceEntry::read(&state, asset);
            assert_eq!(entry.price, btc_price());
            assert_eq!(entry.timestamp, 1_000);
            assert_eq!(entry.confidence, Confidence::High);
        }
    }

    #[test]
    fn apply_oracle_update_unavailable_keeps_price() {
        let mut state = MockState::new();

        // First: set a valid price
        let snapshot1 = full_snapshot(btc_price(), 1_000);
        apply_oracle_update(&mut state, &snapshot1).unwrap();

        // Second: BTC becomes Unavailable
        let mut entries = BTreeMap::new();
        for asset in AssetId::ALL {
            if asset == AssetId::BTC {
                entries.insert(asset, SnapshotEntry::Unavailable);
            } else {
                entries.insert(
                    asset,
                    SnapshotEntry::Updated {
                        price: eth_price(),
                        timestamp: 2_000,
                        confidence: Confidence::High,
                    },
                );
            }
        }
        let snapshot2 = PriceSnapshot { entries, engine_version: 1 };
        apply_oracle_update(&mut state, &snapshot2).unwrap();

        // BTC: price/timestamp preserved, confidence = Unavailable
        let btc = PriceEntry::read(&state, AssetId::BTC);
        assert_eq!(btc.price, btc_price());
        assert_eq!(btc.timestamp, 1_000);
        assert_eq!(btc.confidence, Confidence::Unavailable);
    }

    #[test]
    fn apply_oracle_update_missing_asset() {
        let mut state = MockState::new();
        let mut entries = BTreeMap::new();
        // Only include BTC, missing others
        entries.insert(
            AssetId::BTC,
            SnapshotEntry::Updated {
                price: btc_price(),
                timestamp: 1_000,
                confidence: Confidence::High,
            },
        );
        let snapshot = PriceSnapshot { entries, engine_version: 1 };
        let err = apply_oracle_update(&mut state, &snapshot).unwrap_err();
        assert!(matches!(err, OraclePrecompileError::MissingAsset(_)));
    }

    #[test]
    fn apply_multiple_updates_overwrites() {
        let mut state = MockState::new();
        let snap1 = full_snapshot(btc_price(), 1_000);
        apply_oracle_update(&mut state, &snap1).unwrap();

        let new_price = U256::from(55_000_00000000u64);
        let snap2 = full_snapshot(new_price, 2_000);
        apply_oracle_update(&mut state, &snap2).unwrap();

        let entry = PriceEntry::read(&state, AssetId::BTC);
        assert_eq!(entry.price, new_price);
        assert_eq!(entry.timestamp, 2_000);
    }

    #[test]
    fn genesis_state_all_zeros_unavailable() {
        let state = MockState::new();
        for asset in AssetId::ALL {
            // get_price treats (price=0, timestamp=0) as never-set → Unavailable
            let (price, ts, conf) = get_price(&state, asset.as_u8(), 100).unwrap();
            assert_eq!(price, U256::ZERO);
            assert_eq!(ts, 0);
            assert_eq!(conf, Confidence::Unavailable);
        }
    }
}
