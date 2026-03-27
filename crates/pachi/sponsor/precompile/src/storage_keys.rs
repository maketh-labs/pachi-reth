//! Storage key computation for sponsor precompile.

use alloy_primitives::{address, keccak256, Address, U256};

/// The `SponsorHub` precompile address.
pub const SPONSOR_HUB_ADDRESS: Address = address!("0x0000000000000000000000000000000000000801");

/// Storage key for the `SponsorHub` owner.
pub(crate) fn owner_key() -> U256 {
    let hash = keccak256(b"sponsor_hub_owner");
    U256::from_be_bytes(hash.0)
}

/// Storage base key for a sponsor record.
///
/// `keccak256("sponsor_record", sponsor_address)`
pub(crate) fn sponsor_record_key(sponsor: Address) -> U256 {
    let hash = keccak256([b"sponsor_record".as_slice(), sponsor.as_slice()].concat());
    U256::from_be_bytes(hash.0)
}

/// Global limit state key for a sponsor.
///
/// `keccak256("sponsor_limit", sponsor, limit_context)`
pub(crate) fn sponsor_limit_key(sponsor: Address, limit_context: &[u8]) -> U256 {
    let hash = keccak256([b"sponsor_limit".as_slice(), sponsor.as_slice(), limit_context].concat());
    U256::from_be_bytes(hash.0)
}

/// Per-sender limit state key.
///
/// `keccak256("sponsor_sender_limit", sponsor, sender, limit_context)`
pub(crate) fn sponsor_sender_limit_key(
    sponsor: Address,
    sender: Address,
    limit_context: &[u8],
) -> U256 {
    let hash = keccak256(
        [b"sponsor_sender_limit".as_slice(), sponsor.as_slice(), sender.as_slice(), limit_context]
            .concat(),
    );
    U256::from_be_bytes(hash.0)
}
