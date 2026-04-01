//! Pachi consensus error types.

use pachi_primitives::AssetId;

/// Pachi-specific consensus validation errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum PachiConsensusError {
    /// Block is missing the mandatory `OracleUpdate` system transaction.
    #[error("missing OracleUpdate system transaction")]
    MissingOracleUpdate,

    /// `OracleUpdate` is not the last system transaction in the block.
    #[error("OracleUpdate must be the last transaction in the block")]
    OracleUpdateNotLast,

    /// A supported asset is missing from the `OracleUpdate` snapshot.
    #[error("missing asset {0:?} in OracleUpdate")]
    MissingAsset(AssetId),

    /// An unsupported asset is present in the `OracleUpdate` snapshot.
    #[error("unsupported asset ID 0x{0:02x} in OracleUpdate")]
    UnsupportedAsset(u8),

    /// Engine version in the `OracleUpdate` does not match the node's version.
    #[error("engine version mismatch: expected {expected}, got {got}")]
    EngineVersionMismatch {
        /// Expected engine version.
        expected: u16,
        /// Actual engine version in the block.
        got: u16,
    },

    /// VRF fulfill system tx appears after the `OracleUpdate` (must come before).
    #[error("VRF fulfill at index {fulfill_idx} appears after OracleUpdate")]
    VrfFulfillAfterOracleUpdate {
        /// Index of the fulfill tx in the block.
        fulfill_idx: usize,
    },

    /// `OracleUpdate` has an invalid price entry for an asset.
    #[error("invalid price for asset {asset:?}: {reason}")]
    InvalidAssetPrice {
        /// The asset with the invalid price.
        asset: AssetId,
        /// Description of what's wrong.
        reason: &'static str,
    },

    /// System transaction appears before all user transactions.
    #[error("system transaction at index {0} appears before user transactions end at {1}")]
    SystemTxBeforeUserTx(usize, usize),

    /// Multiple `OracleUpdate` system transactions in one block.
    #[error("multiple OracleUpdate system transactions")]
    MultipleOracleUpdates,
}

impl From<PachiConsensusError> for reth_consensus::ConsensusError {
    fn from(err: PachiConsensusError) -> Self {
        Self::Other(err.to_string())
    }
}
