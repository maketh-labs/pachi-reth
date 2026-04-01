//! Pachi Chain payload builder.
//!
//! Extends the Ethereum payload builder with:
//! - VRF fulfill system transaction injection (after user txs with `requestVRF`)
//! - `OracleUpdate` system transaction injection (every block, always last)
//! - Custom tx type pre/post execution handling (session, sponsor)

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod builder;
mod system_tx;

pub use builder::PachiPayloadBuilder;
pub use system_tx::SystemTxGenerator;
