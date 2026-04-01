//! Oracle Engine for Pachi Chain.
//!
//! Collects real-time price data from 5 CEX `WebSocket` feeds, aggregates via
//! deterministic median algorithm, and produces [`PriceSnapshot`]s for block building.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐
//! │ Binance │ │Coinbase │ │   OKX   │ │  Bybit  │ │ Kraken  │
//! └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘
//!      └─────┬─────┴─────┬─────┴─────┬─────┘           │
//!            v           v           v                  v
//!         ┌──────────────────────────────────────────────┐
//!         │              Aggregator                      │
//!         │  (staleness filter → outlier trim → median)  │
//!         └──────────────────┬───────────────────────────┘
//!                            v
//!                   Local Price State
//!                            │
//!                   generate_snapshot()
//! ```

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod aggregator;
mod engine;
mod source;

pub use aggregator::Aggregator;
pub use engine::OracleEngine;
pub use source::{PriceUpdate, Source, SourceConnector, SourceId, SourceMessage};

#[cfg(test)]
mod tests;
