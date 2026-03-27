//! VRF proof type — a DLEQ proof that gamma was correctly computed.

use crate::VrfError;
use k256::{elliptic_curve::PrimeField, EncodedPoint, Scalar};

/// A VRF proof consisting of the curve point gamma and a Schnorr/DLEQ proof (c, s).
///
/// Encodes as: `gamma_compressed (33 bytes) || c (32 bytes) || s (32 bytes)` = 97 bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VrfProof {
    /// Gamma = sk * H, where H = `hash_to_curve(seed)`. Compressed SEC1 encoding.
    pub gamma: EncodedPoint,
    /// DLEQ challenge scalar.
    pub c: Scalar,
    /// DLEQ response scalar.
    pub s: Scalar,
}

/// Total encoded byte length of a [`VrfProof`].
pub const VRF_PROOF_LEN: usize = 33 + 32 + 32;

impl VrfProof {
    /// Encode the proof to a fixed-size byte array.
    pub fn to_bytes(&self) -> [u8; VRF_PROOF_LEN] {
        let mut buf = [0u8; VRF_PROOF_LEN];
        buf[..33].copy_from_slice(self.gamma.as_bytes());
        buf[33..65].copy_from_slice(&self.c.to_bytes());
        buf[65..97].copy_from_slice(&self.s.to_bytes());
        buf
    }

    /// Decode a proof from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, VrfError> {
        if bytes.len() != VRF_PROOF_LEN {
            return Err(VrfError::MalformedProof {
                reason: "incorrect proof length (expected 97 bytes)",
            });
        }

        let gamma = EncodedPoint::from_bytes(&bytes[..33])
            .map_err(|_| VrfError::MalformedProof { reason: "invalid gamma point encoding" })?;

        let c_bytes: [u8; 32] = bytes[33..65].try_into().unwrap();
        let c_opt = Scalar::from_repr(c_bytes.into());
        let c = if c_opt.is_some().into() {
            c_opt.unwrap()
        } else {
            return Err(VrfError::MalformedProof { reason: "invalid c scalar" });
        };

        let s_bytes: [u8; 32] = bytes[65..97].try_into().unwrap();
        let s_opt = Scalar::from_repr(s_bytes.into());
        let s = if s_opt.is_some().into() {
            s_opt.unwrap()
        } else {
            return Err(VrfError::MalformedProof { reason: "invalid s scalar" });
        };

        Ok(Self { gamma, c, s })
    }
}
