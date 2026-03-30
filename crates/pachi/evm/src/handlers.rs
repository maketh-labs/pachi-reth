//! Pre-execution and post-execution handlers for Pachi custom transaction types.
//!
//! These handlers implement the validation, state setup, and settlement logic that
//! occurs before and after EVM execution for `SessionTx`, `SponsoredTx`, and
//! `SessionSponsoredTx`.

pub mod post_execution;
pub mod pre_execution;

pub use post_execution::PostExecutionHandler;
pub use pre_execution::PreExecutionHandler;
