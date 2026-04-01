//! System transaction generation for VRF fulfillment and oracle updates.

use alloy_primitives::{Bytes, B256, U256};
use alloy_rlp::Encodable;
use pachi_oracle_engine::OracleEngine;
use pachi_primitives::PriceSnapshot;
use pachi_tx::{PachiSystemTx, SystemTxSubtype};

/// Generates system transactions for block building.
#[derive(Debug, Clone)]
pub struct SystemTxGenerator {
    /// Chain ID for system transactions.
    chain_id: u64,
    /// VRF secret key for computing random values (Phase 1: single key held in process).
    vrf_secret_key: Option<[u8; 32]>,
}

impl SystemTxGenerator {
    /// Creates a new generator with the given chain ID.
    pub const fn new(chain_id: u64) -> Self {
        Self { chain_id, vrf_secret_key: None }
    }

    /// Sets the VRF secret key (Phase 1: single sequencer holds the key).
    pub const fn with_vrf_secret_key(mut self, key: [u8; 32]) -> Self {
        self.vrf_secret_key = Some(key);
        self
    }

    /// Generates an `OracleUpdate` system transaction from the engine's current state.
    pub fn oracle_update_tx(&self, engine: &OracleEngine, nonce: u64) -> PachiSystemTx {
        let snapshot = engine.generate_snapshot();
        self.oracle_update_tx_from_snapshot(&snapshot, nonce)
    }

    /// Generates an `OracleUpdate` system transaction from a given snapshot.
    pub fn oracle_update_tx_from_snapshot(
        &self,
        snapshot: &PriceSnapshot,
        nonce: u64,
    ) -> PachiSystemTx {
        let data = serde_json::to_vec(snapshot).expect("PriceSnapshot serialization cannot fail");

        PachiSystemTx {
            chain_id: self.chain_id,
            nonce,
            to: pachi_oracle_precompile::ORACLE_PRECOMPILE_ADDRESS,
            value: U256::ZERO,
            input: Bytes::from(data),
            system_tx_type: SystemTxSubtype::OracleUpdate,
        }
    }

    /// Generates a VRF fulfill system transaction.
    ///
    /// Returns `None` if no VRF secret key is configured.
    pub fn vrf_fulfill_tx(&self, key: B256, seed: B256, nonce: u64) -> Option<PachiSystemTx> {
        let secret_key = self.vrf_secret_key.as_ref()?;

        let (random_value, proof) = pachi_vrf_core::vrf_compute(secret_key, &seed).ok()?;
        let proof_bytes = proof.to_bytes();

        // Encode fulfill call data: key || random_value || proof
        let mut data = Vec::with_capacity(32 + 32 + proof_bytes.len());
        data.extend_from_slice(key.as_slice());
        data.extend_from_slice(random_value.as_slice());
        data.extend_from_slice(&proof_bytes);

        Some(PachiSystemTx {
            chain_id: self.chain_id,
            nonce,
            to: pachi_vrf_precompile::VRF_COMPUTE_ADDRESS,
            value: U256::ZERO,
            input: Bytes::from(data),
            system_tx_type: SystemTxSubtype::VrfFulfill,
        })
    }

    /// Encodes a [`PachiSystemTx`] into its EIP-2718 typed envelope bytes.
    pub fn encode_system_tx(tx: &PachiSystemTx) -> Bytes {
        let mut buf = Vec::new();
        buf.push(pachi_tx::PachiTxType::System as u8);
        tx.encode(&mut buf);
        Bytes::from(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pachi_primitives::{AssetId, Confidence, SnapshotEntry, CURRENT_ENGINE_VERSION};
    use std::collections::BTreeMap;

    #[test]
    fn oracle_update_tx_has_correct_fields() {
        let generator = SystemTxGenerator::new(1337);
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

        let tx = generator.oracle_update_tx_from_snapshot(&snapshot, 42);
        assert_eq!(tx.chain_id, 1337);
        assert_eq!(tx.nonce, 42);
        assert_eq!(tx.system_tx_type, SystemTxSubtype::OracleUpdate);
        assert_eq!(tx.value, U256::ZERO);
        assert!(!tx.input.is_empty());

        // Verify the snapshot can be deserialized back
        let decoded: PriceSnapshot = serde_json::from_slice(&tx.input).unwrap();
        assert_eq!(decoded, snapshot);
    }

    #[test]
    fn encode_system_tx_has_type_prefix() {
        let generator = SystemTxGenerator::new(1);
        let snapshot = PriceSnapshot::all_unavailable(CURRENT_ENGINE_VERSION);
        let tx = generator.oracle_update_tx_from_snapshot(&snapshot, 0);
        let encoded = SystemTxGenerator::encode_system_tx(&tx);
        assert_eq!(encoded[0], pachi_tx::PachiTxType::System as u8);
    }

    #[test]
    fn oracle_update_all_unavailable() {
        let generator = SystemTxGenerator::new(1);
        let snapshot = PriceSnapshot::all_unavailable(CURRENT_ENGINE_VERSION);
        let tx = generator.oracle_update_tx_from_snapshot(&snapshot, 0);

        let decoded: PriceSnapshot = serde_json::from_slice(&tx.input).unwrap();
        assert_eq!(decoded, snapshot);
        assert_eq!(decoded.entries.len(), AssetId::ALL.len());
    }

    #[test]
    fn vrf_fulfill_tx_without_key_returns_none() {
        let generator = SystemTxGenerator::new(1);
        // No VRF key configured → returns None
        let result = generator.vrf_fulfill_tx(B256::ZERO, B256::from([0xAA; 32]), 0);
        assert!(result.is_none());
    }

    #[test]
    fn vrf_fulfill_tx_with_key_generates_tx() {
        // Generate a valid VRF key pair
        let (secret_key, _) = pachi_vrf_core::generate_keypair();
        let generator = SystemTxGenerator::new(1337).with_vrf_secret_key(secret_key);

        let seed = B256::from([0xBB; 32]);
        let key = B256::from([0xCC; 32]);
        let result = generator.vrf_fulfill_tx(key, seed, 7);

        assert!(result.is_some(), "VRF fulfill tx should be generated with valid key");
        let tx = result.unwrap();
        assert_eq!(tx.chain_id, 1337);
        assert_eq!(tx.nonce, 7);
        assert_eq!(tx.system_tx_type, SystemTxSubtype::VrfFulfill);
        assert_eq!(tx.value, U256::ZERO);
        assert_eq!(tx.to, pachi_vrf_precompile::VRF_COMPUTE_ADDRESS);
        // Input = key(32) + random_value(32) + proof(97) = 161 bytes
        assert_eq!(tx.input.len(), 32 + 32 + 97, "input should be key+random+proof");
        // First 32 bytes should be the key
        assert_eq!(&tx.input[..32], key.as_slice());
    }

    #[test]
    fn vrf_fulfill_tx_is_deterministic() {
        let (secret_key, _) = pachi_vrf_core::generate_keypair();
        let generator = SystemTxGenerator::new(1).with_vrf_secret_key(secret_key);
        let seed = B256::from([0x11; 32]);
        let key = B256::from([0x22; 32]);

        let tx1 = generator.vrf_fulfill_tx(key, seed, 0).unwrap();
        let tx2 = generator.vrf_fulfill_tx(key, seed, 0).unwrap();
        // VRF is deterministic: same key + same seed → same output
        assert_eq!(tx1.input, tx2.input);
    }
}
