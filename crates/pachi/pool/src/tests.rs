//! Tests for Pachi pool validation.

use crate::PachiPoolValidator;
use alloy_primitives::{Address, B256, U256};
use pachi_oracle_precompile::{MockState, PachiState};
use pachi_session_precompile::SessionRegistry;
use pachi_sponsor_precompile::SPONSOR_HUB_ADDRESS;

fn setup_active_session(
    state: &mut MockState,
    authorizer: Address,
    signer: Address,
    expires_at: u64,
    block_timestamp: u64,
) -> B256 {
    let session_hash = alloy_primitives::keccak256(
        [authorizer.as_slice(), signer.as_slice(), &expires_at.to_be_bytes()].concat(),
    );
    let (hash, _) = SessionRegistry::create_session(
        state,
        authorizer,
        signer,
        expires_at,
        session_hash,
        block_timestamp,
        10,
    )
    .unwrap();
    hash
}

fn setup_active_sponsor(state: &mut MockState, sponsor: Address, balance: U256) {
    use pachi_sponsor_precompile::storage_keys;

    // Write sponsor record with Active status, Deposit type
    let base = storage_keys::sponsor_record_key(sponsor);
    state.set_storage(SPONSOR_HUB_ADDRESS, base, U256::ZERO); // status = Active (0)
    state.set_storage(SPONSOR_HUB_ADDRESS, base + U256::from(1), U256::from(0)); // type = Deposit
    state.set_storage(SPONSOR_HUB_ADDRESS, base + U256::from(2), balance); // balance
    state.set_storage(SPONSOR_HUB_ADDRESS, base + U256::from(3), U256::from(1000)); // created_at
    state.set_storage(SPONSOR_HUB_ADDRESS, base + U256::from(4), U256::from(u64::MAX)); // valid_until
}

#[test]
fn session_tx_valid() {
    let mut state = MockState::new();
    let authorizer = Address::left_padding_from(&[1]);
    let signer = Address::left_padding_from(&[2]);

    let session_hash = setup_active_session(&mut state, authorizer, signer, 2000, 1000);

    let result =
        PachiPoolValidator::validate_session_tx(&state, &session_hash, authorizer, 0, 1500);
    assert!(result.is_ok());
}

#[test]
fn session_tx_expired() {
    let mut state = MockState::new();
    let authorizer = Address::left_padding_from(&[1]);
    let signer = Address::left_padding_from(&[2]);

    let session_hash = setup_active_session(&mut state, authorizer, signer, 1500, 1000);

    // block_timestamp > expires_at
    let result =
        PachiPoolValidator::validate_session_tx(&state, &session_hash, authorizer, 0, 2000);
    assert!(result.is_err());
}

#[test]
fn session_tx_wrong_nonce() {
    let mut state = MockState::new();
    let authorizer = Address::left_padding_from(&[1]);
    let signer = Address::left_padding_from(&[2]);

    let session_hash = setup_active_session(&mut state, authorizer, signer, 2000, 1000);

    let result =
        PachiPoolValidator::validate_session_tx(&state, &session_hash, authorizer, 1, 1500);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(
        err,
        crate::PachiTxValidationError::SessionNonceMismatch { expected: 0, got: 1 }
    ));
}

#[test]
fn session_tx_wrong_authorizer() {
    let mut state = MockState::new();
    let authorizer = Address::left_padding_from(&[1]);
    let signer = Address::left_padding_from(&[2]);
    let wrong_authorizer = Address::left_padding_from(&[3]);

    let session_hash = setup_active_session(&mut state, authorizer, signer, 2000, 1000);

    let result =
        PachiPoolValidator::validate_session_tx(&state, &session_hash, wrong_authorizer, 0, 1500);
    assert!(result.is_err());
}

#[test]
fn session_tx_nonexistent() {
    let state = MockState::new();
    let authorizer = Address::left_padding_from(&[1]);
    let fake_hash = B256::from([0xAA; 32]);

    let result = PachiPoolValidator::validate_session_tx(&state, &fake_hash, authorizer, 0, 1500);
    assert!(result.is_err());
}

#[test]
fn sponsored_tx_valid() {
    let mut state = MockState::new();
    let sponsor = Address::left_padding_from(&[10]);
    let balance = U256::from(1_000_000u64);
    setup_active_sponsor(&mut state, sponsor, balance);

    let result =
        PachiPoolValidator::validate_sponsored_tx(&state, sponsor, 1500, U256::from(100_000u64));
    assert!(result.is_ok());
}

#[test]
fn sponsored_tx_insufficient_balance() {
    let mut state = MockState::new();
    let sponsor = Address::left_padding_from(&[10]);
    let balance = U256::from(100u64);
    setup_active_sponsor(&mut state, sponsor, balance);

    let result =
        PachiPoolValidator::validate_sponsored_tx(&state, sponsor, 1500, U256::from(100_000u64));
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        crate::PachiTxValidationError::SponsorInsufficientBalance
    ));
}

#[test]
fn sponsored_tx_inactive() {
    let state = MockState::new();
    let sponsor = Address::left_padding_from(&[10]);

    // No sponsor record → status = 0 (not Active)
    let result =
        PachiPoolValidator::validate_sponsored_tx(&state, sponsor, 1500, U256::from(100u64));
    assert!(result.is_err());
}

#[test]
fn session_sponsored_tx_valid() {
    let mut state = MockState::new();
    let authorizer = Address::left_padding_from(&[1]);
    let signer = Address::left_padding_from(&[2]);
    let sponsor = Address::left_padding_from(&[10]);

    let session_hash = setup_active_session(&mut state, authorizer, signer, 2000, 1000);
    setup_active_sponsor(&mut state, sponsor, U256::from(1_000_000u64));

    let result = PachiPoolValidator::validate_session_sponsored_tx(
        &state,
        &session_hash,
        authorizer,
        0,
        sponsor,
        1500,
        U256::from(100_000u64),
    );
    assert!(result.is_ok());
}

#[test]
fn is_pachi_type_returns_correct() {
    assert!(PachiPoolValidator::is_pachi_type(0x04));
    assert!(PachiPoolValidator::is_pachi_type(0x05));
    assert!(PachiPoolValidator::is_pachi_type(0x06));
    assert!(PachiPoolValidator::is_pachi_type(0x50));
    assert!(!PachiPoolValidator::is_pachi_type(0x00));
    assert!(!PachiPoolValidator::is_pachi_type(0x02));
    assert!(!PachiPoolValidator::is_pachi_type(0x03));
}

#[test]
fn session_tx_revoked() {
    let mut state = MockState::new();
    let authorizer = Address::left_padding_from(&[1]);
    let signer = Address::left_padding_from(&[2]);

    let session_hash = setup_active_session(&mut state, authorizer, signer, 2000, 1000);

    // Revoke the session
    SessionRegistry::revoke_session(&mut state, authorizer, &session_hash).unwrap();

    let result =
        PachiPoolValidator::validate_session_tx(&state, &session_hash, authorizer, 0, 1500);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("revoked"), "expected 'revoked' in error: {err_msg}");
}

#[test]
fn sponsored_tx_mint_mode_no_balance_check() {
    let mut state = MockState::new();
    let sponsor = Address::left_padding_from(&[10]);

    // Set up a Mint-mode sponsor (type=1) with zero balance
    let base = pachi_sponsor_precompile::storage_keys::sponsor_record_key(sponsor);
    state.set_storage(SPONSOR_HUB_ADDRESS, base, U256::ZERO); // status = Active
    state.set_storage(SPONSOR_HUB_ADDRESS, base + U256::from(1), U256::from(1)); // type = Mint
    state.set_storage(SPONSOR_HUB_ADDRESS, base + U256::from(2), U256::ZERO); // balance = 0
    state.set_storage(SPONSOR_HUB_ADDRESS, base + U256::from(3), U256::from(1000)); // created_at
    state.set_storage(SPONSOR_HUB_ADDRESS, base + U256::from(4), U256::from(u64::MAX)); // valid_until

    // Mint mode: balance check should NOT trigger even with high gas cost
    let result = PachiPoolValidator::validate_sponsored_tx(
        &state,
        sponsor,
        1500,
        U256::from(1_000_000_000u64),
    );
    assert!(result.is_ok(), "Mint sponsor should not fail on balance: {:?}", result);
}

#[test]
fn sponsored_tx_deactivated() {
    let mut state = MockState::new();
    let sponsor = Address::left_padding_from(&[10]);

    let base = pachi_sponsor_precompile::storage_keys::sponsor_record_key(sponsor);
    state.set_storage(SPONSOR_HUB_ADDRESS, base, U256::from(1)); // status = Deactivated
    state.set_storage(SPONSOR_HUB_ADDRESS, base + U256::from(1), U256::ZERO); // type = Deposit
    state.set_storage(SPONSOR_HUB_ADDRESS, base + U256::from(2), U256::from(1_000_000u64));
    state.set_storage(SPONSOR_HUB_ADDRESS, base + U256::from(3), U256::from(1000)); // created_at
    state.set_storage(SPONSOR_HUB_ADDRESS, base + U256::from(4), U256::from(u64::MAX));

    let result =
        PachiPoolValidator::validate_sponsored_tx(&state, sponsor, 1500, U256::from(100u64));
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("status=1"), "expected deactivated status in error: {err_msg}");
}

#[test]
fn sponsored_tx_expired() {
    let mut state = MockState::new();
    let sponsor = Address::left_padding_from(&[10]);
    setup_active_sponsor(&mut state, sponsor, U256::from(1_000_000u64));

    // Override valid_until to 1000
    let base = pachi_sponsor_precompile::storage_keys::sponsor_record_key(sponsor);
    state.set_storage(SPONSOR_HUB_ADDRESS, base + U256::from(4), U256::from(1000u64));

    // block_timestamp=2000 > valid_until=1000
    let result =
        PachiPoolValidator::validate_sponsored_tx(&state, sponsor, 2000, U256::from(100u64));
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("expired"), "expected 'expired' in error: {err_msg}");
}
