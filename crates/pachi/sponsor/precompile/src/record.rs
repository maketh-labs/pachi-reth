//! Sponsor record types.

use alloy_primitives::{Address, FixedBytes, U256};
use alloy_rlp::{Decodable, Encodable, RlpDecodable, RlpEncodable};
use pachi_primitives::{Constraint, Limit};

/// Sponsor type (deposit or mint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SponsorType {
    /// Gas paid from sponsor's deposited funds.
    Deposit = 0,
    /// Gas paid by protocol minting native token.
    Mint = 1,
}

impl SponsorType {
    /// Converts a `u8` to a `SponsorType`.
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Deposit),
            1 => Some(Self::Mint),
            _ => None,
        }
    }
}

impl Encodable for SponsorType {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        (*self as u8).encode(out);
    }

    fn length(&self) -> usize {
        (*self as u8).length()
    }
}

impl Decodable for SponsorType {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let v = u8::decode(buf)?;
        Self::from_u8(v).ok_or(alloy_rlp::Error::Custom("invalid SponsorType"))
    }
}

/// Sponsor status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SponsorStatus {
    /// Sponsor is active and can sponsor transactions.
    Active = 0,
    /// Sponsor has been deactivated (deposit preserved).
    Deactivated = 1,
}

impl SponsorStatus {
    /// Converts a `u8` to a `SponsorStatus`.
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Active),
            1 => Some(Self::Deactivated),
            _ => None,
        }
    }
}

impl Encodable for SponsorStatus {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        (*self as u8).encode(out);
    }

    fn length(&self) -> usize {
        (*self as u8).length()
    }
}

impl Decodable for SponsorStatus {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let v = u8::decode(buf)?;
        Self::from_u8(v).ok_or(alloy_rlp::Error::Custom("invalid SponsorStatus"))
    }
}

/// Call policy for sponsors.
#[derive(Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable)]
pub struct SponsorCallPolicy {
    /// Target contract address.
    pub target: Address,
    /// Function selector.
    pub selector: FixedBytes<4>,
    /// Per-argument constraints.
    pub constraints: Vec<Constraint>,
}

/// Transfer policy for sponsors.
#[derive(Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable)]
pub struct SponsorTransferPolicy {
    /// Transfer recipient.
    pub target: Address,
    /// Max value per transaction.
    pub max_value_per_tx: U256,
}

/// Sponsor configuration (stored on-chain as RLP blob).
#[derive(Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable)]
pub struct SponsorConfig {
    /// Allowed senders (empty = anyone).
    pub allowed_senders: Vec<Address>,
    /// Whitelisted contract function calls.
    pub call_policies: Vec<SponsorCallPolicy>,
    /// Whitelisted transfers.
    pub transfer_policies: Vec<SponsorTransferPolicy>,
    /// Total gas fee cap.
    pub global_fee_limit: Limit,
    /// Per-sender gas fee cap.
    pub per_sender_fee_limit: Limit,
    /// Max gas units per single tx.
    pub max_gas_per_tx: U256,
    /// Total tx count cap.
    pub global_tx_limit: Limit,
    /// Per-sender tx count cap.
    pub per_sender_tx_limit: Limit,
    /// Policy expiration (unix timestamp).
    pub valid_until: u64,
}

/// On-chain sponsor record.
#[derive(Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable)]
pub struct SponsorRecord {
    /// Sponsor status.
    pub status: SponsorStatus,
    /// Sponsor type (deposit or mint).
    pub sponsor_type: SponsorType,
    /// Deposit balance (only meaningful for deposit mode).
    pub balance: U256,
    /// Full sponsor configuration.
    pub config: SponsorConfig,
    /// Creation timestamp.
    pub created_at: u64,
}
