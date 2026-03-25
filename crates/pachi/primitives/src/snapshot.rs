//! Oracle price snapshot types.

use alloy_primitives::U256;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{AssetId, Confidence};

/// Per-asset state held by the oracle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetState {
    /// Price data is available.
    Available {
        /// 8-decimal USD price.
        price: U256,
        /// Unix timestamp (seconds).
        timestamp: u64,
        /// Data quality confidence level.
        confidence: Confidence,
    },
    /// All sources down, no current price data.
    Unavailable,
}

/// A single entry in a block's price snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotEntry {
    /// New price data.
    Updated {
        /// 8-decimal USD price.
        price: U256,
        /// Unix timestamp (seconds).
        timestamp: u64,
        /// Data quality confidence level.
        confidence: Confidence,
    },
    /// Asset is unavailable this block.
    Unavailable,
}

/// A block-level price snapshot included in the `OracleUpdate` system transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceSnapshot {
    /// Per-asset entries. Must contain all supported assets.
    pub entries: BTreeMap<AssetId, SnapshotEntry>,
    /// Oracle engine version.
    pub engine_version: u16,
}

impl PriceSnapshot {
    /// Creates a snapshot where all assets are unavailable.
    pub fn all_unavailable(engine_version: u16) -> Self {
        let entries = AssetId::ALL.into_iter().map(|id| (id, SnapshotEntry::Unavailable)).collect();
        Self { entries, engine_version }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CURRENT_ENGINE_VERSION;

    #[test]
    fn all_unavailable_contains_all_assets() {
        let snapshot = PriceSnapshot::all_unavailable(CURRENT_ENGINE_VERSION);
        assert_eq!(snapshot.entries.len(), AssetId::ALL.len());
        for asset in AssetId::ALL {
            assert_eq!(snapshot.entries[&asset], SnapshotEntry::Unavailable);
        }
        assert_eq!(snapshot.engine_version, CURRENT_ENGINE_VERSION);
    }

    #[test]
    fn snapshot_serde_roundtrip() {
        let mut entries = BTreeMap::new();
        entries.insert(
            AssetId::BTC,
            SnapshotEntry::Updated {
                price: U256::from(5_000_000_000_000u64), // 50000.00000000
                timestamp: 1700000000,
                confidence: Confidence::High,
            },
        );
        entries.insert(AssetId::ETH, SnapshotEntry::Unavailable);
        let snapshot = PriceSnapshot { entries, engine_version: 1 };

        let json = serde_json::to_string(&snapshot).unwrap();
        let deserialized: PriceSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot, deserialized);
    }

    #[test]
    fn asset_state_variants() {
        let available = AssetState::Available {
            price: U256::from(100),
            timestamp: 1000,
            confidence: Confidence::High,
        };
        let unavailable = AssetState::Unavailable;

        // Just verify construction and pattern matching
        assert!(matches!(available, AssetState::Available { .. }));
        assert!(matches!(unavailable, AssetState::Unavailable));
    }
}
