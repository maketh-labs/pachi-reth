//! VRF precompile dispatch (0x0101, 0x0102).
//!
//! - `VRF_COMPUTE` (0x0101): System-only. Input: `abi.encode(secret_key: bytes32, seed: bytes32)`.
//! - `VRF_VERIFY` (0x0102): Public. Input: `abi.encode(public_key: bytes33, seed: bytes32,
//!   random_value: bytes32, proof: bytes)`.

use alloy_evm::precompiles::PrecompileInput;
use alloy_primitives::{Bytes, B256};
use revm::precompile::{PrecompileOutput, PrecompileResult};

use pachi_vrf_precompile::{GAS_VRF_COMPUTE, GAS_VRF_VERIFY};

/// VRF_COMPUTE precompile (0x0101).
///
/// System-only. Takes `abi.encode(secret_key_bytes32, seed_bytes32)` and returns
/// `abi.encode(random_value_bytes32, proof_bytes)`.
pub(crate) fn vrf_compute_precompile(input: PrecompileInput<'_>) -> PrecompileResult {
    if input.gas < GAS_VRF_COMPUTE {
        return Err(revm::precompile::PrecompileError::OutOfGas);
    }

    let data = input.data;
    if data.len() < 64 {
        return Ok(PrecompileOutput::new_reverted(
            GAS_VRF_COMPUTE,
            Bytes::copy_from_slice(b"input too short: need 64 bytes"),
        ));
    }

    let Ok(secret_key): Result<[u8; 32], _> = data[..32].try_into() else {
        return Ok(PrecompileOutput::new_reverted(
            GAS_VRF_COMPUTE,
            Bytes::copy_from_slice(b"invalid secret key"),
        ));
    };
    let seed = B256::from_slice(&data[32..64]);

    match pachi_vrf_precompile::vrf_compute(input.caller, &secret_key, &seed) {
        Ok((random_value, proof)) => {
            let proof_bytes = proof.to_bytes();
            let mut output = Vec::with_capacity(32 + proof_bytes.len());
            output.extend_from_slice(random_value.as_slice());
            output.extend_from_slice(&proof_bytes);
            Ok(PrecompileOutput::new(GAS_VRF_COMPUTE, output.into()))
        }
        Err(e) => Ok(PrecompileOutput::new_reverted(
            GAS_VRF_COMPUTE,
            Bytes::copy_from_slice(e.to_string().as_bytes()),
        )),
    }
}

/// VRF_VERIFY precompile (0x0102).
///
/// Public. Takes `abi.encode(public_key_33bytes, seed_bytes32, random_value_bytes32, proof_bytes)`
/// and returns `abi.encode(valid: bool)`.
pub(crate) fn vrf_verify_precompile(input: PrecompileInput<'_>) -> PrecompileResult {
    if input.gas < GAS_VRF_VERIFY {
        return Err(revm::precompile::PrecompileError::OutOfGas);
    }

    let data = input.data;
    // Minimum: 33 (pubkey) + 32 (seed) + 32 (random_value) = 97 bytes + proof
    if data.len() < 97 {
        return Ok(PrecompileOutput::new_reverted(
            GAS_VRF_VERIFY,
            Bytes::copy_from_slice(b"input too short"),
        ));
    }

    let Ok(public_key): Result<[u8; 33], _> = data[..33].try_into() else {
        return Ok(PrecompileOutput::new_reverted(
            GAS_VRF_VERIFY,
            Bytes::copy_from_slice(b"invalid public key"),
        ));
    };
    let seed = B256::from_slice(&data[33..65]);
    let random_value = B256::from_slice(&data[65..97]);
    let proof_bytes = &data[97..];

    match pachi_vrf_precompile::vrf_verify(&public_key, &seed, &random_value, proof_bytes) {
        Ok(valid) => {
            let mut output = [0u8; 32];
            if valid {
                output[31] = 1;
            }
            Ok(PrecompileOutput::new(GAS_VRF_VERIFY, Bytes::copy_from_slice(&output)))
        }
        Err(e) => Ok(PrecompileOutput::new_reverted(
            GAS_VRF_VERIFY,
            Bytes::copy_from_slice(e.to_string().as_bytes()),
        )),
    }
}
