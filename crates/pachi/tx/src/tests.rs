use crate::{
    PachiSystemTx, PachiTxEnvelope, PachiTxType, SessionSponsoredTx, SessionTx, SponsoredTx,
    SystemTxSubtype,
};
use alloy_consensus::{
    transaction::{SignerRecoverable, TxHashRef},
    SignableTransaction, Signed, Transaction,
};
use alloy_eips::{eip2930::AccessList, Decodable2718, Encodable2718};
use alloy_primitives::{address, Address, Bytes, Signature, B256, U256};
use alloy_rlp::{Decodable, Encodable};

// Helper: generate a random signing key and its address.
fn test_signer() -> (k256::ecdsa::SigningKey, Address) {
    use k256::ecdsa::SigningKey;
    use rand_08::rngs::OsRng;
    let sk = SigningKey::random(&mut OsRng);
    let pk = sk.verifying_key();
    let addr = Address::from_raw_public_key(&pk.to_encoded_point(false).as_bytes()[1..]);
    (sk, addr)
}

// Helper: sign a transaction.
fn sign_tx<T: SignableTransaction<Signature>>(tx: T, sk: &k256::ecdsa::SigningKey) -> Signed<T> {
    use k256::ecdsa::signature::hazmat::PrehashSigner;
    let sighash = tx.signature_hash();
    let (sig, recovery) = sk.sign_prehash(sighash.as_ref()).unwrap();
    let signature: Signature = (sig, recovery).into();
    tx.into_signed(signature)
}

fn sample_session_tx() -> SessionTx {
    SessionTx {
        chain_id: 1337,
        nonce: 42,
        max_priority_fee_per_gas: 1_000_000_000,
        max_fee_per_gas: 20_000_000_000,
        gas_limit: 100_000,
        to: address!("6069a6c32cf691f5982febae4faf8a6f3ab2f0f6"),
        value: U256::from(1000u64),
        input: Bytes::from(vec![0xab, 0xcd]),
        access_list: AccessList::default(),
        session_hash: B256::repeat_byte(0x11),
        authorizer: address!("dd6b8b3dc6b7ad97db52f08a275ff4483e024cea"),
        session_config: Bytes::from(vec![0x01, 0x02, 0x03]),
    }
}

fn sample_sponsored_tx() -> SponsoredTx {
    SponsoredTx {
        chain_id: 1337,
        nonce: 7,
        max_priority_fee_per_gas: 1_000_000_000,
        max_fee_per_gas: 20_000_000_000,
        gas_limit: 200_000,
        to: address!("6069a6c32cf691f5982febae4faf8a6f3ab2f0f6"),
        value: U256::ZERO,
        input: Bytes::from(vec![0xde, 0xad]),
        access_list: AccessList::default(),
        sponsor: address!("000000000000000000000000000000000000beef"),
    }
}

fn sample_session_sponsored_tx() -> SessionSponsoredTx {
    SessionSponsoredTx {
        chain_id: 1337,
        nonce: 1,
        max_priority_fee_per_gas: 500_000_000,
        max_fee_per_gas: 10_000_000_000,
        gas_limit: 150_000,
        to: address!("6069a6c32cf691f5982febae4faf8a6f3ab2f0f6"),
        value: U256::ZERO,
        input: Bytes::from(vec![0xca, 0xfe]),
        access_list: AccessList::default(),
        session_hash: B256::repeat_byte(0x22),
        authorizer: address!("dd6b8b3dc6b7ad97db52f08a275ff4483e024cea"),
        session_config: Bytes::from(vec![0x04, 0x05]),
        sponsor: address!("000000000000000000000000000000000000cafe"),
    }
}

fn sample_system_tx() -> PachiSystemTx {
    PachiSystemTx {
        chain_id: 1337,
        nonce: 0,
        to: address!("0000000000000000000000000000000000000802"),
        value: U256::ZERO,
        input: Bytes::from(vec![0xff; 64]),
        system_tx_type: SystemTxSubtype::OracleUpdate,
    }
}

// ===== RLP round-trip tests for unsigned types =====

#[test]
fn session_tx_rlp_roundtrip() {
    let tx = sample_session_tx();
    let mut buf = vec![];
    tx.encode(&mut buf);
    let decoded = SessionTx::decode(&mut &buf[..]).unwrap();
    assert_eq!(tx, decoded);
}

#[test]
fn sponsored_tx_rlp_roundtrip() {
    let tx = sample_sponsored_tx();
    let mut buf = vec![];
    tx.encode(&mut buf);
    let decoded = SponsoredTx::decode(&mut &buf[..]).unwrap();
    assert_eq!(tx, decoded);
}

#[test]
fn session_sponsored_tx_rlp_roundtrip() {
    let tx = sample_session_sponsored_tx();
    let mut buf = vec![];
    tx.encode(&mut buf);
    let decoded = SessionSponsoredTx::decode(&mut &buf[..]).unwrap();
    assert_eq!(tx, decoded);
}

#[test]
fn system_tx_rlp_roundtrip() {
    let tx = sample_system_tx();
    let mut buf = vec![];
    tx.encode(&mut buf);
    let decoded = PachiSystemTx::decode(&mut &buf[..]).unwrap();
    assert_eq!(tx, decoded);
}

// ===== Transaction trait tests =====

#[test]
fn session_tx_transaction_trait() {
    let tx = sample_session_tx();
    assert_eq!(tx.chain_id(), Some(1337));
    assert_eq!(tx.nonce(), 42);
    assert_eq!(tx.gas_limit(), 100_000);
    assert_eq!(tx.gas_price(), None);
    assert_eq!(tx.max_fee_per_gas(), 20_000_000_000);
    assert_eq!(tx.max_priority_fee_per_gas(), Some(1_000_000_000));
    assert!(tx.is_dynamic_fee());
    assert!(!tx.is_create());
    assert_eq!(tx.value(), U256::from(1000u64));
}

#[test]
fn system_tx_zero_gas() {
    let tx = sample_system_tx();
    assert_eq!(tx.gas_limit(), 0);
    assert_eq!(tx.max_fee_per_gas(), 0);
    assert_eq!(tx.effective_gas_price(Some(100)), 0);
    assert!(!tx.is_dynamic_fee());
}

// ===== Signed transaction + signer recovery =====

#[test]
fn session_tx_sign_and_recover() {
    let (sk, addr) = test_signer();
    let tx = sample_session_tx();
    let signed = sign_tx(tx, &sk);
    let recovered = signed.recover_signer().unwrap();
    // SessionTx: recovers the session key address (the actual signer)
    assert_eq!(recovered, addr);
}

#[test]
fn sponsored_tx_sign_and_recover() {
    let (sk, addr) = test_signer();
    let tx = sample_sponsored_tx();
    let signed = sign_tx(tx, &sk);
    let recovered = signed.recover_signer().unwrap();
    assert_eq!(recovered, addr);
}

#[test]
fn session_sponsored_tx_sign_and_recover() {
    let (sk, addr) = test_signer();
    let tx = sample_session_sponsored_tx();
    let signed = sign_tx(tx, &sk);
    let recovered = signed.recover_signer().unwrap();
    assert_eq!(recovered, addr);
}

// ===== EIP-2718 envelope round-trip =====

#[test]
fn envelope_session_roundtrip() {
    let (sk, _) = test_signer();
    let signed = sign_tx(sample_session_tx(), &sk);
    let envelope = PachiTxEnvelope::Session(signed);

    let mut buf = vec![];
    envelope.encode_2718(&mut buf);

    let decoded = PachiTxEnvelope::decode_2718(&mut &buf[..]).unwrap();
    assert_eq!(envelope.tx_type(), decoded.tx_type());
    assert_eq!(envelope.nonce(), decoded.nonce());
    assert_eq!(envelope.tx_hash(), decoded.tx_hash());
}

#[test]
fn envelope_sponsored_roundtrip() {
    let (sk, _) = test_signer();
    let signed = sign_tx(sample_sponsored_tx(), &sk);
    let envelope = PachiTxEnvelope::Sponsored(signed);

    let mut buf = vec![];
    envelope.encode_2718(&mut buf);

    let decoded = PachiTxEnvelope::decode_2718(&mut &buf[..]).unwrap();
    assert_eq!(envelope.tx_type(), decoded.tx_type());
    assert_eq!(envelope.tx_hash(), decoded.tx_hash());
}

#[test]
fn envelope_session_sponsored_roundtrip() {
    let (sk, _) = test_signer();
    let signed = sign_tx(sample_session_sponsored_tx(), &sk);
    let envelope = PachiTxEnvelope::SessionSponsored(signed);

    let mut buf = vec![];
    envelope.encode_2718(&mut buf);

    let decoded = PachiTxEnvelope::decode_2718(&mut &buf[..]).unwrap();
    assert_eq!(envelope.tx_type(), decoded.tx_type());
    assert_eq!(envelope.tx_hash(), decoded.tx_hash());
}

#[test]
fn envelope_system_roundtrip() {
    let envelope = PachiTxEnvelope::seal_system(sample_system_tx());

    let mut buf = vec![];
    envelope.encode_2718(&mut buf);

    let decoded = PachiTxEnvelope::decode_2718(&mut &buf[..]).unwrap();
    assert_eq!(envelope.tx_type(), decoded.tx_type());
    assert_eq!(envelope.tx_hash(), decoded.tx_hash());
    assert!(decoded.is_system());
}

// ===== Signer recovery on envelope =====

#[test]
fn envelope_signer_recovery() {
    let (sk, addr) = test_signer();

    let session_env = PachiTxEnvelope::Session(sign_tx(sample_session_tx(), &sk));
    assert_eq!(session_env.recover_signer().unwrap(), addr);

    let sponsored_env = PachiTxEnvelope::Sponsored(sign_tx(sample_sponsored_tx(), &sk));
    assert_eq!(sponsored_env.recover_signer().unwrap(), addr);

    let system_env = PachiTxEnvelope::seal_system(sample_system_tx());
    assert_eq!(system_env.recover_signer().unwrap(), pachi_primitives::SYSTEM_ADDRESS,);
}

// ===== Invalid envelope rejection =====

#[test]
fn invalid_type_byte_rejected() {
    let result = PachiTxEnvelope::decode_2718(&mut &[0x03u8, 0xc0][..]);
    assert!(result.is_err());
}

#[test]
fn truncated_input_rejected() {
    let (sk, _) = test_signer();
    let signed = sign_tx(sample_session_tx(), &sk);
    let envelope = PachiTxEnvelope::Session(signed);

    let mut buf = vec![];
    envelope.encode_2718(&mut buf);

    // Truncate the buffer
    let truncated = &buf[..buf.len() / 2];
    let result = PachiTxEnvelope::decode_2718(&mut &truncated[..]);
    assert!(result.is_err());
}

#[test]
fn empty_input_rejected() {
    let result = PachiTxEnvelope::decode_2718(&mut &[][..]);
    assert!(result.is_err());
}

// ===== Tx type discriminator =====

#[test]
fn tx_type_discriminator() {
    assert_eq!(PachiTxType::Session as u8, 0x04);
    assert_eq!(PachiTxType::Sponsored as u8, 0x05);
    assert_eq!(PachiTxType::SessionSponsored as u8, 0x06);
    assert_eq!(PachiTxType::System as u8, 0x50);
}

// ===== Hash determinism =====

#[test]
fn signed_tx_hash_deterministic() {
    let (sk, _) = test_signer();
    let tx = sample_session_tx();
    let signed = sign_tx(tx.clone(), &sk);
    let h1 = *signed.tx_hash();

    // Re-sign same tx with same key produces same sig hash, but different signature
    // (randomized k). So we check that the hash is computed from the 2718 encoding.
    let env = PachiTxEnvelope::Session(signed.clone());
    let mut buf = vec![];
    env.encode_2718(&mut buf);
    let expected = alloy_primitives::keccak256(&buf);
    assert_eq!(h1, expected);
}

// ===== Fuzz-like RLP decode tests (malformed input) =====

#[test]
fn fuzz_session_tx_decode_garbage() {
    // Random bytes should fail gracefully
    let garbage = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0xFF, 0x80, 0xC0];
    assert!(SessionTx::decode(&mut &garbage[..]).is_err());
}

#[test]
fn fuzz_sponsored_tx_decode_garbage() {
    let garbage = [0x01, 0x02, 0x03, 0x04, 0x05];
    assert!(SponsoredTx::decode(&mut &garbage[..]).is_err());
}

#[test]
fn fuzz_system_tx_decode_garbage() {
    let garbage = [0xFF; 32];
    assert!(PachiSystemTx::decode(&mut &garbage[..]).is_err());
}

#[test]
fn fuzz_envelope_decode_various_garbage() {
    let cases: &[&[u8]] = &[
        &[],
        &[0x04],
        &[0x04, 0xC0],
        &[0x05, 0x80],
        &[0x06, 0xFF, 0xFF],
        &[0x50, 0x00],
        &[0x50, 0xC1, 0x00],
        &[0xFF, 0xFF, 0xFF],
    ];
    for case in cases {
        // Should not panic — errors are fine
        let _ = PachiTxEnvelope::decode_2718(&mut &case[..]);
    }
}
