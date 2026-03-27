//! VRF precompile pure functions.
//!
//! These wrap `pachi-vrf-core` cryptographic operations with access control
//! and ABI encoding. Wiring into the EVM happens in Layer 3.

use alloy_primitives::{address, Address, B256};
use pachi_primitives::SYSTEM_ADDRESS;
use pachi_vrf_core::{VrfProof, VRF_PROOF_LEN};

use crate::error::VrfPrecompileError;

/// `VRF_COMPUTE` precompile address (system-only).
pub const VRF_COMPUTE_ADDRESS: Address = address!("0x0000000000000000000000000000000000000101");

/// `VRF_VERIFY` precompile address (public).
pub const VRF_VERIFY_ADDRESS: Address = address!("0x0000000000000000000000000000000000000102");

/// Computes VRF output from a secret key and seed.
///
/// System-only: reverts if `caller` is not `SYSTEM_ADDRESS`.
///
/// # Arguments
/// - `caller`: The address calling the precompile.
/// - `secret_key`: 32-byte VRF secret key.
/// - `seed`: 32-byte input seed.
///
/// # Returns
/// `(random_value, proof)` on success.
pub fn vrf_compute(
    caller: Address,
    secret_key: &[u8; 32],
    seed: &B256,
) -> Result<(B256, VrfProof), VrfPrecompileError> {
    if caller != SYSTEM_ADDRESS {
        return Err(VrfPrecompileError::NotSystemCaller);
    }

    let (random_value, proof) = pachi_vrf_core::vrf_compute(secret_key, seed)?;
    Ok((random_value, proof))
}

/// Verifies a VRF proof.
///
/// Public: anyone can call this.
///
/// # Arguments
/// - `public_key`: 33-byte compressed public key.
/// - `seed`: 32-byte input seed.
/// - `random_value`: 32-byte claimed VRF output.
/// - `proof_bytes`: Encoded proof bytes.
///
/// # Returns
/// `true` if the proof is valid.
pub fn vrf_verify(
    public_key: &[u8; 33],
    seed: &B256,
    random_value: &B256,
    proof_bytes: &[u8],
) -> Result<bool, VrfPrecompileError> {
    if proof_bytes.len() != VRF_PROOF_LEN {
        return Ok(false);
    }

    let proof = match VrfProof::from_bytes(proof_bytes) {
        Ok(p) => p,
        Err(_) => return Ok(false),
    };

    match pachi_vrf_core::vrf_verify(public_key, seed, random_value, &proof) {
        Ok(valid) => Ok(valid),
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pachi_vrf_core::generate_keypair;

    #[test]
    fn compute_requires_system_caller() {
        let (sk, _) = generate_keypair();
        let seed = B256::from([0xABu8; 32]);
        let err = vrf_compute(Address::from([1u8; 20]), &sk, &seed).unwrap_err();
        assert!(matches!(err, VrfPrecompileError::NotSystemCaller));
    }

    #[test]
    fn compute_and_verify_roundtrip() {
        let (sk, pk) = generate_keypair();
        let seed = B256::from([0xCDu8; 32]);

        let (random_value, proof) = vrf_compute(SYSTEM_ADDRESS, &sk, &seed).unwrap();
        let proof_bytes = proof.to_bytes();

        let valid = vrf_verify(&pk, &seed, &random_value, &proof_bytes).unwrap();
        assert!(valid);
    }

    #[test]
    fn verify_rejects_wrong_random_value() {
        let (sk, pk) = generate_keypair();
        let seed = B256::from([0xEFu8; 32]);

        let (_, proof) = vrf_compute(SYSTEM_ADDRESS, &sk, &seed).unwrap();
        let proof_bytes = proof.to_bytes();

        let wrong_value = B256::from([0x00u8; 32]);
        let valid = vrf_verify(&pk, &seed, &wrong_value, &proof_bytes).unwrap();
        assert!(!valid);
    }

    #[test]
    fn verify_rejects_invalid_proof_length() {
        let (_, pk) = generate_keypair();
        let seed = B256::from([0x11u8; 32]);
        let random_value = B256::from([0x22u8; 32]);

        let valid = vrf_verify(&pk, &seed, &random_value, &[0u8; 10]).unwrap();
        assert!(!valid);
    }

    #[test]
    fn verify_rejects_malformed_proof() {
        let (_, pk) = generate_keypair();
        let seed = B256::from([0x33u8; 32]);
        let random_value = B256::from([0x44u8; 32]);

        let valid = vrf_verify(&pk, &seed, &random_value, &[0u8; VRF_PROOF_LEN]).unwrap();
        assert!(!valid);
    }
}
