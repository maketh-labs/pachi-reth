//! Event emission helpers for Pachi precompiles.
//!
//! Computes topic0 (event signature hash) and encodes event parameters.

use alloy_primitives::{keccak256, Address, B256, U256};

use crate::state_bridge::EvmStateBridge;

// ── SessionRegistry events ──

/// `SessionCreated(bytes32 indexed sessionHash, address indexed authorizer, address signer, uint64
/// expiresAt)`
const SESSION_CREATED_SIG: &[u8] = b"SessionCreated(bytes32,address,address,uint64)";

/// `SessionRevoked(bytes32 indexed sessionHash, address indexed authorizer)`
const SESSION_REVOKED_SIG: &[u8] = b"SessionRevoked(bytes32,address)";

/// `SessionReplaced(bytes32 indexed oldHash, bytes32 indexed newHash, address indexed authorizer)`
const SESSION_REPLACED_SIG: &[u8] = b"SessionReplaced(bytes32,bytes32,address)";

/// Emits `SessionCreated`.
pub(crate) fn emit_session_created(
    bridge: &EvmStateBridge<'_>,
    precompile_addr: Address,
    session_hash: B256,
    authorizer: Address,
    signer: Address,
    expires_at: u64,
) {
    let topic0 = keccak256(SESSION_CREATED_SIG);
    let topic1 = session_hash;
    let topic2 = addr_to_topic(authorizer);

    // Non-indexed: signer (address) + expiresAt (uint64)
    let mut data = Vec::with_capacity(64);
    data.extend_from_slice(&addr_to_word(signer));
    data.extend_from_slice(&U256::from(expires_at).to_be_bytes::<32>());

    bridge.emit_log(precompile_addr, vec![topic0, topic1, topic2], data);
}

/// Emits `SessionRevoked`.
pub(crate) fn emit_session_revoked(
    bridge: &EvmStateBridge<'_>,
    precompile_addr: Address,
    session_hash: B256,
    authorizer: Address,
) {
    let topic0 = keccak256(SESSION_REVOKED_SIG);
    let topic1 = session_hash;
    let topic2 = addr_to_topic(authorizer);
    bridge.emit_log(precompile_addr, vec![topic0, topic1, topic2], vec![]);
}

/// Emits `SessionReplaced`.
pub(crate) fn emit_session_replaced(
    bridge: &EvmStateBridge<'_>,
    precompile_addr: Address,
    old_hash: B256,
    new_hash: B256,
    authorizer: Address,
) {
    let topic0 = keccak256(SESSION_REPLACED_SIG);
    let topic1 = old_hash;
    let topic2 = new_hash;
    let topic3 = addr_to_topic(authorizer);
    bridge.emit_log(precompile_addr, vec![topic0, topic1, topic2, topic3], vec![]);
}

// ── SponsorHub events ──

const DEPOSITED_SIG: &[u8] = b"Deposited(address,uint256,uint256)";
const WITHDRAWN_SIG: &[u8] = b"Withdrawn(address,uint256,uint256)";
const POLICY_REGISTERED_SIG: &[u8] = b"PolicyRegistered(address,uint8,uint64)";
const POLICY_DEACTIVATED_SIG: &[u8] = b"PolicyDeactivated(address)";
const MINT_APPROVED_SIG: &[u8] = b"MintApproved(address,address)";
const MINT_REVOKED_SIG: &[u8] = b"MintRevoked(address,address)";
const OWNERSHIP_TRANSFERRED_SIG: &[u8] = b"OwnershipTransferred(address,address)";

/// Emits `PolicyRegistered(address indexed sponsor, uint8 sponsorType, uint64 validUntil)`.
pub(crate) fn emit_policy_registered(
    bridge: &EvmStateBridge<'_>,
    addr: Address,
    sponsor: Address,
    sponsor_type: u8,
    valid_until: u64,
) {
    let topic0 = keccak256(POLICY_REGISTERED_SIG);
    let topic1 = addr_to_topic(sponsor);
    let mut data = Vec::with_capacity(64);
    data.extend_from_slice(&U256::from(sponsor_type).to_be_bytes::<32>());
    data.extend_from_slice(&U256::from(valid_until).to_be_bytes::<32>());
    bridge.emit_log(addr, vec![topic0, topic1], data);
}

/// Emits `PolicyDeactivated(address indexed sponsor)`.
pub(crate) fn emit_policy_deactivated(
    bridge: &EvmStateBridge<'_>,
    addr: Address,
    sponsor: Address,
) {
    let topic0 = keccak256(POLICY_DEACTIVATED_SIG);
    let topic1 = addr_to_topic(sponsor);
    bridge.emit_log(addr, vec![topic0, topic1], vec![]);
}

/// Emits `Deposited(address indexed sponsor, uint256 amount, uint256 newBalance)`.
pub(crate) fn emit_deposited(
    bridge: &EvmStateBridge<'_>,
    addr: Address,
    sponsor: Address,
    amount: U256,
    new_balance: U256,
) {
    let topic0 = keccak256(DEPOSITED_SIG);
    let topic1 = addr_to_topic(sponsor);
    let mut data = Vec::with_capacity(64);
    data.extend_from_slice(&amount.to_be_bytes::<32>());
    data.extend_from_slice(&new_balance.to_be_bytes::<32>());
    bridge.emit_log(addr, vec![topic0, topic1], data);
}

/// Emits `Withdrawn(address indexed sponsor, uint256 amount, uint256 newBalance)`.
pub(crate) fn emit_withdrawn(
    bridge: &EvmStateBridge<'_>,
    addr: Address,
    sponsor: Address,
    amount: U256,
    new_balance: U256,
) {
    let topic0 = keccak256(WITHDRAWN_SIG);
    let topic1 = addr_to_topic(sponsor);
    let mut data = Vec::with_capacity(64);
    data.extend_from_slice(&amount.to_be_bytes::<32>());
    data.extend_from_slice(&new_balance.to_be_bytes::<32>());
    bridge.emit_log(addr, vec![topic0, topic1], data);
}

/// Emits `MintApproved(address indexed sponsor, address indexed approvedBy)`.
pub(crate) fn emit_mint_approved(
    bridge: &EvmStateBridge<'_>,
    addr: Address,
    sponsor: Address,
    approved_by: Address,
) {
    let topic0 = keccak256(MINT_APPROVED_SIG);
    let topic1 = addr_to_topic(sponsor);
    let topic2 = addr_to_topic(approved_by);
    bridge.emit_log(addr, vec![topic0, topic1, topic2], vec![]);
}

/// Emits `MintRevoked(address indexed sponsor, address indexed revokedBy)`.
pub(crate) fn emit_mint_revoked(
    bridge: &EvmStateBridge<'_>,
    addr: Address,
    sponsor: Address,
    revoked_by: Address,
) {
    let topic0 = keccak256(MINT_REVOKED_SIG);
    let topic1 = addr_to_topic(sponsor);
    let topic2 = addr_to_topic(revoked_by);
    bridge.emit_log(addr, vec![topic0, topic1, topic2], vec![]);
}

/// Emits `OwnershipTransferred(address indexed previousOwner, address indexed newOwner)`.
pub(crate) fn emit_ownership_transferred(
    bridge: &EvmStateBridge<'_>,
    addr: Address,
    previous_owner: Address,
    new_owner: Address,
) {
    let topic0 = keccak256(OWNERSHIP_TRANSFERRED_SIG);
    let topic1 = addr_to_topic(previous_owner);
    let topic2 = addr_to_topic(new_owner);
    bridge.emit_log(addr, vec![topic0, topic1, topic2], vec![]);
}

/// Convert an address to a 32-byte topic (left-padded with zeros).
fn addr_to_topic(addr: Address) -> B256 {
    let mut topic = [0u8; 32];
    topic[12..32].copy_from_slice(addr.as_slice());
    B256::from(topic)
}

/// Convert an address to a 32-byte ABI word (left-padded).
fn addr_to_word(addr: Address) -> [u8; 32] {
    let mut word = [0u8; 32];
    word[12..32].copy_from_slice(addr.as_slice());
    word
}
