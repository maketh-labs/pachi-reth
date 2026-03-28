use crate::{PachiSystemTx, PachiTxType, SessionSponsoredTx, SessionTx, SponsoredTx};
use alloy_consensus::{
    transaction::{RlpEcdsaDecodableTx, SignerRecoverable, TxHashRef},
    Signed, Transaction,
};
use alloy_eips::{
    eip2718::{Eip2718Error, Eip2718Result, IsTyped2718},
    eip2930::AccessList,
    eip7702::SignedAuthorization,
    Decodable2718, Encodable2718, Typed2718,
};
use alloy_primitives::{keccak256, Address, Bytes, ChainId, Sealed, TxKind, B256, U256};
use alloy_rlp::{BufMut, Decodable, Encodable};
use pachi_primitives::SYSTEM_ADDRESS;

/// The Pachi transaction envelope — a signed wrapper for all Pachi-specific
/// transaction types.
///
/// This is the "on-wire" format: each variant carries its signature (or is sealed
/// for system transactions).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PachiTxEnvelope {
    /// Session-key delegated transaction (0x04), ECDSA-signed by the session key.
    Session(Signed<SessionTx>),
    /// Gas-sponsored transaction (0x05), ECDSA-signed by the sender.
    Sponsored(Signed<SponsoredTx>),
    /// Combined session + sponsor transaction (0x06), ECDSA-signed by the session key.
    SessionSponsored(Signed<SessionSponsoredTx>),
    /// System transaction (0x50), unsigned, sealed with its hash.
    System(Sealed<PachiSystemTx>),
}

impl PachiTxEnvelope {
    /// Returns the [`PachiTxType`] for this envelope.
    pub const fn tx_type(&self) -> PachiTxType {
        match self {
            Self::Session(_) => PachiTxType::Session,
            Self::Sponsored(_) => PachiTxType::Sponsored,
            Self::SessionSponsored(_) => PachiTxType::SessionSponsored,
            Self::System(_) => PachiTxType::System,
        }
    }

    /// Returns `true` if this is a system transaction.
    pub const fn is_system(&self) -> bool {
        matches!(self, Self::System(_))
    }

    /// Heuristic in-memory size.
    pub fn size(&self) -> usize {
        match self {
            Self::Session(tx) => tx.tx().size(),
            Self::Sponsored(tx) => tx.tx().size(),
            Self::SessionSponsored(tx) => tx.tx().size(),
            Self::System(tx) => tx.inner().size(),
        }
    }

    /// Wrap a [`PachiSystemTx`], computing its EIP-2718 hash.
    pub fn seal_system(tx: PachiSystemTx) -> Self {
        let mut buf = Vec::with_capacity(tx.length() + 1);
        buf.put_u8(PachiTxType::System as u8);
        tx.encode(&mut buf);
        let hash = keccak256(&buf);
        Self::System(Sealed::new_unchecked(tx, hash))
    }
}

// ---------------------------------------------------------------------------
// alloy_consensus::Transaction
// ---------------------------------------------------------------------------

macro_rules! delegate {
    ($self:expr => $method:ident($($arg:expr),*)) => {
        match $self {
            PachiTxEnvelope::Session(tx) => tx.$method($($arg),*),
            PachiTxEnvelope::Sponsored(tx) => tx.$method($($arg),*),
            PachiTxEnvelope::SessionSponsored(tx) => tx.$method($($arg),*),
            PachiTxEnvelope::System(tx) => tx.$method($($arg),*),
        }
    };
}

impl Transaction for PachiTxEnvelope {
    fn chain_id(&self) -> Option<ChainId> {
        delegate!(self => chain_id())
    }
    fn nonce(&self) -> u64 {
        delegate!(self => nonce())
    }
    fn gas_limit(&self) -> u64 {
        delegate!(self => gas_limit())
    }
    fn gas_price(&self) -> Option<u128> {
        delegate!(self => gas_price())
    }
    fn max_fee_per_gas(&self) -> u128 {
        delegate!(self => max_fee_per_gas())
    }
    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        delegate!(self => max_priority_fee_per_gas())
    }
    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        delegate!(self => max_fee_per_blob_gas())
    }
    fn priority_fee_or_price(&self) -> u128 {
        delegate!(self => priority_fee_or_price())
    }
    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        delegate!(self => effective_gas_price(base_fee))
    }
    fn is_dynamic_fee(&self) -> bool {
        delegate!(self => is_dynamic_fee())
    }
    fn kind(&self) -> TxKind {
        delegate!(self => kind())
    }
    fn is_create(&self) -> bool {
        delegate!(self => is_create())
    }
    fn value(&self) -> U256 {
        delegate!(self => value())
    }
    fn input(&self) -> &Bytes {
        delegate!(self => input())
    }
    fn access_list(&self) -> Option<&AccessList> {
        delegate!(self => access_list())
    }
    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        delegate!(self => blob_versioned_hashes())
    }
    fn authorization_list(&self) -> Option<&[SignedAuthorization]> {
        delegate!(self => authorization_list())
    }
}

// ---------------------------------------------------------------------------
// EIP-2718 typed encoding
// ---------------------------------------------------------------------------

impl Typed2718 for PachiTxEnvelope {
    fn ty(&self) -> u8 {
        match self {
            Self::Session(tx) => tx.ty(),
            Self::Sponsored(tx) => tx.ty(),
            Self::SessionSponsored(tx) => tx.ty(),
            Self::System(tx) => tx.inner().ty(),
        }
    }
}

impl IsTyped2718 for PachiTxEnvelope {
    fn is_type(type_id: u8) -> bool {
        PachiTxType::is_pachi_type(type_id)
    }
}

impl Encodable2718 for PachiTxEnvelope {
    fn encode_2718_len(&self) -> usize {
        match self {
            Self::Session(tx) => tx.eip2718_encoded_length(),
            Self::Sponsored(tx) => tx.eip2718_encoded_length(),
            Self::SessionSponsored(tx) => tx.eip2718_encoded_length(),
            Self::System(tx) => {
                // type byte + RLP-encoded body
                1 + tx.inner().length()
            }
        }
    }

    fn encode_2718(&self, out: &mut dyn BufMut) {
        match self {
            Self::Session(tx) => tx.eip2718_encode(out),
            Self::Sponsored(tx) => tx.eip2718_encode(out),
            Self::SessionSponsored(tx) => tx.eip2718_encode(out),
            Self::System(tx) => {
                out.put_u8(PachiTxType::System as u8);
                tx.inner().encode(out);
            }
        }
    }
}

impl Decodable2718 for PachiTxEnvelope {
    fn typed_decode(ty: u8, buf: &mut &[u8]) -> Eip2718Result<Self> {
        match PachiTxType::try_from(ty).map_err(|_| Eip2718Error::UnexpectedType(ty))? {
            PachiTxType::Session => Ok(Self::Session(SessionTx::rlp_decode_signed(buf)?)),
            PachiTxType::Sponsored => Ok(Self::Sponsored(SponsoredTx::rlp_decode_signed(buf)?)),
            PachiTxType::SessionSponsored => {
                Ok(Self::SessionSponsored(SessionSponsoredTx::rlp_decode_signed(buf)?))
            }
            PachiTxType::System => {
                let tx = PachiSystemTx::decode(buf)?;
                // Reconstruct hash from the 2718 encoding
                let mut hash_buf = Vec::with_capacity(tx.length() + 1);
                hash_buf.put_u8(PachiTxType::System as u8);
                tx.encode(&mut hash_buf);
                let hash = keccak256(&hash_buf);
                Ok(Self::System(Sealed::new_unchecked(tx, hash)))
            }
        }
    }

    fn fallback_decode(_buf: &mut &[u8]) -> Eip2718Result<Self> {
        // Pachi custom types are always typed (no legacy fallback).
        Err(Eip2718Error::UnexpectedType(0))
    }
}

// ---------------------------------------------------------------------------
// RLP encoding (for database/network wire)
// ---------------------------------------------------------------------------

impl Encodable for PachiTxEnvelope {
    fn encode(&self, out: &mut dyn BufMut) {
        self.encode_2718(out);
    }

    fn length(&self) -> usize {
        self.encode_2718_len()
    }
}

impl Decodable for PachiTxEnvelope {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        Self::decode_2718(buf)
            .map_err(|_| alloy_rlp::Error::Custom("failed to decode PachiTxEnvelope"))
    }
}

// ---------------------------------------------------------------------------
// Signer recovery
// ---------------------------------------------------------------------------

impl SignerRecoverable for PachiTxEnvelope {
    fn recover_signer(&self) -> Result<Address, alloy_consensus::crypto::RecoveryError> {
        match self {
            // Session txs: recover the session key address (NOT the authorizer)
            Self::Session(tx) => SignerRecoverable::recover_signer(tx),
            // Sponsored txs: recover the sender
            Self::Sponsored(tx) => SignerRecoverable::recover_signer(tx),
            // Session+Sponsored txs: recover the session key address
            Self::SessionSponsored(tx) => SignerRecoverable::recover_signer(tx),
            // System txs: no signature — return system address
            Self::System(_) => Ok(SYSTEM_ADDRESS),
        }
    }

    fn recover_signer_unchecked(&self) -> Result<Address, alloy_consensus::crypto::RecoveryError> {
        match self {
            Self::Session(tx) => SignerRecoverable::recover_signer_unchecked(tx),
            Self::Sponsored(tx) => SignerRecoverable::recover_signer_unchecked(tx),
            Self::SessionSponsored(tx) => SignerRecoverable::recover_signer_unchecked(tx),
            Self::System(_) => Ok(SYSTEM_ADDRESS),
        }
    }
}

// ---------------------------------------------------------------------------
// Transaction hash reference
// ---------------------------------------------------------------------------

impl TxHashRef for PachiTxEnvelope {
    fn tx_hash(&self) -> &B256 {
        match self {
            Self::Session(tx) => tx.tx_hash(),
            Self::Sponsored(tx) => tx.tx_hash(),
            Self::SessionSponsored(tx) => tx.tx_hash(),
            Self::System(tx) => tx.hash_ref(),
        }
    }
}
