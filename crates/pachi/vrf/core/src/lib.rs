//! Pure ECVRF cryptography for Pachi Chain (secp256k1).
//!
//! Implements ECVRF-SECP256K1-SHA256-TAI: a verifiable random function
//! using hash-to-try-and-increment for hash-to-curve.
//!
//! # Functions
//!
//! - [`vrf_compute`]: Compute VRF output and proof from a secret key and seed.
//! - [`vrf_verify`]: Verify a VRF output given a public key, seed, output, and proof.
//! - [`generate_keypair`]: Generate a new VRF keypair.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod ecvrf;
mod error;
mod proof;

pub use ecvrf::{generate_keypair, vrf_compute, vrf_verify};
pub use error::VrfError;
pub use proof::{VrfProof, VRF_PROOF_LEN};
