//! SponsorHub precompile dispatch (0x0801).
//!
//! Routes ABI-encoded calls to the sponsor state machine. Governance functions
//! are owner-only and verified within the Layer 1 code.
//!
//! `registerPolicy` accepts RLP-encoded `SponsorConfig` bytes (not ABI-encoded),
//! matching the on-chain storage format from the spec.
//!
//! # Selectors (keccak256 of canonical signature)
//!
//! - `registerPolicy(bytes)` → `0xa2ea1376` (RLP-encoded SponsorConfig)
//! - `deactivatePolicy()` → `0x8634f7a8`
//! - `deposit()` → `0xd0e30db0`
//! - `withdraw(uint256)` → `0x2e1a7d4d`
//! - `getBalance(address)` → `0xf8b2cb4f`
//! - `isActive(address)` → `0x9f8a13d7`
//! - `getValidUntil(address)` → `0x7beec939`
//! - `getSponsorType(address)` → `0xe498e7fd`
//! - `getPolicy(address)` → `0x3791dc6a`
//! - `canSponsor(address,address,address,bytes4)` → `0xfc5c31a0`
//! - `owner()` → `0x8da5cb5b`
//! - `approveMint(address)` → `0x0e801ee1`
//! - `revokeMint(address)` → `0x3935bac6`
//! - `transferOwnership(address)` → `0xf2fde38b`

use alloy_evm::precompiles::PrecompileInput;
use alloy_primitives::{Address, Bytes, FixedBytes, U256};
use alloy_rlp::Decodable;
use revm::precompile::{PrecompileOutput, PrecompileResult};

use crate::{events, state_bridge::EvmStateBridge};
use pachi_sponsor_precompile::{
    SponsorConfig, SponsorGovernance, SponsorHub, SponsorValidationInput, SponsorValidator,
    GAS_SPONSOR_CAN_SPONSOR, GAS_SPONSOR_DEACTIVATE, GAS_SPONSOR_DEPOSIT, GAS_SPONSOR_MINT_TOGGLE,
    GAS_SPONSOR_REGISTER, GAS_SPONSOR_TRANSFER_OWNERSHIP, GAS_SPONSOR_VIEW, GAS_SPONSOR_WITHDRAW,
};

// Function selectors: keccak256(canonical_signature)[:4]
const SEL_REGISTER_POLICY: FixedBytes<4> = FixedBytes::new([0xa2, 0xea, 0x13, 0x76]);
const SEL_DEACTIVATE_POLICY: FixedBytes<4> = FixedBytes::new([0x86, 0x34, 0xf7, 0xa8]);
const SEL_DEPOSIT: FixedBytes<4> = FixedBytes::new([0xd0, 0xe3, 0x0d, 0xb0]);
const SEL_WITHDRAW: FixedBytes<4> = FixedBytes::new([0x2e, 0x1a, 0x7d, 0x4d]);
const SEL_GET_BALANCE: FixedBytes<4> = FixedBytes::new([0xf8, 0xb2, 0xcb, 0x4f]);
const SEL_IS_ACTIVE: FixedBytes<4> = FixedBytes::new([0x9f, 0x8a, 0x13, 0xd7]);
const SEL_GET_VALID_UNTIL: FixedBytes<4> = FixedBytes::new([0x7b, 0xee, 0xc9, 0x39]);
const SEL_GET_SPONSOR_TYPE: FixedBytes<4> = FixedBytes::new([0xe4, 0x98, 0xe7, 0xfd]);
const SEL_GET_POLICY: FixedBytes<4> = FixedBytes::new([0x37, 0x91, 0xdc, 0x6a]);
const SEL_CAN_SPONSOR: FixedBytes<4> = FixedBytes::new([0xfc, 0x5c, 0x31, 0xa0]);
const SEL_OWNER: FixedBytes<4> = FixedBytes::new([0x8d, 0xa5, 0xcb, 0x5b]);
const SEL_APPROVE_MINT: FixedBytes<4> = FixedBytes::new([0x0e, 0x80, 0x1e, 0xe1]);
const SEL_REVOKE_MINT: FixedBytes<4> = FixedBytes::new([0x39, 0x35, 0xba, 0xc6]);
const SEL_TRANSFER_OWNERSHIP: FixedBytes<4> = FixedBytes::new([0xf2, 0xfd, 0xe3, 0x8b]);

/// SponsorHub precompile entry point.
pub(crate) fn sponsor_hub_precompile(input: PrecompileInput<'_>) -> PrecompileResult {
    let data = input.data;
    if data.len() < 4 {
        return Ok(PrecompileOutput::new_reverted(0, Bytes::copy_from_slice(b"input too short")));
    }

    let selector = FixedBytes::<4>::from_slice(&data[..4]);
    let args = &data[4..];
    let gas = input.gas;
    let caller = input.caller;
    let value = input.value;
    let is_static = input.is_static;
    let mut bridge = EvmStateBridge::new(input.internals);
    let block_timestamp = bridge.block_timestamp();

    match selector {
        // ── State-modifying functions ──
        s if s == SEL_REGISTER_POLICY => {
            if is_static {
                return revert(GAS_SPONSOR_REGISTER, b"state-modifying call in static context");
            }
            if gas < GAS_SPONSOR_REGISTER {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            // args = RLP-encoded SponsorConfig bytes
            if args.is_empty() {
                return revert(GAS_SPONSOR_REGISTER, b"empty config");
            }
            let config = match SponsorConfig::decode(&mut &args[..]) {
                Ok(c) => c,
                Err(_) => return revert(GAS_SPONSOR_REGISTER, b"invalid RLP config"),
            };
            let valid_until = config.valid_until;
            match SponsorHub::register_policy(&mut bridge, caller, &config, block_timestamp) {
                Ok(()) => {
                    events::emit_policy_registered(
                        &bridge,
                        pachi_sponsor_precompile::SPONSOR_HUB_ADDRESS,
                        caller,
                        0, // Deposit type by default
                        valid_until,
                    );
                    Ok(PrecompileOutput::new(GAS_SPONSOR_REGISTER, Bytes::new()))
                }
                Err(e) => revert(GAS_SPONSOR_REGISTER, e.to_string().as_bytes()),
            }
        }
        s if s == SEL_DEACTIVATE_POLICY => {
            if is_static {
                return revert(GAS_SPONSOR_DEACTIVATE, b"state-modifying call in static context");
            }
            if gas < GAS_SPONSOR_DEACTIVATE {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            match SponsorHub::deactivate_policy(&mut bridge, caller) {
                Ok(()) => {
                    events::emit_policy_deactivated(
                        &bridge,
                        pachi_sponsor_precompile::SPONSOR_HUB_ADDRESS,
                        caller,
                    );
                    Ok(PrecompileOutput::new(GAS_SPONSOR_DEACTIVATE, Bytes::new()))
                }
                Err(e) => revert(GAS_SPONSOR_DEACTIVATE, e.to_string().as_bytes()),
            }
        }
        s if s == SEL_DEPOSIT => {
            if is_static {
                return revert(GAS_SPONSOR_DEPOSIT, b"state-modifying call in static context");
            }
            if gas < GAS_SPONSOR_DEPOSIT {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            match SponsorHub::deposit(&mut bridge, caller, value) {
                Ok(()) => {
                    let new_balance = SponsorHub::get_balance(&bridge, caller);
                    events::emit_deposited(
                        &bridge,
                        pachi_sponsor_precompile::SPONSOR_HUB_ADDRESS,
                        caller,
                        value,
                        new_balance,
                    );
                    Ok(PrecompileOutput::new(GAS_SPONSOR_DEPOSIT, Bytes::new()))
                }
                Err(e) => revert(GAS_SPONSOR_DEPOSIT, e.to_string().as_bytes()),
            }
        }
        s if s == SEL_WITHDRAW => {
            if is_static {
                return revert(GAS_SPONSOR_WITHDRAW, b"state-modifying call in static context");
            }
            if gas < GAS_SPONSOR_WITHDRAW {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            if args.len() < 32 {
                return revert(GAS_SPONSOR_WITHDRAW, b"input too short for withdraw");
            }
            let amount = U256::from_be_slice(&args[..32]);
            match SponsorHub::withdraw(&mut bridge, caller, amount) {
                Ok(()) => {
                    let new_balance = SponsorHub::get_balance(&bridge, caller);
                    events::emit_withdrawn(
                        &bridge,
                        pachi_sponsor_precompile::SPONSOR_HUB_ADDRESS,
                        caller,
                        amount,
                        new_balance,
                    );
                    Ok(PrecompileOutput::new(GAS_SPONSOR_WITHDRAW, Bytes::new()))
                }
                Err(e) => revert(GAS_SPONSOR_WITHDRAW, e.to_string().as_bytes()),
            }
        }
        s if s == SEL_APPROVE_MINT => {
            if is_static {
                return revert(GAS_SPONSOR_MINT_TOGGLE, b"state-modifying call in static context");
            }
            if gas < GAS_SPONSOR_MINT_TOGGLE {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            if let Some(r) = require_addr(args, GAS_SPONSOR_MINT_TOGGLE) {
                return r;
            }
            let sponsor = Address::from_slice(&args[12..32]);
            match SponsorGovernance::approve_mint(&mut bridge, caller, sponsor) {
                Ok(()) => {
                    events::emit_mint_approved(
                        &bridge,
                        pachi_sponsor_precompile::SPONSOR_HUB_ADDRESS,
                        sponsor,
                        caller,
                    );
                    Ok(PrecompileOutput::new(GAS_SPONSOR_MINT_TOGGLE, Bytes::new()))
                }
                Err(e) => revert(GAS_SPONSOR_MINT_TOGGLE, e.to_string().as_bytes()),
            }
        }
        s if s == SEL_REVOKE_MINT => {
            if is_static {
                return revert(GAS_SPONSOR_MINT_TOGGLE, b"state-modifying call in static context");
            }
            if gas < GAS_SPONSOR_MINT_TOGGLE {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            if let Some(r) = require_addr(args, GAS_SPONSOR_MINT_TOGGLE) {
                return r;
            }
            let sponsor = Address::from_slice(&args[12..32]);
            match SponsorGovernance::revoke_mint(&mut bridge, caller, sponsor) {
                Ok(()) => {
                    events::emit_mint_revoked(
                        &bridge,
                        pachi_sponsor_precompile::SPONSOR_HUB_ADDRESS,
                        sponsor,
                        caller,
                    );
                    Ok(PrecompileOutput::new(GAS_SPONSOR_MINT_TOGGLE, Bytes::new()))
                }
                Err(e) => revert(GAS_SPONSOR_MINT_TOGGLE, e.to_string().as_bytes()),
            }
        }
        s if s == SEL_TRANSFER_OWNERSHIP => {
            if is_static {
                return revert(
                    GAS_SPONSOR_TRANSFER_OWNERSHIP,
                    b"state-modifying call in static context",
                );
            }
            if gas < GAS_SPONSOR_TRANSFER_OWNERSHIP {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            if let Some(r) = require_addr(args, GAS_SPONSOR_TRANSFER_OWNERSHIP) {
                return r;
            }
            let new_owner = Address::from_slice(&args[12..32]);
            match SponsorGovernance::transfer_ownership(&mut bridge, caller, new_owner) {
                Ok(()) => {
                    events::emit_ownership_transferred(
                        &bridge,
                        pachi_sponsor_precompile::SPONSOR_HUB_ADDRESS,
                        caller,
                        new_owner,
                    );
                    Ok(PrecompileOutput::new(GAS_SPONSOR_TRANSFER_OWNERSHIP, Bytes::new()))
                }
                Err(e) => revert(GAS_SPONSOR_TRANSFER_OWNERSHIP, e.to_string().as_bytes()),
            }
        }

        // ── View functions ──
        s if s == SEL_GET_BALANCE => {
            if gas < GAS_SPONSOR_VIEW {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            if let Some(r) = require_addr(args, GAS_SPONSOR_VIEW) {
                return r;
            }
            let sponsor = Address::from_slice(&args[12..32]);
            let balance = SponsorHub::get_balance(&bridge, sponsor);
            Ok(PrecompileOutput::new(
                GAS_SPONSOR_VIEW,
                Bytes::copy_from_slice(&balance.to_be_bytes::<32>()),
            ))
        }
        s if s == SEL_IS_ACTIVE => {
            if gas < GAS_SPONSOR_VIEW {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            if let Some(r) = require_addr(args, GAS_SPONSOR_VIEW) {
                return r;
            }
            let sponsor = Address::from_slice(&args[12..32]);
            let active = SponsorHub::is_active(&bridge, sponsor);
            Ok(PrecompileOutput::new(GAS_SPONSOR_VIEW, encode_bool(active)))
        }
        s if s == SEL_GET_VALID_UNTIL => {
            if gas < GAS_SPONSOR_VIEW {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            if let Some(r) = require_addr(args, GAS_SPONSOR_VIEW) {
                return r;
            }
            let sponsor = Address::from_slice(&args[12..32]);
            let valid_until = SponsorHub::get_valid_until(&bridge, sponsor);
            Ok(PrecompileOutput::new(
                GAS_SPONSOR_VIEW,
                Bytes::copy_from_slice(&U256::from(valid_until).to_be_bytes::<32>()),
            ))
        }
        s if s == SEL_GET_SPONSOR_TYPE => {
            if gas < GAS_SPONSOR_VIEW {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            if let Some(r) = require_addr(args, GAS_SPONSOR_VIEW) {
                return r;
            }
            let sponsor = Address::from_slice(&args[12..32]);
            let type_val = SponsorGovernance::get_sponsor_type(&bridge, sponsor)
                .map(|t| t as u8)
                .unwrap_or(u8::MAX);
            Ok(PrecompileOutput::new(
                GAS_SPONSOR_VIEW,
                Bytes::copy_from_slice(&U256::from(type_val).to_be_bytes::<32>()),
            ))
        }
        s if s == SEL_GET_POLICY => {
            if gas < GAS_SPONSOR_VIEW {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            if let Some(r) = require_addr(args, GAS_SPONSOR_VIEW) {
                return r;
            }
            let sponsor = Address::from_slice(&args[12..32]);
            match SponsorHub::load_config(&bridge, sponsor) {
                Some(config) => {
                    let mut rlp_buf = Vec::new();
                    alloy_rlp::Encodable::encode(&config, &mut rlp_buf);
                    Ok(PrecompileOutput::new(GAS_SPONSOR_VIEW, rlp_buf.into()))
                }
                None => revert(GAS_SPONSOR_VIEW, b"no policy registered"),
            }
        }
        s if s == SEL_CAN_SPONSOR => {
            if gas < GAS_SPONSOR_CAN_SPONSOR {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            // args: sponsor(32) + sender(32) + to(32) + selector(32) = 128 bytes
            if args.len() < 128 {
                return revert(GAS_SPONSOR_CAN_SPONSOR, b"input too short for canSponsor");
            }
            let sponsor = Address::from_slice(&args[12..32]);
            let sender = Address::from_slice(&args[44..64]);
            let to = Address::from_slice(&args[76..96]);
            let selector_bytes = &args[96 + 28..128]; // bytes4 right-aligned in 32

            let config = match SponsorHub::load_config(&bridge, sponsor) {
                Some(c) => c,
                None => {
                    return Ok(PrecompileOutput::new(GAS_SPONSOR_CAN_SPONSOR, encode_bool(false)));
                }
            };

            let validation = SponsorValidationInput {
                sponsor,
                sender,
                to,
                calldata: selector_bytes,
                gas_limit: U256::ZERO,
                block_timestamp,
            };
            let can = SponsorValidator::validate(&bridge, &validation, &config).is_ok();
            Ok(PrecompileOutput::new(GAS_SPONSOR_CAN_SPONSOR, encode_bool(can)))
        }
        s if s == SEL_OWNER => {
            if gas < GAS_SPONSOR_VIEW {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            let owner = SponsorGovernance::get_owner(&bridge);
            let mut output = [0u8; 32];
            output[12..32].copy_from_slice(owner.as_slice());
            Ok(PrecompileOutput::new(GAS_SPONSOR_VIEW, Bytes::copy_from_slice(&output)))
        }
        _ => Ok(PrecompileOutput::new_reverted(0, Bytes::copy_from_slice(b"unknown selector"))),
    }
}

/// Helper: revert with gas cost and message.
fn revert(gas: u64, msg: &[u8]) -> PrecompileResult {
    Ok(PrecompileOutput::new_reverted(gas, Bytes::copy_from_slice(msg)))
}

/// Helper: require at least 32 bytes of args (for address parameter).
fn require_addr(args: &[u8], gas: u64) -> Option<PrecompileResult> {
    if args.len() < 32 {
        Some(revert(gas, b"input too short"))
    } else {
        None
    }
}

/// Helper: ABI-encode a bool as 32-byte word.
fn encode_bool(val: bool) -> Bytes {
    let mut output = [0u8; 32];
    if val {
        output[31] = 1;
    }
    Bytes::copy_from_slice(&output)
}
