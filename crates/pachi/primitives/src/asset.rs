//! Supported asset identifiers.

use alloy_primitives::U256;
use serde::{Deserialize, Serialize};

/// Supported asset identifier for the native price oracle.
///
/// Hard-coded — additions require a hard fork.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum AssetId {
    /// BTC/USD
    BTC = 0x01,
    /// ETH/USD
    ETH = 0x02,
    /// SOL/USD
    SOL = 0x03,
    /// USDC/USD (depeg detection)
    USDC = 0x04,
    /// USDT/USD (depeg detection)
    USDT = 0x05,
    /// DAI/USD (depeg detection)
    DAI = 0x06,
}

impl AssetId {
    /// All supported assets in order.
    pub const ALL: [Self; 6] = [Self::BTC, Self::ETH, Self::SOL, Self::USDC, Self::USDT, Self::DAI];

    /// Try to convert a `u8` to an [`AssetId`].
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::BTC),
            0x02 => Some(Self::ETH),
            0x03 => Some(Self::SOL),
            0x04 => Some(Self::USDC),
            0x05 => Some(Self::USDT),
            0x06 => Some(Self::DAI),
            _ => None,
        }
    }

    /// Returns true if the given `u8` is a supported asset ID.
    pub const fn is_supported(v: u8) -> bool {
        Self::from_u8(v).is_some()
    }

    /// Returns the `u8` value.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Returns the storage base slot for this asset in the oracle precompile.
    ///
    /// Each asset uses 2 slots: `base` for price, `base + 1` for packed timestamp/confidence.
    pub fn storage_base_slot(self) -> U256 {
        U256::from(self.as_u8() as u64 * 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_u8() {
        for asset in AssetId::ALL {
            assert_eq!(AssetId::from_u8(asset.as_u8()), Some(asset));
        }
    }

    #[test]
    fn unsupported_returns_none() {
        assert_eq!(AssetId::from_u8(0x00), None);
        assert_eq!(AssetId::from_u8(0x07), None);
        assert_eq!(AssetId::from_u8(0xFF), None);
    }

    #[test]
    fn is_supported() {
        assert!(AssetId::is_supported(0x01));
        assert!(AssetId::is_supported(0x06));
        assert!(!AssetId::is_supported(0x00));
        assert!(!AssetId::is_supported(0x07));
    }

    #[test]
    fn storage_slots_no_overlap() {
        let slots: Vec<U256> = AssetId::ALL.iter().map(|a| a.storage_base_slot()).collect();
        for i in 0..slots.len() {
            for j in (i + 1)..slots.len() {
                // Each asset occupies 2 slots, so base slots must differ by at least 2
                let diff = slots[j] - slots[i];
                assert!(diff >= U256::from(2));
            }
        }
    }

    #[test]
    fn serde_roundtrip() {
        let asset = AssetId::BTC;
        let json = serde_json::to_string(&asset).unwrap();
        let deserialized: AssetId = serde_json::from_str(&json).unwrap();
        assert_eq!(asset, deserialized);
    }
}
