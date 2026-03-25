//! Pachi Chain hardfork definitions.
//!
//! Defines `PachiHardfork` enum with Phase1 and Phase2 variants,
//! along with genesis configuration extensions for Pachi-specific parameters.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod config;
mod hardfork;

pub use config::{
    OracleGenesisConfig, PachiGenesisConfig, SessionGenesisConfig, SponsorGenesisConfig,
};
pub use hardfork::PachiHardfork;
