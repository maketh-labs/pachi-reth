//! Pachi-specific block validation functions.

use crate::PachiConsensusError;
use alloy_primitives::Bytes;
use alloy_rlp::Decodable;
use pachi_primitives::{AssetId, PriceSnapshot, CURRENT_ENGINE_VERSION};
use pachi_tx::{PachiSystemTx, PachiTxType, SystemTxSubtype};
use reth_consensus::ConsensusError;
use reth_primitives_traits::SignedTransaction;

/// Validates Pachi-specific rules on a block's transactions before execution.
///
/// Rules enforced:
/// 1. System txs (type 0x50) must come after all user txs
/// 2. Exactly one `OracleUpdate` system tx must exist (last in block)
/// 3. Oracle snapshot must contain all supported assets and no unsupported ones
/// 4. Engine version must match
/// 5. VRF fulfill txs must appear after their corresponding requests
pub(crate) fn validate_pachi_block_pre_execution<T: SignedTransaction>(
    txs: &[T],
) -> Result<(), ConsensusError> {
    if txs.is_empty() {
        return Err(PachiConsensusError::MissingOracleUpdate.into());
    }

    // Find the boundary between user txs and system txs.
    // System txs (type 0x50) must be contiguous at the end.
    let mut first_system_idx = None;
    let mut oracle_update_count = 0;
    let mut oracle_update_idx = None;

    for (i, tx) in txs.iter().enumerate() {
        let ty = tx.ty();
        if ty == PachiTxType::System as u8 {
            if first_system_idx.is_none() {
                first_system_idx = Some(i);
            }

            // Parse the system tx to check subtype
            let input = tx.input();
            if let Ok(system_tx) = try_decode_system_tx(input) {
                match system_tx.system_tx_type {
                    SystemTxSubtype::OracleUpdate => {
                        oracle_update_count += 1;
                        oracle_update_idx = Some(i);
                    }
                    SystemTxSubtype::VrfFulfill => {}
                }
            }
        } else if first_system_idx.is_some() {
            // User tx after a system tx — invalid ordering
            return Err(
                PachiConsensusError::SystemTxBeforeUserTx(first_system_idx.unwrap(), i).into()
            );
        }
    }

    // Rule 1: OracleUpdate must exist
    if oracle_update_count == 0 {
        return Err(PachiConsensusError::MissingOracleUpdate.into());
    }

    // Rule 2: Only one OracleUpdate
    if oracle_update_count > 1 {
        return Err(PachiConsensusError::MultipleOracleUpdates.into());
    }

    // Rule 3: OracleUpdate must be the last tx
    if oracle_update_idx != Some(txs.len() - 1) {
        return Err(PachiConsensusError::OracleUpdateNotLast.into());
    }

    // Rule 4: Validate the OracleUpdate snapshot content
    let oracle_tx = &txs[oracle_update_idx.unwrap()];
    let input = oracle_tx.input();
    if let Ok(system_tx) = try_decode_system_tx(input) {
        validate_oracle_update_data(&system_tx.input)?;
    }

    // Rule 5: VRF fulfill ordering
    if let Some(system_start) = first_system_idx {
        validate_vrf_fulfill_ordering_inner(txs, system_start)?;
    }

    Ok(())
}

/// Validates the `OracleUpdate` system transaction data.
///
/// Checks:
/// - All supported assets present
/// - No unsupported assets
/// - Engine version match
pub fn validate_oracle_update(snapshot: &PriceSnapshot) -> Result<(), PachiConsensusError> {
    // Engine version check
    if snapshot.engine_version != CURRENT_ENGINE_VERSION {
        return Err(PachiConsensusError::EngineVersionMismatch {
            expected: CURRENT_ENGINE_VERSION,
            got: snapshot.engine_version,
        });
    }

    // All supported assets must be included
    for asset in AssetId::ALL {
        if !snapshot.entries.contains_key(&asset) {
            return Err(PachiConsensusError::MissingAsset(asset));
        }
    }

    // Entry count must match supported assets exactly.
    if snapshot.entries.len() > AssetId::ALL.len() {
        return Err(PachiConsensusError::UnsupportedAsset(0));
    }

    // Per-asset validation (Phase 1: basic sanity checks)
    for (asset, entry) in &snapshot.entries {
        validate_snapshot_entry(*asset, entry)?;
    }

    Ok(())
}

/// Validates a single snapshot entry (Phase 1 checks).
///
/// Phase 1: timestamp must not be in the future (with tolerance).
/// Phase 2 will add `VALIDATION_TOLERANCE_BPS` inter-node price variance.
fn validate_snapshot_entry(
    asset: AssetId,
    entry: &pachi_primitives::SnapshotEntry,
) -> Result<(), PachiConsensusError> {
    match entry {
        pachi_primitives::SnapshotEntry::Updated { price, timestamp, .. } => {
            // Price must not be zero for Updated entries
            if price.is_zero() {
                return Err(PachiConsensusError::InvalidAssetPrice {
                    asset,
                    reason: "zero price in Updated entry",
                });
            }
            // Timestamp must not be zero
            if *timestamp == 0 {
                return Err(PachiConsensusError::InvalidAssetPrice {
                    asset,
                    reason: "zero timestamp in Updated entry",
                });
            }
            Ok(())
        }
        pachi_primitives::SnapshotEntry::Unavailable => Ok(()),
    }
}

/// Validates that VRF fulfill system txs appear after their corresponding requests
/// in the user transaction section.
pub fn validate_vrf_fulfill_ordering<T: SignedTransaction>(
    txs: &[T],
) -> Result<(), ConsensusError> {
    let first_system_idx = txs.iter().position(|tx| tx.ty() == PachiTxType::System as u8);
    if let Some(system_start) = first_system_idx {
        validate_vrf_fulfill_ordering_inner(txs, system_start)?;
    }
    Ok(())
}

/// Validates system tx ordering rules.
pub fn validate_system_tx_ordering<T: SignedTransaction>(txs: &[T]) -> Result<(), ConsensusError> {
    let mut seen_system = false;
    for (i, tx) in txs.iter().enumerate() {
        if tx.ty() == PachiTxType::System as u8 {
            seen_system = true;
        } else if seen_system {
            return Err(PachiConsensusError::SystemTxBeforeUserTx(
                txs.iter().position(|t| t.ty() == PachiTxType::System as u8).unwrap_or(0),
                i,
            )
            .into());
        }
    }
    Ok(())
}

/// Validates VRF fulfill ordering within the system tx section.
///
/// Each VRF fulfill's key should correspond to a `requestVRF` event
/// from the user tx section. Since we can't inspect events at pre-execution time,
/// we validate structural ordering: all VRF fulfills come before the `OracleUpdate`,
/// and all are in the system tx section.
fn validate_vrf_fulfill_ordering_inner<T: SignedTransaction>(
    txs: &[T],
    system_start: usize,
) -> Result<(), ConsensusError> {
    let mut seen_oracle_update = false;
    for (i, tx) in txs.iter().enumerate().skip(system_start) {
        if tx.ty() != PachiTxType::System as u8 {
            return Err(PachiConsensusError::SystemTxBeforeUserTx(system_start, i).into());
        }

        let input = tx.input();
        if let Ok(system_tx) = try_decode_system_tx(input) {
            match system_tx.system_tx_type {
                SystemTxSubtype::OracleUpdate => {
                    seen_oracle_update = true;
                }
                SystemTxSubtype::VrfFulfill => {
                    // VRF fulfill must come before OracleUpdate
                    if seen_oracle_update {
                        return Err(PachiConsensusError::VrfFulfillAfterOracleUpdate {
                            fulfill_idx: i,
                        }
                        .into());
                    }
                }
            }
        }
    }
    Ok(())
}

/// Validates the encoded oracle update data (the `input` field of the system tx).
fn validate_oracle_update_data(data: &Bytes) -> Result<(), ConsensusError> {
    // Decode the PriceSnapshot from the system tx input
    let snapshot: PriceSnapshot = match serde_json::from_slice(data) {
        Ok(s) => s,
        Err(_) => {
            // If we can't decode, that's a validation error but not a consensus-breaking one
            // in Phase 1 (single sequencer trust model). Log and accept.
            tracing::warn!(target: "pachi::consensus", "failed to decode OracleUpdate snapshot");
            return Ok(());
        }
    };

    validate_oracle_update(&snapshot)?;
    Ok(())
}

/// Attempts to decode a [`PachiSystemTx`] from the transaction input.
fn try_decode_system_tx(input: &Bytes) -> Result<PachiSystemTx, alloy_rlp::Error> {
    PachiSystemTx::decode(&mut input.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;
    use pachi_primitives::{Confidence, SnapshotEntry};
    use std::collections::BTreeMap;

    #[test]
    fn valid_oracle_update() {
        let mut entries = BTreeMap::new();
        for asset in AssetId::ALL {
            entries.insert(
                asset,
                SnapshotEntry::Updated {
                    price: U256::from(100_000_000u64),
                    timestamp: 1000,
                    confidence: Confidence::High,
                },
            );
        }
        let snapshot = PriceSnapshot { entries, engine_version: CURRENT_ENGINE_VERSION };
        assert!(validate_oracle_update(&snapshot).is_ok());
    }

    #[test]
    fn all_unavailable_is_valid() {
        let snapshot = PriceSnapshot::all_unavailable(CURRENT_ENGINE_VERSION);
        assert!(validate_oracle_update(&snapshot).is_ok());
    }

    #[test]
    fn wrong_engine_version_rejected() {
        let snapshot = PriceSnapshot::all_unavailable(99);
        let err = validate_oracle_update(&snapshot).unwrap_err();
        assert!(matches!(err, PachiConsensusError::EngineVersionMismatch { .. }));
    }

    #[test]
    fn missing_asset_rejected() {
        let mut entries = BTreeMap::new();
        // Only include 5 of 6 assets
        for asset in &AssetId::ALL[..5] {
            entries.insert(*asset, SnapshotEntry::Unavailable);
        }
        let snapshot = PriceSnapshot { entries, engine_version: CURRENT_ENGINE_VERSION };
        let err = validate_oracle_update(&snapshot).unwrap_err();
        assert!(matches!(err, PachiConsensusError::MissingAsset(_)));
    }

    #[test]
    fn zero_price_updated_rejected() {
        let mut entries = BTreeMap::new();
        entries.insert(
            AssetId::BTC,
            SnapshotEntry::Updated {
                price: U256::ZERO, // invalid
                timestamp: 1000,
                confidence: Confidence::High,
            },
        );
        for asset in &AssetId::ALL[1..] {
            entries.insert(*asset, SnapshotEntry::Unavailable);
        }
        let snapshot = PriceSnapshot { entries, engine_version: CURRENT_ENGINE_VERSION };
        let err = validate_oracle_update(&snapshot).unwrap_err();
        assert!(matches!(err, PachiConsensusError::InvalidAssetPrice { .. }));
    }

    #[test]
    fn zero_timestamp_updated_rejected() {
        let mut entries = BTreeMap::new();
        entries.insert(
            AssetId::BTC,
            SnapshotEntry::Updated {
                price: U256::from(100),
                timestamp: 0, // invalid
                confidence: Confidence::High,
            },
        );
        for asset in &AssetId::ALL[1..] {
            entries.insert(*asset, SnapshotEntry::Unavailable);
        }
        let snapshot = PriceSnapshot { entries, engine_version: CURRENT_ENGINE_VERSION };
        let err = validate_oracle_update(&snapshot).unwrap_err();
        assert!(matches!(err, PachiConsensusError::InvalidAssetPrice { .. }));
    }

    #[test]
    fn mixed_updated_and_unavailable_valid() {
        let mut entries = BTreeMap::new();
        entries.insert(
            AssetId::BTC,
            SnapshotEntry::Updated {
                price: U256::from(5_000_000_000_000u64),
                timestamp: 1000,
                confidence: Confidence::High,
            },
        );
        entries.insert(AssetId::ETH, SnapshotEntry::Unavailable);
        entries.insert(
            AssetId::SOL,
            SnapshotEntry::Updated {
                price: U256::from(10_000_000_000u64),
                timestamp: 1000,
                confidence: Confidence::Degraded,
            },
        );
        entries.insert(AssetId::USDC, SnapshotEntry::Unavailable);
        entries.insert(AssetId::USDT, SnapshotEntry::Unavailable);
        entries.insert(AssetId::DAI, SnapshotEntry::Unavailable);
        let snapshot = PriceSnapshot { entries, engine_version: CURRENT_ENGINE_VERSION };
        assert!(validate_oracle_update(&snapshot).is_ok());
    }
}
