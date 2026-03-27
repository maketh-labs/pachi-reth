//! ECVRF-SECP256K1-SHA256-TAI implementation.
//!
//! Construction:
//! 1. H = `hash_to_curve(pk, seed)` — try-and-increment
//! 2. gamma = sk * H
//! 3. `random_value` = SHA256(`suite_string` || `gamma_compressed`)
//! 4. proof = DLEQ(sk, H, gamma) — proves gamma = sk * H without revealing sk

use alloy_primitives::B256;
use k256::{
    elliptic_curve::{
        sec1::{FromEncodedPoint, ToEncodedPoint},
        PrimeField,
    },
    AffinePoint, EncodedPoint, ProjectivePoint, Scalar, SecretKey,
};
use sha2::{Digest, Sha256};

use crate::{VrfError, VrfProof};

/// Suite string prefix for domain separation.
const SUITE_STRING: &[u8] = b"ECVRF-SECP256K1-SHA256-TAI-PACHI";

/// Max attempts for hash-to-try-and-increment.
const MAX_HASH_ATTEMPTS: u32 = 256;

/// Generate a new VRF keypair.
///
/// Returns `(secret_key_bytes, public_key_compressed_bytes)`.
pub fn generate_keypair() -> ([u8; 32], [u8; 33]) {
    let sk = SecretKey::random(&mut rand_08::rngs::OsRng);
    let pk = sk.public_key();
    let sk_bytes: [u8; 32] = sk.to_bytes().into();
    let pk_point = pk.to_encoded_point(true);
    let mut pk_bytes = [0u8; 33];
    pk_bytes.copy_from_slice(pk_point.as_bytes());
    (sk_bytes, pk_bytes)
}

/// Compute a VRF output and proof.
///
/// # Arguments
/// - `secret_key`: 32-byte secret key.
/// - `seed`: 32-byte seed (typically `keccak256(msg.sender || user_seed)`).
///
/// # Returns
/// `(random_value, proof)` where `random_value` is a deterministic 32-byte hash.
pub fn vrf_compute(secret_key: &[u8; 32], seed: &B256) -> Result<(B256, VrfProof), VrfError> {
    // Decode secret key
    let sk = SecretKey::from_bytes(secret_key.into()).map_err(|_| VrfError::InvalidSecretKey)?;
    let sk_nz = sk.to_nonzero_scalar();
    let sk_scalar: Scalar = *sk_nz.as_ref();

    // Derive public key
    let pk_point = ProjectivePoint::GENERATOR * sk_scalar;
    let pk_affine = pk_point.to_affine();
    let pk_encoded = pk_affine.to_encoded_point(true);

    // Step 1: hash to curve
    let h = hash_to_curve(&pk_encoded, seed)?;

    // Step 2: gamma = sk * H
    let gamma = h * sk_scalar;
    let gamma_affine = gamma.to_affine();
    let gamma_encoded = gamma_affine.to_encoded_point(true);

    // Step 3: DLEQ proof
    let (c, s) = dleq_prove(&sk_scalar, &h, &gamma, &pk_point)?;

    // Step 4: random_value = SHA256(suite_string || 0x03 || gamma_compressed)
    let random_value = proof_to_hash(&gamma_encoded);

    let proof = VrfProof { gamma: gamma_encoded, c, s };

    Ok((random_value, proof))
}

/// Verify a VRF output against a public key, seed, and proof.
///
/// # Arguments
/// - `public_key`: 33-byte compressed public key.
/// - `seed`: 32-byte seed.
/// - `random_value`: 32-byte claimed VRF output.
/// - `proof`: The VRF proof.
pub fn vrf_verify(
    public_key: &[u8; 33],
    seed: &B256,
    random_value: &B256,
    proof: &VrfProof,
) -> Result<bool, VrfError> {
    // Decode public key
    let pk_encoded =
        EncodedPoint::from_bytes(public_key).map_err(|_| VrfError::InvalidPublicKey)?;
    let pk_affine_opt = AffinePoint::from_encoded_point(&pk_encoded);
    let pk_affine = if pk_affine_opt.is_some().into() {
        pk_affine_opt.unwrap()
    } else {
        return Err(VrfError::InvalidPublicKey);
    };
    let pk_proj = ProjectivePoint::from(pk_affine);

    // Decode gamma from proof
    let gamma_affine_opt = AffinePoint::from_encoded_point(&proof.gamma);
    let gamma_affine = if gamma_affine_opt.is_some().into() {
        gamma_affine_opt.unwrap()
    } else {
        return Err(VrfError::MalformedProof { reason: "gamma not on curve" });
    };
    let gamma_proj = ProjectivePoint::from(gamma_affine);

    // Step 1: hash to curve
    let h = hash_to_curve(&pk_encoded, seed)?;

    // Step 2: DLEQ verify
    let valid = dleq_verify(&pk_proj, &h, &gamma_proj, &proof.c, &proof.s);
    if !valid {
        return Ok(false);
    }

    // Step 3: check random_value
    let expected = proof_to_hash(&proof.gamma);
    Ok(expected == *random_value)
}

/// Hash-to-try-and-increment: maps (pk, seed) to a curve point.
///
/// For each counter `0..MAX_HASH_ATTEMPTS`:
///   candidate = SHA256(suite || 0x01 || `pk_compressed` || seed || counter)
///   Try to decompress as a secp256k1 point (with 0x02 prefix).
fn hash_to_curve(pk_encoded: &EncodedPoint, seed: &B256) -> Result<ProjectivePoint, VrfError> {
    for ctr in 0..MAX_HASH_ATTEMPTS {
        let mut hasher = Sha256::new();
        hasher.update(SUITE_STRING);
        hasher.update([0x01]); // hash_to_curve tag
        hasher.update(pk_encoded.as_bytes());
        hasher.update(seed.as_slice());
        hasher.update(ctr.to_be_bytes());
        let hash = hasher.finalize();

        // Attempt to construct a compressed point: 0x02 || hash
        let mut compressed = [0u8; 33];
        compressed[0] = 0x02; // even y-coordinate
        compressed[1..33].copy_from_slice(&hash);

        if let Ok(point_encoded) = EncodedPoint::from_bytes(compressed) {
            let affine_opt = AffinePoint::from_encoded_point(&point_encoded);
            if affine_opt.is_some().into() {
                let affine: AffinePoint = affine_opt.unwrap();
                return Ok(ProjectivePoint::from(affine));
            }
        }
    }

    Err(VrfError::HashToCurveFailed { max_attempts: MAX_HASH_ATTEMPTS })
}

/// Derive the VRF random value from gamma.
///
/// `random_value = SHA256(suite_string || 0x03 || gamma_compressed)`
fn proof_to_hash(gamma_encoded: &EncodedPoint) -> B256 {
    let mut hasher = Sha256::new();
    hasher.update(SUITE_STRING);
    hasher.update([0x03]); // proof_to_hash tag
    hasher.update(gamma_encoded.as_bytes());
    let hash = hasher.finalize();
    B256::from_slice(&hash)
}

/// DLEQ proof generation: proves `log_G(pk) == log_H(gamma)`.
///
/// Schnorr-like proof:
///   k = random nonce
///   U = k * G
///   V = k * H
///   c = SHA256(suite || 0x02 || G || H || pk || gamma || U || V) (truncated to scalar)
///   s = k - c * sk  (mod n)
fn dleq_prove(
    sk_scalar: &Scalar,
    h: &ProjectivePoint,
    gamma: &ProjectivePoint,
    pk: &ProjectivePoint,
) -> Result<(Scalar, Scalar), VrfError> {
    // Deterministic nonce: k = SHA256(sk || H_compressed) reduced mod n
    // This makes VRF fully deterministic (no randomness needed at prove time).
    let h_bytes = h.to_affine().to_encoded_point(true);
    let k = deterministic_nonce(sk_scalar, h_bytes.as_bytes());

    let u = ProjectivePoint::GENERATOR * k; // k * G
    let v = *h * k; // k * H

    let c = dleq_challenge(h, pk, gamma, &u, &v);
    let s = k - c * sk_scalar; // s = k - c * sk

    Ok((c, s))
}

/// DLEQ verification: check that c == SHA256(...) for the recomputed U, V.
///
///   U' = s * G + c * pk
///   V' = s * H + c * gamma
///   c' = hash(...)
///   valid iff c' == c
fn dleq_verify(
    pk: &ProjectivePoint,
    h: &ProjectivePoint,
    gamma: &ProjectivePoint,
    c: &Scalar,
    s: &Scalar,
) -> bool {
    let u = ProjectivePoint::GENERATOR * s + pk * c; // s*G + c*pk
    let v = *h * s + gamma * c; // s*H + c*gamma

    let c_prime = dleq_challenge(h, pk, gamma, &u, &v);
    *c == c_prime
}

/// Compute the DLEQ challenge scalar.
///
/// `c = SHA256(suite || 0x02 || H || pk || gamma || U || V)` reduced mod n.
fn dleq_challenge(
    h: &ProjectivePoint,
    pk: &ProjectivePoint,
    gamma: &ProjectivePoint,
    u: &ProjectivePoint,
    v: &ProjectivePoint,
) -> Scalar {
    let encode = |p: &ProjectivePoint| -> Vec<u8> {
        p.to_affine().to_encoded_point(true).as_bytes().to_vec()
    };

    let mut hasher = Sha256::new();
    hasher.update(SUITE_STRING);
    hasher.update([0x02]); // dleq_challenge tag
    hasher.update(encode(h));
    hasher.update(encode(pk));
    hasher.update(encode(gamma));
    hasher.update(encode(u));
    hasher.update(encode(v));
    let hash = hasher.finalize();

    // Reduce the 256-bit hash modulo the curve order to get a valid scalar.
    let bytes: [u8; 32] = hash.into();
    scalar_from_bytes_reduced(&bytes)
}

/// Deterministic nonce derivation for DLEQ proof (RFC 6979-style).
///
/// `k = SHA256(sk_bytes || h_bytes)` reduced mod n.
fn deterministic_nonce(sk: &Scalar, h_bytes: &[u8]) -> Scalar {
    let mut hasher = Sha256::new();
    hasher.update(sk.to_bytes());
    hasher.update(h_bytes);
    let hash = hasher.finalize();
    let bytes: [u8; 32] = hash.into();
    scalar_from_bytes_reduced(&bytes)
}

/// Reduce a 32-byte big-endian value modulo the secp256k1 curve order.
fn scalar_from_bytes_reduced(bytes: &[u8; 32]) -> Scalar {
    // Use from_repr which checks if valid. If >= n, we reduce manually.
    // For a SHA256 output, the probability of being >= n is negligible (~2^-128),
    // but we handle it correctly.
    let repr = k256::FieldBytes::from_slice(bytes);
    let opt = Scalar::from_repr(*repr);
    if opt.is_some().into() {
        opt.unwrap()
    } else {
        // Value >= curve order. Use reduce to bring it in range.
        // reduce_nonzero from WideBytes is the correct approach, but for a 32-byte
        // value that's just barely over n, we can subtract n.
        // Simpler: use from_uint_reduced which handles this.
        <Scalar as k256::elliptic_curve::ops::Reduce<k256::U256>>::reduce(
            k256::U256::from_be_slice(bytes),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::B256;

    fn test_keypair() -> ([u8; 32], [u8; 33]) {
        // Deterministic test key
        let sk_bytes: [u8; 32] = Sha256::digest(b"pachi-vrf-test-key").into();
        let sk = SecretKey::from_bytes((&sk_bytes).into()).unwrap();
        let pk = sk.public_key();
        let pk_encoded = pk.to_encoded_point(true);
        let mut pk_bytes = [0u8; 33];
        pk_bytes.copy_from_slice(pk_encoded.as_bytes());
        (sk_bytes, pk_bytes)
    }

    #[test]
    fn compute_verify_roundtrip() {
        let (sk, pk) = test_keypair();
        let seed = B256::from([0xABu8; 32]);

        let (random_value, proof) = vrf_compute(&sk, &seed).unwrap();

        let valid = vrf_verify(&pk, &seed, &random_value, &proof).unwrap();
        assert!(valid, "VRF verify must accept a valid compute result");
    }

    #[test]
    fn deterministic_output() {
        let (sk, _) = test_keypair();
        let seed = B256::from([0x42u8; 32]);

        let (rv1, proof1) = vrf_compute(&sk, &seed).unwrap();
        let (rv2, proof2) = vrf_compute(&sk, &seed).unwrap();

        assert_eq!(rv1, rv2, "VRF must be deterministic");
        assert_eq!(proof1, proof2, "proof must be deterministic");
    }

    #[test]
    fn different_seeds_different_outputs() {
        let (sk, _) = test_keypair();
        let seed1 = B256::from([0x01u8; 32]);
        let seed2 = B256::from([0x02u8; 32]);

        let (rv1, _) = vrf_compute(&sk, &seed1).unwrap();
        let (rv2, _) = vrf_compute(&sk, &seed2).unwrap();

        assert_ne!(rv1, rv2, "different seeds must produce different outputs");
    }

    #[test]
    fn different_keys_different_outputs() {
        let seed = B256::from([0xFFu8; 32]);

        let sk1: [u8; 32] = Sha256::digest(b"key-1").into();
        let sk2: [u8; 32] = Sha256::digest(b"key-2").into();

        let (rv1, _) = vrf_compute(&sk1, &seed).unwrap();
        let (rv2, _) = vrf_compute(&sk2, &seed).unwrap();

        assert_ne!(rv1, rv2, "different keys must produce different outputs");
    }

    #[test]
    fn invalid_proof_rejected() {
        let (sk, pk) = test_keypair();
        let seed = B256::from([0x99u8; 32]);

        let (random_value, mut proof) = vrf_compute(&sk, &seed).unwrap();

        // Tamper with the proof's s scalar
        proof.s = proof.s + Scalar::ONE;

        let valid = vrf_verify(&pk, &seed, &random_value, &proof).unwrap();
        assert!(!valid, "tampered proof must be rejected");
    }

    #[test]
    fn wrong_random_value_rejected() {
        let (sk, pk) = test_keypair();
        let seed = B256::from([0x77u8; 32]);

        let (_random_value, proof) = vrf_compute(&sk, &seed).unwrap();
        let wrong_value = B256::from([0x00u8; 32]);

        let valid = vrf_verify(&pk, &seed, &wrong_value, &proof).unwrap();
        assert!(!valid, "wrong random value must be rejected");
    }

    #[test]
    fn wrong_seed_rejected() {
        let (sk, pk) = test_keypair();
        let seed = B256::from([0x55u8; 32]);
        let wrong_seed = B256::from([0x56u8; 32]);

        let (random_value, proof) = vrf_compute(&sk, &seed).unwrap();

        let valid = vrf_verify(&pk, &wrong_seed, &random_value, &proof).unwrap();
        assert!(!valid, "wrong seed must be rejected");
    }

    #[test]
    fn wrong_public_key_rejected() {
        let (sk, _pk) = test_keypair();
        let seed = B256::from([0x33u8; 32]);

        let (random_value, proof) = vrf_compute(&sk, &seed).unwrap();

        // Use a different public key
        let other_sk: [u8; 32] = Sha256::digest(b"other-key").into();
        let other_sk_obj = SecretKey::from_bytes((&other_sk).into()).unwrap();
        let other_pk = other_sk_obj.public_key().to_encoded_point(true);
        let mut other_pk_bytes = [0u8; 33];
        other_pk_bytes.copy_from_slice(other_pk.as_bytes());

        let valid = vrf_verify(&other_pk_bytes, &seed, &random_value, &proof).unwrap();
        assert!(!valid, "wrong public key must be rejected");
    }

    #[test]
    fn proof_serialization_roundtrip() {
        let (sk, _) = test_keypair();
        let seed = B256::from([0x11u8; 32]);

        let (_, proof) = vrf_compute(&sk, &seed).unwrap();

        let bytes = proof.to_bytes();
        assert_eq!(bytes.len(), 97);

        let decoded = VrfProof::from_bytes(&bytes).unwrap();
        assert_eq!(proof, decoded);
    }

    #[test]
    fn malformed_proof_rejected() {
        assert!(VrfProof::from_bytes(&[0u8; 96]).is_err()); // too short
        assert!(VrfProof::from_bytes(&[0u8; 98]).is_err()); // too long
        assert!(VrfProof::from_bytes(&[0u8; 97]).is_err()); // invalid point
    }

    #[test]
    fn invalid_secret_key_rejected() {
        // All zeros is not a valid secret key
        let bad_sk = [0u8; 32];
        let seed = B256::from([0x01u8; 32]);
        assert!(vrf_compute(&bad_sk, &seed).is_err());
    }

    #[test]
    fn invalid_public_key_rejected() {
        let bad_pk = [0u8; 33];
        let seed = B256::ZERO;
        let rv = B256::ZERO;
        let (sk, _) = test_keypair();
        let (_, proof) = vrf_compute(&sk, &seed).unwrap();
        assert!(vrf_verify(&bad_pk, &seed, &rv, &proof).is_err());
    }

    #[test]
    fn generate_keypair_works() {
        let (sk, pk) = generate_keypair();
        // Verify key is valid by doing a compute+verify
        let seed = B256::from([0xCCu8; 32]);
        let (rv, proof) = vrf_compute(&sk, &seed).unwrap();
        let valid = vrf_verify(&pk, &seed, &rv, &proof).unwrap();
        assert!(valid);
    }
}
