#![cfg_attr(not(test), warn(unused_crate_dependencies))]

//! Pachi Chain custom transaction types.
//!
//! Defines four transaction types extending the EIP-2718 envelope:
//!
//! | Type | Name | Signer | Gas Payer | msg.sender |
//! |------|------|--------|-----------|------------|
//! | 0x04 | [`SessionTx`] | Session key | Authorizer | Authorizer |
//! | 0x05 | [`SponsoredTx`] | Sender EOA | Sponsor | Sender |
//! | 0x06 | [`SessionSponsoredTx`] | Session key | Sponsor | Authorizer |
//! | 0x50 | [`PachiSystemTx`] | Unsigned | Free | System addr |

mod envelope;
mod session;
mod session_sponsored;
mod sponsored;
mod system;
mod tx_type;

pub use envelope::PachiTxEnvelope;
pub use session::SessionTx;
pub use session_sponsored::SessionSponsoredTx;
pub use sponsored::SponsoredTx;
pub use system::PachiSystemTx;
pub use tx_type::{InvalidPachiTxType, InvalidSystemTxSubtype, PachiTxType, SystemTxSubtype};

#[cfg(test)]
mod tests;
