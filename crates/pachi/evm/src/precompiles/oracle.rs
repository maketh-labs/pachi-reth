//! `PriceOracle` precompile dispatch (0x0802).
//!
//! Routes ABI-encoded calls to the oracle state machine. All functions are read-only.
//!
//! # Selectors (keccak256 of canonical signature)
//!
//! - `getPrice(uint8)` → `0x37f1e7f2`
//! - `getPriceBatch(uint8[])` → `0xbc348364`
//! - `isSupported(uint8)` → `0x5a803a54`

use alloy_evm::precompiles::PrecompileInput;
use alloy_primitives::{Bytes, FixedBytes, U256};
use revm::precompile::{PrecompileOutput, PrecompileResult};

use crate::state_bridge::EvmStateBridge;
use pachi_oracle_precompile::{
    gas_get_price_batch, get_price, get_price_batch, is_supported, GAS_GET_PRICE,
    GAS_GET_PRICE_BATCH_BASE, GAS_IS_SUPPORTED,
};

/// `getPrice(uint8)` → `keccak256("getPrice(uint8)")[:4]`
const SEL_GET_PRICE: FixedBytes<4> = FixedBytes::new([0x37, 0xf1, 0xe7, 0xf2]);
/// `getPriceBatch(uint8[])` → `keccak256("getPriceBatch(uint8[])")[:4]`
const SEL_GET_PRICE_BATCH: FixedBytes<4> = FixedBytes::new([0xbc, 0x34, 0x83, 0x64]);
/// `isSupported(uint8)` → `keccak256("isSupported(uint8)")[:4]`
const SEL_IS_SUPPORTED: FixedBytes<4> = FixedBytes::new([0x5a, 0x80, 0x3a, 0x54]);

/// `PriceOracle` precompile entry point.
pub(crate) fn oracle_precompile(input: PrecompileInput<'_>) -> PrecompileResult {
    let data = input.data;
    if data.len() < 4 {
        return Ok(PrecompileOutput::new_reverted(0, Bytes::copy_from_slice(b"input too short")));
    }

    let selector = FixedBytes::<4>::from_slice(&data[..4]);
    let args = &data[4..];
    let gas = input.gas;
    let bridge = EvmStateBridge::new(input.internals);
    let block_timestamp = bridge.block_timestamp();

    match selector {
        s if s == SEL_GET_PRICE => {
            if gas < GAS_GET_PRICE {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            if args.len() < 32 {
                return Ok(PrecompileOutput::new_reverted(
                    GAS_GET_PRICE,
                    Bytes::copy_from_slice(b"input too short for getPrice"),
                ));
            }
            let asset_id = args[31]; // uint8 right-aligned in 32 bytes

            match get_price(&bridge, asset_id, block_timestamp) {
                Ok((price, timestamp, confidence)) => {
                    let mut output = Vec::with_capacity(96);
                    output.extend_from_slice(&price.to_be_bytes::<32>());
                    output.extend_from_slice(&U256::from(timestamp).to_be_bytes::<32>());
                    output.extend_from_slice(&U256::from(confidence.as_u8()).to_be_bytes::<32>());
                    Ok(PrecompileOutput::new(GAS_GET_PRICE, output.into()))
                }
                Err(e) => Ok(PrecompileOutput::new_reverted(
                    GAS_GET_PRICE,
                    Bytes::copy_from_slice(e.to_string().as_bytes()),
                )),
            }
        }
        s if s == SEL_GET_PRICE_BATCH => {
            // Parse dynamic array: offset(32) + length(32) + elements
            if args.len() < 64 {
                return Ok(PrecompileOutput::new_reverted(
                    GAS_GET_PRICE_BATCH_BASE,
                    Bytes::copy_from_slice(b"input too short for getPriceBatch"),
                ));
            }
            let length = U256::from_be_slice(&args[32..64]).saturating_to::<u64>();
            let gas_cost = gas_get_price_batch(length);
            if gas < gas_cost {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }

            let mut asset_ids = Vec::with_capacity(length as usize);
            for i in 0..length as usize {
                let start = 64 + i * 32;
                if start + 32 > args.len() {
                    return Ok(PrecompileOutput::new_reverted(
                        gas_cost,
                        Bytes::copy_from_slice(b"input too short for batch elements"),
                    ));
                }
                asset_ids.push(args[start + 31]);
            }

            match get_price_batch(&bridge, &asset_ids, block_timestamp) {
                Ok(results) => {
                    // ABI encode: (uint256[] prices, uint64[] timestamps, uint8[] confidences)
                    // Three dynamic arrays with separate offset/length/data sections
                    let n = results.len();
                    // Offsets for 3 dynamic arrays (3 * 32 bytes)
                    let prices_offset = 3 * 32; // after the 3 offset words
                    let timestamps_offset = prices_offset + 32 + n * 32; // length + n words
                    let confidences_offset = timestamps_offset + 32 + n * 32;

                    let mut output = Vec::with_capacity(3 * 32 + 3 * (32 + n * 32));

                    // 3 offset words
                    output.extend_from_slice(&U256::from(prices_offset).to_be_bytes::<32>());
                    output.extend_from_slice(&U256::from(timestamps_offset).to_be_bytes::<32>());
                    output.extend_from_slice(&U256::from(confidences_offset).to_be_bytes::<32>());

                    // prices array: length + data
                    output.extend_from_slice(&U256::from(n).to_be_bytes::<32>());
                    for (price, _, _) in &results {
                        output.extend_from_slice(&price.to_be_bytes::<32>());
                    }
                    // timestamps array: length + data
                    output.extend_from_slice(&U256::from(n).to_be_bytes::<32>());
                    for (_, timestamp, _) in &results {
                        output.extend_from_slice(&U256::from(*timestamp).to_be_bytes::<32>());
                    }
                    // confidences array: length + data
                    output.extend_from_slice(&U256::from(n).to_be_bytes::<32>());
                    for (_, _, confidence) in &results {
                        output
                            .extend_from_slice(&U256::from(confidence.as_u8()).to_be_bytes::<32>());
                    }
                    Ok(PrecompileOutput::new(gas_cost, output.into()))
                }
                Err(e) => Ok(PrecompileOutput::new_reverted(
                    gas_cost,
                    Bytes::copy_from_slice(e.to_string().as_bytes()),
                )),
            }
        }
        s if s == SEL_IS_SUPPORTED => {
            if gas < GAS_IS_SUPPORTED {
                return Err(revm::precompile::PrecompileError::OutOfGas);
            }
            if args.len() < 32 {
                return Ok(PrecompileOutput::new_reverted(
                    GAS_IS_SUPPORTED,
                    Bytes::copy_from_slice(b"input too short for isSupported"),
                ));
            }
            let asset_id = args[31];
            let supported = is_supported(asset_id);
            let mut output = [0u8; 32];
            if supported {
                output[31] = 1;
            }
            Ok(PrecompileOutput::new(GAS_IS_SUPPORTED, Bytes::copy_from_slice(&output)))
        }
        _ => Ok(PrecompileOutput::new_reverted(0, Bytes::copy_from_slice(b"unknown selector"))),
    }
}
