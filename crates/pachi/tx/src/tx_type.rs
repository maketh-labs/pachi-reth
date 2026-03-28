use alloy_primitives::U8;
use alloy_rlp::{Decodable, Encodable};
use core::fmt;

/// Pachi-specific transaction type identifiers.
///
/// These extend the Ethereum transaction type space with custom types
/// for session key delegation, gas sponsorship, and system operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum PachiTxType {
    /// Session key delegated execution (0x04).
    /// Signer: session key, Gas payer: authorizer, msg.sender: authorizer.
    Session = 0x04,
    /// Gas-sponsored transaction (0x05).
    /// Signer: sender EOA, Gas payer: sponsor, msg.sender: sender.
    Sponsored = 0x05,
    /// Combined session key + gas sponsor (0x06).
    /// Signer: session key, Gas payer: sponsor, msg.sender: authorizer.
    SessionSponsored = 0x06,
    /// System transaction for protocol operations (0x50).
    /// Unsigned, free gas, from system address.
    System = 0x50,
}

impl PachiTxType {
    /// Returns `true` if the given byte is a valid Pachi transaction type.
    pub const fn is_pachi_type(ty: u8) -> bool {
        matches!(ty, 0x04 | 0x05 | 0x06 | 0x50)
    }
}

impl TryFrom<u8> for PachiTxType {
    type Error = InvalidPachiTxType;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x04 => Ok(Self::Session),
            0x05 => Ok(Self::Sponsored),
            0x06 => Ok(Self::SessionSponsored),
            0x50 => Ok(Self::System),
            _ => Err(InvalidPachiTxType(value)),
        }
    }
}

impl From<PachiTxType> for u8 {
    fn from(tx_type: PachiTxType) -> Self {
        tx_type as Self
    }
}

impl From<PachiTxType> for U8 {
    fn from(tx_type: PachiTxType) -> Self {
        Self::from(tx_type as u8)
    }
}

impl fmt::Display for PachiTxType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session => write!(f, "Session (0x04)"),
            Self::Sponsored => write!(f, "Sponsored (0x05)"),
            Self::SessionSponsored => write!(f, "SessionSponsored (0x06)"),
            Self::System => write!(f, "System (0x50)"),
        }
    }
}

impl Encodable for PachiTxType {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        (*self as u8).encode(out);
    }

    fn length(&self) -> usize {
        (*self as u8).length()
    }
}

impl Decodable for PachiTxType {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let ty = u8::decode(buf)?;
        Self::try_from(ty).map_err(|_| alloy_rlp::Error::Custom("invalid Pachi tx type"))
    }
}

/// Error for invalid Pachi transaction type byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid Pachi transaction type: 0x{0:02x}")]
pub struct InvalidPachiTxType(pub u8);

/// Subtypes for [`PachiTxType::System`] transactions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SystemTxSubtype {
    /// Oracle price update — mandatory every block.
    #[default]
    OracleUpdate = 0x00,
    /// VRF fulfillment — one per VRF request in the same block.
    VrfFulfill = 0x01,
}

impl TryFrom<u8> for SystemTxSubtype {
    type Error = InvalidSystemTxSubtype;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Self::OracleUpdate),
            0x01 => Ok(Self::VrfFulfill),
            _ => Err(InvalidSystemTxSubtype(value)),
        }
    }
}

impl From<SystemTxSubtype> for u8 {
    fn from(subtype: SystemTxSubtype) -> Self {
        subtype as Self
    }
}

impl Encodable for SystemTxSubtype {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        (*self as u8).encode(out);
    }

    fn length(&self) -> usize {
        (*self as u8).length()
    }
}

impl Decodable for SystemTxSubtype {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let ty = u8::decode(buf)?;
        Self::try_from(ty).map_err(|_| alloy_rlp::Error::Custom("invalid system tx subtype"))
    }
}

/// Error for invalid system transaction subtype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid system tx subtype: 0x{0:02x}")]
pub struct InvalidSystemTxSubtype(pub u8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tx_type_roundtrip() {
        for (ty, expected) in [
            (0x04u8, PachiTxType::Session),
            (0x05, PachiTxType::Sponsored),
            (0x06, PachiTxType::SessionSponsored),
            (0x50, PachiTxType::System),
        ] {
            assert_eq!(PachiTxType::try_from(ty).unwrap(), expected);
            assert_eq!(u8::from(expected), ty);
        }
    }

    #[test]
    fn invalid_tx_type() {
        assert!(PachiTxType::try_from(0x00).is_err());
        assert!(PachiTxType::try_from(0x03).is_err());
        assert!(PachiTxType::try_from(0xFF).is_err());
    }

    #[test]
    fn system_subtype_roundtrip() {
        assert_eq!(SystemTxSubtype::try_from(0x00).unwrap(), SystemTxSubtype::OracleUpdate);
        assert_eq!(SystemTxSubtype::try_from(0x01).unwrap(), SystemTxSubtype::VrfFulfill);
        assert!(SystemTxSubtype::try_from(0x02).is_err());
    }

    #[test]
    fn rlp_roundtrip() {
        let mut buf = vec![];
        PachiTxType::Session.encode(&mut buf);
        let decoded = PachiTxType::decode(&mut &buf[..]).unwrap();
        assert_eq!(decoded, PachiTxType::Session);
    }
}
