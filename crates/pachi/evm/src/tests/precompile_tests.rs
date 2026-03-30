//! Tests for precompile registration and factory.
//!
//! We can't easily invoke precompile dispatch functions directly because
//! `PrecompileInput` requires live `EvmInternals`. Instead, we test the
//! factory creation, precompile registration, and ConfigureEvm construction.

use alloy_primitives::Address;

use pachi_oracle_precompile::ORACLE_PRECOMPILE_ADDRESS;
use pachi_session_precompile::SESSION_REGISTRY_ADDRESS;
use pachi_sponsor_precompile::SPONSOR_HUB_ADDRESS;
use pachi_vrf_precompile::{VRF_COMPUTE_ADDRESS, VRF_VERIFY_ADDRESS};

// ==================== Precompile Registration ====================

#[test]
fn pachi_precompiles_contains_all_five() {
    let map = crate::pachi_precompiles();

    // Verify all 5 Pachi precompile addresses are registered
    assert!(map.get(&VRF_COMPUTE_ADDRESS).is_some(), "VRF_COMPUTE (0x0101) not registered");
    assert!(map.get(&VRF_VERIFY_ADDRESS).is_some(), "VRF_VERIFY (0x0102) not registered");
    assert!(
        map.get(&SESSION_REGISTRY_ADDRESS).is_some(),
        "SessionRegistry (0x0800) not registered"
    );
    assert!(map.get(&SPONSOR_HUB_ADDRESS).is_some(), "SponsorHub (0x0801) not registered");
    assert!(map.get(&ORACLE_PRECOMPILE_ADDRESS).is_some(), "PriceOracle (0x0802) not registered");
}

#[test]
fn pachi_precompiles_contains_ethereum_builtins() {
    let map = crate::pachi_precompiles();

    // Standard Ethereum precompiles should also be registered
    let ecrecover = Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    let sha256 = Address::from([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]);

    assert!(map.get(&ecrecover).is_some(), "ecrecover (0x01) not registered");
    assert!(map.get(&sha256).is_some(), "sha256 (0x02) not registered");
}

#[test]
fn pachi_precompiles_addresses_are_correct() {
    // Verify the address constants match the spec
    assert_eq!(
        VRF_COMPUTE_ADDRESS,
        "0x0000000000000000000000000000000000000101".parse::<Address>().unwrap()
    );
    assert_eq!(
        VRF_VERIFY_ADDRESS,
        "0x0000000000000000000000000000000000000102".parse::<Address>().unwrap()
    );
    assert_eq!(
        SESSION_REGISTRY_ADDRESS,
        "0x0000000000000000000000000000000000000800".parse::<Address>().unwrap()
    );
    assert_eq!(
        SPONSOR_HUB_ADDRESS,
        "0x0000000000000000000000000000000000000801".parse::<Address>().unwrap()
    );
    assert_eq!(
        ORACLE_PRECOMPILE_ADDRESS,
        "0x0000000000000000000000000000000000000802".parse::<Address>().unwrap()
    );
}

// ==================== PachiEvmConfig ====================

#[test]
fn pachi_evm_config_can_be_created() {
    use reth_chainspec::ChainSpec;
    use std::sync::Arc;

    let chain_spec: Arc<ChainSpec> = Arc::new(ChainSpec::default());
    let config = crate::PachiEvmConfig::new(chain_spec.clone());

    // Verify the chain spec is accessible
    assert_eq!(Arc::as_ptr(config.chain_spec()), Arc::as_ptr(&chain_spec),);
}

#[test]
fn pachi_evm_factory_is_default() {
    let factory = crate::PachiEvmFactory::default();
    // Just verify it can be created and is Debug
    let _ = format!("{factory:?}");
}

#[test]
fn selector_owner_matches_solidity_standard() {
    use alloy_primitives::keccak256;
    // owner() is a well-known selector: 0x8da5cb5b
    let h = keccak256(b"owner()");
    assert_eq!(&h[..4], &[0x8d, 0xa5, 0xcb, 0x5b]);

    // deposit() = 0xd0e30db0
    let h = keccak256(b"deposit()");
    assert_eq!(&h[..4], &[0xd0, 0xe3, 0x0d, 0xb0]);

    // transferOwnership(address) = 0xf2fde38b
    let h = keccak256(b"transferOwnership(address)");
    assert_eq!(&h[..4], &[0xf2, 0xfd, 0xe3, 0x8b]);
}
