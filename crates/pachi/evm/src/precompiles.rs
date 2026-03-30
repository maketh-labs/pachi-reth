//! Pachi precompile registration and dispatch.
//!
//! Registers all 5 Pachi precompiles into a `PrecompilesMap` and routes ABI-encoded
//! calls to the appropriate Layer 1 state machine functions.

mod oracle;
mod session;
mod sponsor;
mod vrf;

use alloy_evm::precompiles::{DynPrecompile, PrecompilesMap};
use alloy_primitives::Address;
use revm::precompile::{PrecompileId, Precompiles};

use pachi_oracle_precompile::ORACLE_PRECOMPILE_ADDRESS;
use pachi_session_precompile::SESSION_REGISTRY_ADDRESS;
use pachi_sponsor_precompile::SPONSOR_HUB_ADDRESS;
use pachi_vrf_precompile::{VRF_COMPUTE_ADDRESS, VRF_VERIFY_ADDRESS};

/// Creates a [`PrecompilesMap`] with all Ethereum precompiles plus 5 Pachi precompiles.
pub fn pachi_precompiles() -> PrecompilesMap {
    let base = PrecompilesMap::from_static(Precompiles::prague());

    base.with_extended_precompiles(pachi_custom_precompiles())
}

/// Returns the 5 Pachi precompile entries for insertion into a `PrecompilesMap`.
fn pachi_custom_precompiles() -> Vec<(Address, DynPrecompile)> {
    vec![
        (
            VRF_COMPUTE_ADDRESS,
            DynPrecompile::new_stateful(
                PrecompileId::custom("pachi-vrf-compute"),
                vrf::vrf_compute_precompile,
            ),
        ),
        (
            VRF_VERIFY_ADDRESS,
            DynPrecompile::new_stateful(
                PrecompileId::custom("pachi-vrf-verify"),
                vrf::vrf_verify_precompile,
            ),
        ),
        (
            SESSION_REGISTRY_ADDRESS,
            DynPrecompile::new_stateful(
                PrecompileId::custom("pachi-session-registry"),
                session::session_registry_precompile,
            ),
        ),
        (
            SPONSOR_HUB_ADDRESS,
            DynPrecompile::new_stateful(
                PrecompileId::custom("pachi-sponsor-hub"),
                sponsor::sponsor_hub_precompile,
            ),
        ),
        (
            ORACLE_PRECOMPILE_ADDRESS,
            DynPrecompile::new_stateful(
                PrecompileId::custom("pachi-price-oracle"),
                oracle::oracle_precompile,
            ),
        ),
    ]
}
