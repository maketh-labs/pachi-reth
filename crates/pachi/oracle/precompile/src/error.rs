//! Oracle precompile errors.

use pachi_primitives::AssetId;

/// Errors returned by oracle precompile operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OraclePrecompileError {
    /// Asset ID is not in the supported set.
    #[error("unsupported asset: {asset_id:#04x}")]
    UnsupportedAsset {
        /// The raw asset ID byte.
        asset_id: u8,
    },

    /// Snapshot is missing an entry for a supported asset.
    #[error("snapshot missing asset: {0:?}")]
    MissingAsset(AssetId),

    /// Snapshot contains an unsupported asset.
    #[error("snapshot contains unsupported asset: {asset_id:#04x}")]
    SnapshotUnsupportedAsset {
        /// The raw asset ID byte.
        asset_id: u8,
    },
}
