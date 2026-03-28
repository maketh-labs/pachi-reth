use crate::PachiTxType;
use alloy_consensus::{
    transaction::{RlpEcdsaDecodableTx, RlpEcdsaEncodableTx},
    SignableTransaction, Transaction,
};
use alloy_eips::{
    eip2718::IsTyped2718, eip2930::AccessList, eip7702::SignedAuthorization, Typed2718,
};
use alloy_primitives::{Address, Bytes, ChainId, Signature, TxKind, B256, U256};
use alloy_rlp::{BufMut, Decodable, Encodable};

/// A combined session-key + gas-sponsored transaction (type 0x06).
///
/// The session key signs the transaction, `msg.sender` is set to the authorizer,
/// and gas is paid by the sponsor.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SessionSponsoredTx {
    /// Chain ID (EIP-155).
    pub chain_id: ChainId,
    /// Session key nonce.
    pub nonce: u64,
    /// Max priority fee per gas (EIP-1559).
    pub max_priority_fee_per_gas: u128,
    /// Max fee per gas (EIP-1559).
    pub max_fee_per_gas: u128,
    /// Gas limit.
    pub gas_limit: u64,
    /// Destination address.
    pub to: Address,
    /// Transfer value.
    pub value: U256,
    /// Calldata.
    pub input: Bytes,
    /// Access list (EIP-2930).
    pub access_list: AccessList,
    /// Hash identifying the session configuration.
    pub session_hash: B256,
    /// The authorizer (main wallet) address.
    pub authorizer: Address,
    /// ABI-encoded `SessionConfig` bytes (for hash verification).
    pub session_config: Bytes,
    /// Sponsor address (registry lookup).
    pub sponsor: Address,
}

impl SessionSponsoredTx {
    /// Returns the transaction type.
    pub const fn tx_type() -> PachiTxType {
        PachiTxType::SessionSponsored
    }

    /// Calculates a heuristic for the in-memory size.
    pub fn size(&self) -> usize {
        size_of::<Self>() + self.access_list.size() + self.input.len() + self.session_config.len()
    }
}

impl RlpEcdsaEncodableTx for SessionSponsoredTx {
    fn rlp_encoded_fields_length(&self) -> usize {
        self.chain_id.length() +
            self.nonce.length() +
            self.max_priority_fee_per_gas.length() +
            self.max_fee_per_gas.length() +
            self.gas_limit.length() +
            self.to.length() +
            self.value.length() +
            self.input.0.length() +
            self.access_list.length() +
            self.session_hash.length() +
            self.authorizer.length() +
            self.session_config.0.length() +
            self.sponsor.length()
    }

    fn rlp_encode_fields(&self, out: &mut dyn BufMut) {
        self.chain_id.encode(out);
        self.nonce.encode(out);
        self.max_priority_fee_per_gas.encode(out);
        self.max_fee_per_gas.encode(out);
        self.gas_limit.encode(out);
        self.to.encode(out);
        self.value.encode(out);
        self.input.0.encode(out);
        self.access_list.encode(out);
        self.session_hash.encode(out);
        self.authorizer.encode(out);
        self.session_config.0.encode(out);
        self.sponsor.encode(out);
    }
}

impl RlpEcdsaDecodableTx for SessionSponsoredTx {
    const DEFAULT_TX_TYPE: u8 = PachiTxType::SessionSponsored as u8;

    fn rlp_decode_fields(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        Ok(Self {
            chain_id: Decodable::decode(buf)?,
            nonce: Decodable::decode(buf)?,
            max_priority_fee_per_gas: Decodable::decode(buf)?,
            max_fee_per_gas: Decodable::decode(buf)?,
            gas_limit: Decodable::decode(buf)?,
            to: Decodable::decode(buf)?,
            value: Decodable::decode(buf)?,
            input: Decodable::decode(buf)?,
            access_list: Decodable::decode(buf)?,
            session_hash: Decodable::decode(buf)?,
            authorizer: Decodable::decode(buf)?,
            session_config: Decodable::decode(buf)?,
            sponsor: Decodable::decode(buf)?,
        })
    }
}

impl Transaction for SessionSponsoredTx {
    fn chain_id(&self) -> Option<ChainId> {
        Some(self.chain_id)
    }

    fn nonce(&self) -> u64 {
        self.nonce
    }

    fn gas_limit(&self) -> u64 {
        self.gas_limit
    }

    fn gas_price(&self) -> Option<u128> {
        None
    }

    fn max_fee_per_gas(&self) -> u128 {
        self.max_fee_per_gas
    }

    fn max_priority_fee_per_gas(&self) -> Option<u128> {
        Some(self.max_priority_fee_per_gas)
    }

    fn max_fee_per_blob_gas(&self) -> Option<u128> {
        None
    }

    fn priority_fee_or_price(&self) -> u128 {
        self.max_priority_fee_per_gas
    }

    fn effective_gas_price(&self, base_fee: Option<u64>) -> u128 {
        alloy_eips::eip1559::calc_effective_gas_price(
            self.max_fee_per_gas,
            self.max_priority_fee_per_gas,
            base_fee,
        )
    }

    fn is_dynamic_fee(&self) -> bool {
        true
    }

    fn kind(&self) -> TxKind {
        TxKind::Call(self.to)
    }

    fn is_create(&self) -> bool {
        false
    }

    fn value(&self) -> U256 {
        self.value
    }

    fn input(&self) -> &Bytes {
        &self.input
    }

    fn access_list(&self) -> Option<&AccessList> {
        Some(&self.access_list)
    }

    fn blob_versioned_hashes(&self) -> Option<&[B256]> {
        None
    }

    fn authorization_list(&self) -> Option<&[SignedAuthorization]> {
        None
    }
}

impl Typed2718 for SessionSponsoredTx {
    fn ty(&self) -> u8 {
        PachiTxType::SessionSponsored as u8
    }
}

impl IsTyped2718 for SessionSponsoredTx {
    fn is_type(type_id: u8) -> bool {
        type_id == PachiTxType::SessionSponsored as u8
    }
}

impl SignableTransaction<Signature> for SessionSponsoredTx {
    fn set_chain_id(&mut self, chain_id: ChainId) {
        self.chain_id = chain_id;
    }

    fn encode_for_signing(&self, out: &mut dyn BufMut) {
        out.put_u8(Self::tx_type() as u8);
        self.encode(out);
    }

    fn payload_len_for_signature(&self) -> usize {
        self.length() + 1
    }
}

impl Encodable for SessionSponsoredTx {
    fn encode(&self, out: &mut dyn BufMut) {
        self.rlp_encode(out);
    }

    fn length(&self) -> usize {
        self.rlp_encoded_length()
    }
}

impl Decodable for SessionSponsoredTx {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        Self::rlp_decode(buf)
    }
}
