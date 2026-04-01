//! Oracle Engine: coordinates `WebSocket` sources, aggregation, and snapshot generation.

use pachi_primitives::{
    AssetId, AssetState, Confidence, PriceSnapshot, SnapshotEntry, CURRENT_ENGINE_VERSION,
    HEARTBEAT_INTERVAL,
};
use parking_lot::RwLock;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc;

use crate::{
    source::{Source, SourceConnector, SourceId, SourceMessage},
    Aggregator,
};

/// Shared state holding the latest aggregated prices.
#[derive(Debug, Default)]
struct LocalPriceState {
    /// Per-asset latest aggregated state.
    states: HashMap<AssetId, AssetState>,
    /// Per-source connection status.
    source_status: HashMap<SourceId, bool>,
}

/// The Oracle Engine subsystem.
///
/// Runs `WebSocket` connectors for all 5 CEXs, aggregates prices on a 1-second heartbeat,
/// and provides [`generate_snapshot`] for block building.
pub struct OracleEngine {
    state: Arc<RwLock<LocalPriceState>>,
    aggregator: Arc<RwLock<Aggregator>>,
}

impl std::fmt::Debug for OracleEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OracleEngine").finish()
    }
}

impl Default for OracleEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl OracleEngine {
    /// Creates a new Oracle Engine.
    pub fn new() -> Self {
        let mut initial_states = HashMap::new();
        for asset in AssetId::ALL {
            initial_states.insert(asset, AssetState::Unavailable);
        }
        let mut source_status = HashMap::new();
        for source in SourceId::ALL {
            source_status.insert(source, false);
        }
        Self {
            state: Arc::new(RwLock::new(LocalPriceState { states: initial_states, source_status })),
            aggregator: Arc::new(RwLock::new(Aggregator::new())),
        }
    }

    /// Starts the engine. Spawns `WebSocket` connectors and the heartbeat aggregation loop.
    ///
    /// This returns immediately; the engine runs in background tokio tasks.
    pub fn start(&self) {
        let (tx, rx) = mpsc::channel::<SourceMessage>(1024);

        // Spawn source connectors
        for source_id in SourceId::ALL {
            let source = Source::new(source_id);
            let connector = SourceConnector::new(source, tx.clone());
            tokio::spawn(connector.run());
        }

        // Spawn message processor
        let aggregator = self.aggregator.clone();
        let state = self.state.clone();
        tokio::spawn(Self::process_messages(rx, aggregator.clone(), state.clone()));

        // Spawn heartbeat aggregation
        tokio::spawn(Self::heartbeat_loop(aggregator, state));
    }

    /// Generates a [`PriceSnapshot`] from the current local price state.
    ///
    /// Called by the block builder when assembling a new block.
    pub fn generate_snapshot(&self) -> PriceSnapshot {
        let state = self.state.read();
        let entries = AssetId::ALL
            .into_iter()
            .map(|asset| {
                let entry = match state.states.get(&asset) {
                    Some(AssetState::Available { price, timestamp, confidence }) => {
                        SnapshotEntry::Updated {
                            price: *price,
                            timestamp: *timestamp,
                            confidence: *confidence,
                        }
                    }
                    Some(AssetState::Unavailable) | None => SnapshotEntry::Unavailable,
                };
                (asset, entry)
            })
            .collect();

        PriceSnapshot { entries, engine_version: CURRENT_ENGINE_VERSION }
    }

    /// Returns the current connection status of each source.
    pub fn source_status(&self) -> HashMap<SourceId, bool> {
        self.state.read().source_status.clone()
    }

    /// Returns the number of currently connected sources.
    pub fn connected_source_count(&self) -> usize {
        self.state.read().source_status.values().filter(|&&v| v).count()
    }

    /// Returns the current aggregated state for a single asset.
    pub fn get_asset_state(&self, asset: AssetId) -> AssetState {
        self.state.read().states.get(&asset).cloned().unwrap_or(AssetState::Unavailable)
    }

    /// Manually injects a price update (useful for testing and mock sources).
    pub fn inject_update(&self, update: &crate::PriceUpdate) {
        self.aggregator.write().update(update);
    }

    /// Manually triggers aggregation (useful for testing).
    pub fn trigger_aggregation(&self) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        self.run_aggregation(now);
    }

    /// Manually triggers aggregation at a given timestamp (for deterministic tests).
    pub fn trigger_aggregation_at(&self, now: u64) {
        self.run_aggregation(now);
    }

    fn run_aggregation(&self, now: u64) {
        let all_states = self.aggregator.read().aggregate_all(now);
        let mut state = self.state.write();
        apply_aggregated_states(&mut state.states, all_states);
    }

    /// Processes incoming messages from source connectors.
    async fn process_messages(
        mut rx: mpsc::Receiver<SourceMessage>,
        aggregator: Arc<RwLock<Aggregator>>,
        state: Arc<RwLock<LocalPriceState>>,
    ) {
        while let Some(msg) = rx.recv().await {
            match msg {
                SourceMessage::Price(update) => {
                    tracing::trace!(
                        target: "pachi::oracle",
                        source = %update.source,
                        asset = ?update.asset,
                        price = %update.price,
                        "price update"
                    );
                    aggregator.write().update(&update);
                }
                SourceMessage::Disconnected(source) => {
                    tracing::warn!(
                        target: "pachi::oracle",
                        source = %source,
                        "source disconnected"
                    );
                    state.write().source_status.insert(source, false);
                    aggregator.write().clear_source(source as u8);
                }
                SourceMessage::Connected(source) => {
                    tracing::info!(
                        target: "pachi::oracle",
                        source = %source,
                        "source connected"
                    );
                    state.write().source_status.insert(source, true);
                }
            }
        }
    }

    /// Runs the heartbeat aggregation loop (1-second interval).
    async fn heartbeat_loop(
        aggregator: Arc<RwLock<Aggregator>>,
        state: Arc<RwLock<LocalPriceState>>,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL));

        loop {
            interval.tick().await;
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

            let all_states = aggregator.read().aggregate_all(now);
            let mut local = state.write();
            apply_aggregated_states(&mut local.states, all_states);
        }
    }
}

/// Merges aggregated states into the local state map.
/// When new data is Unavailable but previous price exists, keeps price with Unavailable confidence.
fn apply_aggregated_states(
    states: &mut HashMap<AssetId, AssetState>,
    new_states: HashMap<AssetId, AssetState>,
) {
    for (asset, new_state) in new_states {
        let merged = match (&new_state, states.get(&asset)) {
            (AssetState::Unavailable, Some(AssetState::Available { price, timestamp, .. })) => {
                AssetState::Available {
                    price: *price,
                    timestamp: *timestamp,
                    confidence: Confidence::Unavailable,
                }
            }
            _ => new_state,
        };
        states.insert(asset, merged);
    }
}
