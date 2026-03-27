//! `SessionRegistry` precompile state machine for Pachi Chain.
//!
//! Implements the session key lifecycle at address `0x0800`:
//! create, revoke, query, validate sessions with slot management and nonce tracking.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod error;
mod gas;
mod nonce;
mod record;
mod registry;
mod slots;
mod storage_keys;
mod validation;

pub use error::SessionPrecompileError;
pub use gas::*;
pub use nonce::SessionNonce;
pub use record::{SessionRecord, SessionStatus};
pub use registry::SessionRegistry;
pub use storage_keys::SESSION_REGISTRY_ADDRESS;
pub use validation::SessionValidator;
