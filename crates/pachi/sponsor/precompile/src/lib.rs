//! `SponsorHub` precompile state machine for Pachi Chain.
//!
//! Implements dual-mode gas sponsorship at address `0x0801`:
//! - **Deposit mode**: Sponsor pre-deposits funds; gas is deducted from deposit.
//! - **Mint mode**: Protocol mints native token for gas; governance-approved only.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod error;
mod gas;
mod governance;
mod hub;
mod record;
mod settlement;
pub mod storage_keys;
mod validation;

pub use error::SponsorPrecompileError;
pub use gas::*;
pub use governance::SponsorGovernance;
pub use hub::SponsorHub;
pub use record::{
    SponsorCallPolicy, SponsorConfig, SponsorRecord, SponsorStatus, SponsorTransferPolicy,
    SponsorType,
};
pub use settlement::{SettlementAction, SponsorSettlement};
pub use storage_keys::SPONSOR_HUB_ADDRESS;
pub use validation::{SponsorValidationInput, SponsorValidator};
