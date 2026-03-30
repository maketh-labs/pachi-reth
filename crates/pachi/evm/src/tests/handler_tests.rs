//! Tests for pre/post execution handlers.

use alloy_primitives::{Address, U256};
use pachi_oracle_precompile::MockState;
use pachi_primitives::{CallPolicy, Limit, TransferPolicy};
use pachi_session_precompile::{
    SessionNonce, SessionPrecompileError, SessionRegistry, SessionValidationInput,
};
use pachi_sponsor_precompile::{
    SettlementAction, SponsorConfig, SponsorHub, SponsorRecord, SponsorSettlement,
    SponsorValidationInput,
};

use crate::handlers::{
    post_execution::PostExecutionHandler,
    pre_execution::{PreExecutionError, PreExecutionHandler, PreExecutionResult},
};

fn default_sponsor_config() -> SponsorConfig {
    SponsorConfig {
        allowed_senders: vec![],
        call_policies: vec![],
        transfer_policies: vec![],
        global_fee_limit: Limit::unlimited(),
        per_sender_fee_limit: Limit::unlimited(),
        max_gas_per_tx: U256::from(1_000_000u64),
        global_tx_limit: Limit::unlimited(),
        per_sender_tx_limit: Limit::unlimited(),
        valid_until: 100_000,
    }
}

// ==================== PreExecutionHandler::validate_session_tx ====================

#[test]
fn pre_session_valid() {
    let mut state = MockState::new();
    let authorizer = Address::from([0xAAu8; 20]);
    let signer = Address::from([0xBBu8; 20]);
    let hash = alloy_primitives::B256::from([0x11u8; 32]);
    let target = Address::from([0xCCu8; 20]);
    let selector = [0xDE, 0xAD, 0xBE, 0xEF];

    SessionRegistry::create_session(&mut state, authorizer, signer, 10_000, hash, 100, 10).unwrap();

    let calldata = selector.to_vec();
    let call_policy = CallPolicy {
        target,
        selector: alloy_primitives::FixedBytes::from(selector),
        value_limit: Limit::unlimited(),
        max_value_per_use: U256::MAX,
        constraints: vec![],
    };

    let input = SessionValidationInput {
        session_hash: &hash,
        authorizer,
        nonce: 0,
        to: target,
        calldata: &calldata,
        value: U256::ZERO,
        estimated_gas_cost: U256::from(100u64),
        block_timestamp: 200,
        call_policies: &[call_policy],
        transfer_policies: &[],
        fee_limit: &Limit::unlimited(),
    };

    let result = PreExecutionHandler::validate_session_tx(&state, &input).unwrap();
    match result {
        PreExecutionResult::Session { authorizer: auth } => {
            assert_eq!(auth, authorizer);
        }
        _ => panic!("expected Session result"),
    }
}

#[test]
fn pre_session_expired_fails() {
    let mut state = MockState::new();
    let authorizer = Address::from([0xAAu8; 20]);
    let signer = Address::from([0xBBu8; 20]);
    let hash = alloy_primitives::B256::from([0x11u8; 32]);
    let target = Address::from([0xCCu8; 20]);

    SessionRegistry::create_session(&mut state, authorizer, signer, 10_000, hash, 100, 10).unwrap();

    let input = SessionValidationInput {
        session_hash: &hash,
        authorizer,
        nonce: 0,
        to: target,
        calldata: &[],
        value: U256::ZERO,
        estimated_gas_cost: U256::ZERO,
        block_timestamp: 20_000, // past expires_at
        call_policies: &[],
        transfer_policies: &[],
        fee_limit: &Limit::unlimited(),
    };

    let err = PreExecutionHandler::validate_session_tx(&state, &input).unwrap_err();
    assert!(matches!(
        err,
        PreExecutionError::Session(SessionPrecompileError::SessionExpired { .. })
    ));
}

#[test]
fn pre_session_wrong_nonce_fails() {
    let mut state = MockState::new();
    let authorizer = Address::from([0xAAu8; 20]);
    let signer = Address::from([0xBBu8; 20]);
    let hash = alloy_primitives::B256::from([0x11u8; 32]);
    let target = Address::from([0xCCu8; 20]);
    let selector = [0xDE, 0xAD, 0xBE, 0xEF];

    SessionRegistry::create_session(&mut state, authorizer, signer, 10_000, hash, 100, 10).unwrap();

    let call_policy = CallPolicy {
        target,
        selector: alloy_primitives::FixedBytes::from(selector),
        value_limit: Limit::unlimited(),
        max_value_per_use: U256::MAX,
        constraints: vec![],
    };

    let input = SessionValidationInput {
        session_hash: &hash,
        authorizer,
        nonce: 999, // wrong nonce
        to: target,
        calldata: &selector,
        value: U256::ZERO,
        estimated_gas_cost: U256::ZERO,
        block_timestamp: 200,
        call_policies: &[call_policy],
        transfer_policies: &[],
        fee_limit: &Limit::unlimited(),
    };

    let err = PreExecutionHandler::validate_session_tx(&state, &input).unwrap_err();
    assert!(matches!(
        err,
        PreExecutionError::Session(SessionPrecompileError::NonceMismatch { .. })
    ));
}

#[test]
fn pre_session_revoked_fails() {
    let mut state = MockState::new();
    let authorizer = Address::from([0xAAu8; 20]);
    let signer = Address::from([0xBBu8; 20]);
    let hash = alloy_primitives::B256::from([0x11u8; 32]);

    SessionRegistry::create_session(&mut state, authorizer, signer, 10_000, hash, 100, 10).unwrap();
    SessionRegistry::revoke_session(&mut state, authorizer, &hash).unwrap();

    let input = SessionValidationInput {
        session_hash: &hash,
        authorizer,
        nonce: 0,
        to: Address::ZERO,
        calldata: &[],
        value: U256::ZERO,
        estimated_gas_cost: U256::ZERO,
        block_timestamp: 200,
        call_policies: &[],
        transfer_policies: &[],
        fee_limit: &Limit::unlimited(),
    };

    let err = PreExecutionHandler::validate_session_tx(&state, &input).unwrap_err();
    assert!(matches!(
        err,
        PreExecutionError::Session(SessionPrecompileError::SessionRevoked { .. })
    ));
}

// ==================== PreExecutionHandler::validate_sponsored_tx ====================

#[test]
fn pre_sponsored_deposit_locks_balance() {
    let mut state = MockState::new();
    let sponsor = Address::from([0xAAu8; 20]);
    let sender = Address::from([0xBBu8; 20]);

    let config = default_sponsor_config();
    SponsorHub::register_policy(&mut state, sponsor, &config, 100).unwrap();
    SponsorHub::deposit(&mut state, sponsor, U256::from(10_000u64)).unwrap();

    let record = SponsorRecord {
        status: pachi_sponsor_precompile::SponsorStatus::Active,
        sponsor_type: pachi_sponsor_precompile::SponsorType::Deposit,
        balance: U256::from(10_000u64),
        config: config.clone(),
        created_at: 100,
    };

    let validation = SponsorValidationInput {
        sponsor,
        sender,
        to: Address::ZERO,
        calldata: &[],
        gas_limit: U256::from(100_000u64),
        block_timestamp: 200,
    };

    let result = PreExecutionHandler::validate_sponsored_tx(
        &mut state,
        &validation,
        &record,
        U256::from(5_000u64),
    )
    .unwrap();

    match result {
        PreExecutionResult::Sponsored { sponsor: s, locked_amount } => {
            assert_eq!(s, sponsor);
            assert_eq!(locked_amount, U256::from(5_000u64));
        }
        _ => panic!("expected Sponsored result"),
    }

    // Balance reduced by locked amount
    assert_eq!(SponsorHub::get_balance(&state, sponsor), U256::from(5_000u64));
}

// ==================== PreExecutionHandler::validate_session_sponsored_tx ====================

#[test]
fn pre_session_sponsored_validates_both() {
    let mut state = MockState::new();
    let authorizer = Address::from([0xAAu8; 20]);
    let signer = Address::from([0xBBu8; 20]);
    let sponsor_addr = Address::from([0xCCu8; 20]);
    let target = Address::from([0xDDu8; 20]);
    let hash = alloy_primitives::B256::from([0x33u8; 32]);
    let selector = [0xDE, 0xAD, 0xBE, 0xEF];

    // Setup session
    SessionRegistry::create_session(&mut state, authorizer, signer, 10_000, hash, 100, 10).unwrap();

    // Setup sponsor
    let config = default_sponsor_config();
    SponsorHub::register_policy(&mut state, sponsor_addr, &config, 100).unwrap();
    SponsorHub::deposit(&mut state, sponsor_addr, U256::from(20_000u64)).unwrap();

    let call_policy = CallPolicy {
        target,
        selector: alloy_primitives::FixedBytes::from(selector),
        value_limit: Limit::unlimited(),
        max_value_per_use: U256::MAX,
        constraints: vec![],
    };

    let session_input = SessionValidationInput {
        session_hash: &hash,
        authorizer,
        nonce: 0,
        to: target,
        calldata: &selector,
        value: U256::ZERO,
        estimated_gas_cost: U256::from(100u64),
        block_timestamp: 200,
        call_policies: &[call_policy],
        transfer_policies: &[],
        fee_limit: &Limit::unlimited(),
    };

    // Sponsor config must also allow calls to target/selector
    let sponsor_call_policy = pachi_sponsor_precompile::SponsorCallPolicy {
        target,
        selector: alloy_primitives::FixedBytes::from(selector),
        constraints: vec![],
    };
    let mut sponsor_config = default_sponsor_config();
    sponsor_config.call_policies = vec![sponsor_call_policy];

    let sponsor_record = SponsorRecord {
        status: pachi_sponsor_precompile::SponsorStatus::Active,
        sponsor_type: pachi_sponsor_precompile::SponsorType::Deposit,
        balance: U256::from(20_000u64),
        config: sponsor_config,
        created_at: 100,
    };

    let sponsor_input = SponsorValidationInput {
        sponsor: sponsor_addr,
        sender: signer,
        to: target,
        calldata: &selector,
        gas_limit: U256::from(100_000u64),
        block_timestamp: 200,
    };

    let result = PreExecutionHandler::validate_session_sponsored_tx(
        &mut state,
        &session_input,
        &sponsor_input,
        &sponsor_record,
        U256::from(10_000u64),
    )
    .unwrap();

    match result {
        PreExecutionResult::SessionSponsored { authorizer: auth, sponsor, locked_amount } => {
            assert_eq!(auth, authorizer);
            assert_eq!(sponsor, sponsor_addr);
            assert_eq!(locked_amount, U256::from(10_000u64));
        }
        _ => panic!("expected SessionSponsored result"),
    }
}

#[test]
fn pre_session_sponsored_session_failure_stops_early() {
    let mut state = MockState::new();
    let authorizer = Address::from([0xAAu8; 20]);
    let sponsor_addr = Address::from([0xCCu8; 20]);
    let hash = alloy_primitives::B256::from([0x44u8; 32]);

    // No session created → should fail at session step

    let config = default_sponsor_config();
    SponsorHub::register_policy(&mut state, sponsor_addr, &config, 100).unwrap();
    SponsorHub::deposit(&mut state, sponsor_addr, U256::from(20_000u64)).unwrap();

    let session_input = SessionValidationInput {
        session_hash: &hash,
        authorizer,
        nonce: 0,
        to: Address::ZERO,
        calldata: &[],
        value: U256::ZERO,
        estimated_gas_cost: U256::ZERO,
        block_timestamp: 200,
        call_policies: &[],
        transfer_policies: &[],
        fee_limit: &Limit::unlimited(),
    };

    let sponsor_record = SponsorRecord {
        status: pachi_sponsor_precompile::SponsorStatus::Active,
        sponsor_type: pachi_sponsor_precompile::SponsorType::Deposit,
        balance: U256::from(20_000u64),
        config,
        created_at: 100,
    };

    let sponsor_input = SponsorValidationInput {
        sponsor: sponsor_addr,
        sender: Address::from([0xBBu8; 20]),
        to: Address::ZERO,
        calldata: &[],
        gas_limit: U256::from(100_000u64),
        block_timestamp: 200,
    };

    let err = PreExecutionHandler::validate_session_sponsored_tx(
        &mut state,
        &session_input,
        &sponsor_input,
        &sponsor_record,
        U256::from(10_000u64),
    )
    .unwrap_err();

    // Should fail with session error, not sponsor error
    assert!(matches!(err, PreExecutionError::Session(_)));

    // Sponsor balance should NOT be locked (session failed first)
    assert_eq!(SponsorHub::get_balance(&state, sponsor_addr), U256::from(20_000u64));
}

// ==================== PostExecutionHandler ====================

#[test]
fn post_session_increments_nonce() {
    let mut state = MockState::new();
    let authorizer = Address::from([0xAAu8; 20]);
    let signer = Address::from([0xBBu8; 20]);
    let hash = alloy_primitives::B256::from([0x11u8; 32]);

    SessionRegistry::create_session(&mut state, authorizer, signer, 10_000, hash, 100, 10).unwrap();

    assert_eq!(SessionNonce::get(&state, authorizer, &hash), 0);

    PostExecutionHandler::finalize_session_tx(
        &mut state,
        authorizer,
        &hash,
        U256::from(1000u64),
        200,
        &Limit::unlimited(),
        Address::from([0xCCu8; 20]),
        &[0xDE, 0xAD, 0xBE, 0xEF],
        U256::ZERO,
        &[],
        &[],
    );

    assert_eq!(SessionNonce::get(&state, authorizer, &hash), 1);
}

#[test]
fn post_session_nonce_increments_sequentially() {
    let mut state = MockState::new();
    let authorizer = Address::from([0xAAu8; 20]);
    let signer = Address::from([0xBBu8; 20]);
    let hash = alloy_primitives::B256::from([0x55u8; 32]);

    SessionRegistry::create_session(&mut state, authorizer, signer, 10_000, hash, 100, 10).unwrap();

    for expected in 0..5u64 {
        assert_eq!(SessionNonce::get(&state, authorizer, &hash), expected);
        PostExecutionHandler::finalize_session_tx(
            &mut state,
            authorizer,
            &hash,
            U256::from(100u64),
            200,
            &Limit::unlimited(),
            Address::ZERO,
            &[],
            U256::ZERO,
            &[],
            &[TransferPolicy {
                target: Address::ZERO,
                max_value_per_use: U256::MAX,
                value_limit: Limit::unlimited(),
            }],
        );
    }
    assert_eq!(SessionNonce::get(&state, authorizer, &hash), 5);
}

#[test]
fn post_sponsored_deposit_settles() {
    let mut state = MockState::new();
    let sponsor = Address::from([0xAAu8; 20]);
    let sender = Address::from([0xBBu8; 20]);

    let config = default_sponsor_config();
    SponsorHub::register_policy(&mut state, sponsor, &config, 100).unwrap();
    SponsorHub::deposit(&mut state, sponsor, U256::from(10_000u64)).unwrap();

    let locked = SponsorSettlement::pre_lock(&mut state, sponsor, U256::from(5_000u64)).unwrap();

    let record = SponsorRecord {
        status: pachi_sponsor_precompile::SponsorStatus::Active,
        sponsor_type: pachi_sponsor_precompile::SponsorType::Deposit,
        balance: U256::from(5_000u64),
        config,
        created_at: 100,
    };

    let result = PostExecutionHandler::finalize_sponsored_tx(
        &mut state,
        sponsor,
        sender,
        U256::from(3_000u64),
        locked,
        200,
        &record,
    )
    .unwrap();

    assert_eq!(
        result.settlement,
        Some(SettlementAction::DepositDeducted { actual_cost: U256::from(3_000u64) })
    );
    // Balance: 5000 (after lock) + 2000 (refund) = 7000
    assert_eq!(SponsorHub::get_balance(&state, sponsor), U256::from(7_000u64));
}

#[test]
fn post_session_updates_fee_limit() {
    let mut state = MockState::new();
    let authorizer = Address::from([0xAAu8; 20]);
    let signer = Address::from([0xBBu8; 20]);
    let hash = alloy_primitives::B256::from([0x22u8; 32]);

    SessionRegistry::create_session(&mut state, authorizer, signer, 10_000, hash, 100, 10).unwrap();

    let fee_limit = Limit::lifetime(U256::from(5_000u64));

    // First tx: 3000 gas
    PostExecutionHandler::finalize_session_tx(
        &mut state,
        authorizer,
        &hash,
        U256::from(3_000u64),
        200,
        &fee_limit,
        Address::ZERO,
        &[],
        U256::ZERO,
        &[],
        &[],
    );

    assert_eq!(SessionNonce::get(&state, authorizer, &hash), 1);

    // Validate that fee limit is now tracked: 3000 used, so 3000 more should exceed 5000
    let input = SessionValidationInput {
        session_hash: &hash,
        authorizer,
        nonce: 1,
        to: Address::ZERO,
        calldata: &[],
        value: U256::ZERO,
        estimated_gas_cost: U256::from(3_000u64),
        block_timestamp: 300,
        call_policies: &[],
        transfer_policies: &[TransferPolicy {
            target: Address::ZERO,
            max_value_per_use: U256::MAX,
            value_limit: Limit::unlimited(),
        }],
        fee_limit: &fee_limit,
    };

    let err = pachi_session_precompile::SessionValidator::validate(&state, &input).unwrap_err();
    assert!(matches!(err, SessionPrecompileError::FeeLimitExceeded));
}

#[test]
fn post_session_sponsored_does_both() {
    let mut state = MockState::new();
    let authorizer = Address::from([0xAAu8; 20]);
    let signer = Address::from([0xBBu8; 20]);
    let sponsor_addr = Address::from([0xCCu8; 20]);
    let hash = alloy_primitives::B256::from([0x66u8; 32]);

    SessionRegistry::create_session(&mut state, authorizer, signer, 10_000, hash, 100, 10).unwrap();

    let config = default_sponsor_config();
    SponsorHub::register_policy(&mut state, sponsor_addr, &config, 100).unwrap();
    SponsorHub::deposit(&mut state, sponsor_addr, U256::from(10_000u64)).unwrap();
    let locked =
        SponsorSettlement::pre_lock(&mut state, sponsor_addr, U256::from(5_000u64)).unwrap();

    let record = SponsorRecord {
        status: pachi_sponsor_precompile::SponsorStatus::Active,
        sponsor_type: pachi_sponsor_precompile::SponsorType::Deposit,
        balance: U256::from(5_000u64),
        config,
        created_at: 100,
    };

    let result = PostExecutionHandler::finalize_session_sponsored_tx(
        &mut state,
        authorizer,
        &hash,
        sponsor_addr,
        signer,
        U256::from(2_000u64),
        locked,
        200,
        &Limit::unlimited(),
        Address::ZERO,
        &[],
        U256::ZERO,
        &[],
        &[],
        &record,
    )
    .unwrap();

    // Session nonce incremented
    assert_eq!(SessionNonce::get(&state, authorizer, &hash), 1);
    // Sponsor settled
    assert_eq!(
        result.settlement,
        Some(SettlementAction::DepositDeducted { actual_cost: U256::from(2_000u64) })
    );
    // Balance: 5000 + 3000 (refund) = 8000
    assert_eq!(SponsorHub::get_balance(&state, sponsor_addr), U256::from(8_000u64));
}
