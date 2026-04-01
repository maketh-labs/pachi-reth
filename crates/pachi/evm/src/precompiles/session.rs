//! `SessionRegistry` precompile dispatch (0x0800).
//!
//! Routes ABI-encoded calls to the session state machine.
//!
//! # Selectors (keccak256 of canonical signature)
//!
//! - `createSession(address,uint64,bytes32)` → `0xaad64115` (authorizer = msg.sender)
//! - `revokeSession(bytes32)` → `0xa7fed385`
//! - `getSession(bytes32)` → `0x39b240bd`
//! - `getActiveSessions(address)` → `0x80ea53de`
//! - `isValid(bytes32)` → `0x6a938567`

use alloy_evm::precompiles::PrecompileInput;
use alloy_primitives::{Address, Bytes, FixedBytes, B256, U256};
use revm::precompile::{PrecompileOutput, PrecompileResult};

use crate::{events, state_bridge::EvmStateBridge};
use pachi_primitives::DEFAULT_MAX_SESSIONS_PER_ACCOUNT;
use pachi_session_precompile::{
    SessionRegistry, GAS_SESSION_CREATE, GAS_SESSION_GET, GAS_SESSION_IS_VALID,
    GAS_SESSION_LIST_BASE, GAS_SESSION_LIST_PER, GAS_SESSION_REVOKE,
};

// Function selectors: keccak256(canonical_signature)[:4]
const SEL_CREATE_SESSION: FixedBytes<4> = FixedBytes::new([0xaa, 0xd6, 0x41, 0x15]);
const SEL_REVOKE_SESSION: FixedBytes<4> = FixedBytes::new([0xa7, 0xfe, 0xd3, 0x85]);
const SEL_GET_SESSION: FixedBytes<4> = FixedBytes::new([0x39, 0xb2, 0x40, 0xbd]);
const SEL_GET_ACTIVE_SESSIONS: FixedBytes<4> = FixedBytes::new([0x80, 0xea, 0x53, 0xde]);
const SEL_IS_VALID: FixedBytes<4> = FixedBytes::new([0x6a, 0x93, 0x85, 0x67]);

/// `SessionRegistry` precompile entry point.
pub(crate) fn session_registry_precompile(input: PrecompileInput<'_>) -> PrecompileResult {
    let data = input.data;
    if data.len() < 4 {
        return Ok(PrecompileOutput::new_reverted(0, Bytes::copy_from_slice(b"input too short")));
    }

    let selector = FixedBytes::<4>::from_slice(&data[..4]);
    let args = &data[4..];
    let gas = input.gas;
    let caller = input.caller;
    let is_static = input.is_static;
    let mut bridge = EvmStateBridge::new(input.internals);
    let block_timestamp = bridge.block_timestamp();

    match selector {
        s if s == SEL_CREATE_SESSION => {
            if is_static {
                return Ok(PrecompileOutput::new_reverted(
                    GAS_SESSION_CREATE,
                    Bytes::copy_from_slice(b"state-modifying call in static context"),
                ));
            }
            if gas < GAS_SESSION_CREATE {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            handle_create_session(args, caller, block_timestamp, &mut bridge)
        }
        s if s == SEL_REVOKE_SESSION => {
            if is_static {
                return Ok(PrecompileOutput::new_reverted(
                    GAS_SESSION_REVOKE,
                    Bytes::copy_from_slice(b"state-modifying call in static context"),
                ));
            }
            if gas < GAS_SESSION_REVOKE {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            handle_revoke_session(args, caller, &mut bridge)
        }
        s if s == SEL_GET_SESSION => {
            if gas < GAS_SESSION_GET {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            handle_get_session(args, &bridge)
        }
        s if s == SEL_GET_ACTIVE_SESSIONS => {
            if gas < GAS_SESSION_LIST_BASE {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            handle_get_active_sessions(args, block_timestamp, gas, &bridge)
        }
        s if s == SEL_IS_VALID => {
            if gas < GAS_SESSION_IS_VALID {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            handle_is_valid(args, block_timestamp, &bridge)
        }
        _ => Ok(PrecompileOutput::new_reverted(0, Bytes::copy_from_slice(b"unknown selector"))),
    }
}

/// `createSession(address signer, uint64 expiresAt, bytes32 sessionHash)`
fn handle_create_session(
    args: &[u8],
    caller: Address,
    block_timestamp: u64,
    bridge: &mut EvmStateBridge<'_>,
) -> PrecompileResult {
    // ABI: signer(address/32) + expiresAt(uint64/32) + sessionHash(bytes32/32) = 96 bytes
    if args.len() < 96 {
        return Ok(PrecompileOutput::new_reverted(
            GAS_SESSION_CREATE,
            Bytes::copy_from_slice(b"input too short for createSession"),
        ));
    }

    let signer = Address::from_slice(&args[12..32]); // address is left-padded
    let expires_at = U256::from_be_slice(&args[32..64]).saturating_to::<u64>();
    let session_hash = B256::from_slice(&args[64..96]);

    match SessionRegistry::create_session(
        bridge,
        caller, // authorizer = msg.sender
        signer,
        expires_at,
        session_hash,
        block_timestamp,
        DEFAULT_MAX_SESSIONS_PER_ACCOUNT,
    ) {
        Ok((hash, replaced)) => {
            let precompile_addr = pachi_session_precompile::SESSION_REGISTRY_ADDRESS;
            if let Some(old_hash) = replaced {
                events::emit_session_replaced(bridge, precompile_addr, old_hash, hash, caller);
            }
            events::emit_session_created(bridge, precompile_addr, hash, caller, signer, expires_at);
            Ok(PrecompileOutput::new(GAS_SESSION_CREATE, Bytes::copy_from_slice(hash.as_slice())))
        }
        Err(e) => Ok(PrecompileOutput::new_reverted(
            GAS_SESSION_CREATE,
            Bytes::copy_from_slice(e.to_string().as_bytes()),
        )),
    }
}

/// `revokeSession(bytes32 sessionHash)`
fn handle_revoke_session(
    args: &[u8],
    caller: Address,
    bridge: &mut EvmStateBridge<'_>,
) -> PrecompileResult {
    if args.len() < 32 {
        return Ok(PrecompileOutput::new_reverted(
            GAS_SESSION_REVOKE,
            Bytes::copy_from_slice(b"input too short for revokeSession"),
        ));
    }

    let session_hash = B256::from_slice(&args[..32]);

    match SessionRegistry::revoke_session(bridge, caller, &session_hash) {
        Ok(()) => {
            events::emit_session_revoked(
                bridge,
                pachi_session_precompile::SESSION_REGISTRY_ADDRESS,
                session_hash,
                caller,
            );
            Ok(PrecompileOutput::new(GAS_SESSION_REVOKE, Bytes::new()))
        }
        Err(e) => Ok(PrecompileOutput::new_reverted(
            GAS_SESSION_REVOKE,
            Bytes::copy_from_slice(e.to_string().as_bytes()),
        )),
    }
}

/// `getSession(bytes32 sessionHash) -> (uint8, address, address, uint64, uint64)`
fn handle_get_session(args: &[u8], bridge: &EvmStateBridge<'_>) -> PrecompileResult {
    if args.len() < 32 {
        return Ok(PrecompileOutput::new_reverted(
            GAS_SESSION_GET,
            Bytes::copy_from_slice(b"input too short for getSession"),
        ));
    }

    let session_hash = B256::from_slice(&args[..32]);

    match SessionRegistry::get_session(bridge, &session_hash) {
        Ok(record) => {
            let mut output = Vec::with_capacity(160);
            output.extend_from_slice(&U256::from(record.status as u8).to_be_bytes::<32>());
            output.extend_from_slice(&{
                let mut padded = [0u8; 32];
                padded[12..32].copy_from_slice(record.authorizer.as_slice());
                padded
            });
            output.extend_from_slice(&{
                let mut padded = [0u8; 32];
                padded[12..32].copy_from_slice(record.signer.as_slice());
                padded
            });
            output.extend_from_slice(&U256::from(record.expires_at).to_be_bytes::<32>());
            output.extend_from_slice(&U256::from(record.created_at).to_be_bytes::<32>());
            Ok(PrecompileOutput::new(GAS_SESSION_GET, output.into()))
        }
        Err(e) => Ok(PrecompileOutput::new_reverted(
            GAS_SESSION_GET,
            Bytes::copy_from_slice(e.to_string().as_bytes()),
        )),
    }
}

/// `getActiveSessions(address authorizer) -> bytes32[]`
fn handle_get_active_sessions(
    args: &[u8],
    block_timestamp: u64,
    gas: u64,
    bridge: &EvmStateBridge<'_>,
) -> PrecompileResult {
    if args.len() < 32 {
        return Ok(PrecompileOutput::new_reverted(
            GAS_SESSION_LIST_BASE,
            Bytes::copy_from_slice(b"input too short for getActiveSessions"),
        ));
    }

    let authorizer = Address::from_slice(&args[12..32]);

    let sessions = SessionRegistry::get_active_sessions(
        bridge,
        authorizer,
        block_timestamp,
        DEFAULT_MAX_SESSIONS_PER_ACCOUNT,
    );

    let gas_cost = GAS_SESSION_LIST_BASE + GAS_SESSION_LIST_PER * sessions.len() as u64;
    if gas < gas_cost {
        return Err(revm::precompile::PrecompileError::OutOfGas);
    }

    // ABI encode dynamic array of bytes32
    let mut output = Vec::with_capacity(64 + sessions.len() * 32);
    // offset to array data
    output.extend_from_slice(&U256::from(32).to_be_bytes::<32>());
    // array length
    output.extend_from_slice(&U256::from(sessions.len()).to_be_bytes::<32>());
    for hash in &sessions {
        output.extend_from_slice(hash.as_slice());
    }
    Ok(PrecompileOutput::new(gas_cost, output.into()))
}

/// `isValid(bytes32 sessionHash) -> bool`
fn handle_is_valid(
    args: &[u8],
    block_timestamp: u64,
    bridge: &EvmStateBridge<'_>,
) -> PrecompileResult {
    if args.len() < 32 {
        return Ok(PrecompileOutput::new_reverted(
            GAS_SESSION_IS_VALID,
            Bytes::copy_from_slice(b"input too short for isValid"),
        ));
    }

    let session_hash = B256::from_slice(&args[..32]);
    let valid = SessionRegistry::is_valid(bridge, &session_hash, block_timestamp);

    let mut output = [0u8; 32];
    if valid {
        output[31] = 1;
    }
    Ok(PrecompileOutput::new(GAS_SESSION_IS_VALID, Bytes::copy_from_slice(&output)))
}
