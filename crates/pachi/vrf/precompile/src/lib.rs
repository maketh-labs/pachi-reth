//! VRF precompile logic and Dealer state machine for Pachi Chain.
//!
//! # Precompiles
//!
//! - `0x0101` `VRF_COMPUTE`: System-only. Computes VRF output from secret key + seed.
//! - `0x0102` `VRF_VERIFY`: Public. Verifies a VRF proof.
//!
//! # Dealer State Machine
//!
//! Manages VRF request lifecycle: request → fulfill → query.
//! Key = `keccak256(requester || seed)`.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod dealer;
mod error;
mod gas;
mod precompile;

pub use dealer::{Dealer, VrfRequest, VrfStatus, DEALER_ADDRESS};
pub use error::VrfPrecompileError;
pub use gas::{GAS_VRF_COMPUTE, GAS_VRF_VERIFY};
pub use precompile::{vrf_compute, vrf_verify, VRF_COMPUTE_ADDRESS, VRF_VERIFY_ADDRESS};
