//! Oracle confidence levels.

use serde::{Deserialize, Serialize};

/// Confidence level for oracle price data.
///
/// Assigned based on source count and price spread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Confidence {
    /// Spread <= 10bps, sources >= 3.
    High = 0,
    /// Spread <= 50bps, sources >= 3.
    Medium = 1,
    /// Spread > 50bps OR sources < 3.
    Degraded = 2,
    /// All sources down, no current data.
    Unavailable = 3,
}

impl Confidence {
    /// Try to convert a `u8` into a [`Confidence`].
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::High),
            1 => Some(Self::Medium),
            2 => Some(Self::Degraded),
            3 => Some(Self::Unavailable),
            _ => None,
        }
    }

    /// Returns the `u8` value.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Returns the worse (higher numeric value) of two confidence levels.
    pub const fn worse(self, other: Self) -> Self {
        if self.as_u8() > other.as_u8() {
            self
        } else {
            other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering() {
        assert!(Confidence::High < Confidence::Medium);
        assert!(Confidence::Medium < Confidence::Degraded);
        assert!(Confidence::Degraded < Confidence::Unavailable);
    }

    #[test]
    fn roundtrip_u8() {
        for v in 0..=3u8 {
            let c = Confidence::from_u8(v).unwrap();
            assert_eq!(c.as_u8(), v);
        }
        assert_eq!(Confidence::from_u8(4), None);
    }

    #[test]
    fn worse_picks_higher() {
        assert_eq!(Confidence::High.worse(Confidence::Medium), Confidence::Medium);
        assert_eq!(Confidence::Degraded.worse(Confidence::High), Confidence::Degraded);
        assert_eq!(Confidence::Unavailable.worse(Confidence::Medium), Confidence::Unavailable);
    }
}
