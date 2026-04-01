//! Deterministic price aggregation algorithm.
//!
//! Steps: staleness filter → active source count check → outlier trim → median → confidence.

use alloy_primitives::U256;
use pachi_primitives::{
    AssetId, AssetState, Confidence, MIN_ACTIVE_SOURCES, SPREAD_HIGH_BPS, SPREAD_MEDIUM_BPS,
    STALENESS_THRESHOLD,
};
use std::collections::HashMap;

use crate::PriceUpdate;

/// Per-asset, per-source latest price data.
#[derive(Debug, Clone)]
struct SourcePrice {
    price: U256,
    timestamp: u64,
}

/// Aggregates prices from multiple sources into a single deterministic result per asset.
#[derive(Debug)]
pub struct Aggregator {
    /// Latest price per (asset, source). Key = (`asset_id`, `source_id` as u8).
    latest: HashMap<(AssetId, u8), SourcePrice>,
}

impl Default for Aggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl Aggregator {
    /// Creates a new empty aggregator.
    pub fn new() -> Self {
        Self { latest: HashMap::new() }
    }

    /// Ingests a price update from a source.
    pub fn update(&mut self, update: &PriceUpdate) {
        let key = (update.asset, update.source as u8);
        // Only accept newer or equal timestamps
        if let Some(existing) = self.latest.get(&key) &&
            update.timestamp < existing.timestamp
        {
            return;
        }
        self.latest.insert(key, SourcePrice { price: update.price, timestamp: update.timestamp });
    }

    /// Aggregates current data for a single asset.
    ///
    /// This is the core deterministic aggregation algorithm from the spec:
    /// 1. Staleness filter
    /// 2. Active source count check
    /// 3. Outlier trim (if 5+ sources)
    /// 4. Median
    /// 5. Spread-based confidence
    pub fn aggregate(&self, asset: AssetId, now: u64) -> AssetState {
        // Collect fresh prices for this asset from all sources
        let fresh: Vec<U256> = self
            .latest
            .iter()
            .filter(|((a, _), _)| *a == asset)
            .filter(|(_, sp)| now.saturating_sub(sp.timestamp) <= STALENESS_THRESHOLD)
            .map(|(_, sp)| sp.price)
            .collect();

        if fresh.is_empty() {
            return AssetState::Unavailable;
        }

        let mut prices = fresh;
        prices.sort();

        // Find the max timestamp among fresh sources for this asset
        let max_ts = self
            .latest
            .iter()
            .filter(|((a, _), _)| *a == asset)
            .filter(|(_, sp)| now.saturating_sub(sp.timestamp) <= STALENESS_THRESHOLD)
            .map(|(_, sp)| sp.timestamp)
            .max()
            .unwrap_or(now);

        if prices.len() < MIN_ACTIVE_SOURCES {
            // Insufficient sources → compute but mark Degraded
            let median = prices[prices.len() / 2];
            return AssetState::Available {
                price: median,
                timestamp: max_ts,
                confidence: Confidence::Degraded,
            };
        }

        // Outlier trim: remove highest and lowest if 5+ sources
        if prices.len() >= 5 {
            prices = prices[1..prices.len() - 1].to_vec();
        }

        // Median
        let median = prices[prices.len() / 2];

        // Spread-based confidence
        let lo = prices[0];
        let hi = prices[prices.len() - 1];
        let confidence = if median.is_zero() {
            Confidence::Degraded
        } else {
            let spread_bps = (hi - lo) * U256::from(10000) / median;
            if spread_bps <= U256::from(SPREAD_HIGH_BPS) {
                Confidence::High
            } else if spread_bps <= U256::from(SPREAD_MEDIUM_BPS) {
                Confidence::Medium
            } else {
                Confidence::Degraded
            }
        };

        AssetState::Available { price: median, timestamp: max_ts, confidence }
    }

    /// Aggregates all supported assets at the given timestamp.
    pub fn aggregate_all(&self, now: u64) -> HashMap<AssetId, AssetState> {
        AssetId::ALL.into_iter().map(|asset| (asset, self.aggregate(asset, now))).collect()
    }

    /// Clears all data for a given source (e.g., on disconnect).
    pub fn clear_source(&mut self, source_id: u8) {
        self.latest.retain(|(_, s), _| *s != source_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    fn make_update(source: SourceId, asset: AssetId, price: u64, timestamp: u64) -> PriceUpdate {
        PriceUpdate { source, asset, price: U256::from(price), timestamp }
    }

    #[test]
    fn no_data_returns_unavailable() {
        let agg = Aggregator::new();
        assert_eq!(agg.aggregate(AssetId::BTC, 100), AssetState::Unavailable);
    }

    #[test]
    fn stale_data_filtered() {
        let mut agg = Aggregator::new();
        // Data at t=80, queried at t=100. STALENESS_THRESHOLD=10, so 100-80=20 > 10 → stale.
        agg.update(&make_update(SourceId::Binance, AssetId::BTC, 5_000_000_000_000, 80));
        assert_eq!(agg.aggregate(AssetId::BTC, 100), AssetState::Unavailable);
    }

    #[test]
    fn single_source_degraded() {
        let mut agg = Aggregator::new();
        agg.update(&make_update(SourceId::Binance, AssetId::BTC, 5_000_000_000_000, 99));
        let result = agg.aggregate(AssetId::BTC, 100);
        match result {
            AssetState::Available { price, confidence, .. } => {
                assert_eq!(price, U256::from(5_000_000_000_000u64));
                assert_eq!(confidence, Confidence::Degraded);
            }
            _ => panic!("expected Available"),
        }
    }

    #[test]
    fn two_sources_degraded() {
        let mut agg = Aggregator::new();
        agg.update(&make_update(SourceId::Binance, AssetId::BTC, 5_000_000_000_000, 99));
        agg.update(&make_update(SourceId::Coinbase, AssetId::BTC, 5_000_100_000_000, 99));
        let result = agg.aggregate(AssetId::BTC, 100);
        match result {
            AssetState::Available { confidence, .. } => {
                assert_eq!(confidence, Confidence::Degraded);
            }
            _ => panic!("expected Available"),
        }
    }

    #[test]
    fn three_sources_high_confidence() {
        let mut agg = Aggregator::new();
        // All very close prices → spread < 10bps
        let base = 5_000_000_000_000u64; // $50,000
        agg.update(&make_update(SourceId::Binance, AssetId::BTC, base, 99));
        agg.update(&make_update(SourceId::Coinbase, AssetId::BTC, base + 100_000, 99)); // +$0.001
        agg.update(&make_update(SourceId::Okx, AssetId::BTC, base + 200_000, 99)); // +$0.002
        let result = agg.aggregate(AssetId::BTC, 100);
        match result {
            AssetState::Available { price, confidence, .. } => {
                // Median of sorted [base, base+100000, base+200000] = base+100000
                assert_eq!(price, U256::from(base + 100_000));
                assert_eq!(confidence, Confidence::High);
            }
            _ => panic!("expected Available"),
        }
    }

    #[test]
    fn five_sources_outlier_trimmed() {
        let mut agg = Aggregator::new();
        let base = 5_000_000_000_000u64; // $50,000
                                         // One low outlier, one high outlier
        agg.update(&make_update(SourceId::Binance, AssetId::BTC, base - 500_000_000_000, 99)); // outlier low
        agg.update(&make_update(SourceId::Coinbase, AssetId::BTC, base, 99));
        agg.update(&make_update(SourceId::Okx, AssetId::BTC, base + 100_000, 99));
        agg.update(&make_update(SourceId::Bybit, AssetId::BTC, base + 200_000, 99));
        agg.update(&make_update(SourceId::Kraken, AssetId::BTC, base + 500_000_000_000, 99)); // outlier high

        let result = agg.aggregate(AssetId::BTC, 100);
        match result {
            AssetState::Available { price, .. } => {
                // After trim: [base, base+100000, base+200000]. Median = base+100000
                assert_eq!(price, U256::from(base + 100_000));
            }
            _ => panic!("expected Available"),
        }
    }

    #[test]
    fn medium_confidence_spread() {
        let mut agg = Aggregator::new();
        let base = 100_000_000u64; // $1.00
                                   // With 3 sources, no trimming. Spread = (hi - lo) * 10000 / median.
                                   // We want spread > 10 and <= 50 bps.
                                   // 20 bps of $1.00 = 0.002 * 100_000_000 / 10000 = 20_000.
                                   // So hi - lo = 200_000 → spread = 200_000 * 10000 / 100_000_000 = 20 bps → Medium.
        agg.update(&make_update(SourceId::Binance, AssetId::USDC, base - 100_000, 99));
        agg.update(&make_update(SourceId::Coinbase, AssetId::USDC, base, 99));
        agg.update(&make_update(SourceId::Okx, AssetId::USDC, base + 100_000, 99));

        let result = agg.aggregate(AssetId::USDC, 100);
        match result {
            AssetState::Available { confidence, .. } => {
                assert_eq!(confidence, Confidence::Medium);
            }
            _ => panic!("expected Available"),
        }
    }

    #[test]
    fn clear_source_removes_data() {
        let mut agg = Aggregator::new();
        agg.update(&make_update(SourceId::Binance, AssetId::BTC, 5_000_000_000_000, 99));
        agg.update(&make_update(SourceId::Coinbase, AssetId::BTC, 5_000_000_000_000, 99));
        assert!(matches!(agg.aggregate(AssetId::BTC, 100), AssetState::Available { .. }));

        agg.clear_source(SourceId::Binance as u8);
        agg.clear_source(SourceId::Coinbase as u8);
        assert_eq!(agg.aggregate(AssetId::BTC, 100), AssetState::Unavailable);
    }

    #[test]
    fn newer_timestamp_overwrites() {
        let mut agg = Aggregator::new();
        agg.update(&make_update(SourceId::Binance, AssetId::BTC, 100, 90));
        agg.update(&make_update(SourceId::Binance, AssetId::BTC, 200, 95));
        // Older timestamp should be ignored
        agg.update(&make_update(SourceId::Binance, AssetId::BTC, 50, 89));

        let result = agg.aggregate(AssetId::BTC, 100);
        match result {
            AssetState::Available { price, .. } => {
                assert_eq!(price, U256::from(200));
            }
            _ => panic!("expected Available"),
        }
    }

    #[test]
    fn aggregate_all_covers_all_assets() {
        let agg = Aggregator::new();
        let result = agg.aggregate_all(100);
        assert_eq!(result.len(), AssetId::ALL.len());
        for asset in AssetId::ALL {
            assert_eq!(result[&asset], AssetState::Unavailable);
        }
    }

    #[test]
    fn staleness_at_exact_boundary_is_fresh() {
        let mut agg = Aggregator::new();
        // STALENESS_THRESHOLD=10. Data at t=90, query at t=100. 100-90=10 <= 10 → fresh.
        agg.update(&make_update(SourceId::Binance, AssetId::BTC, 5_000_000_000_000, 90));
        let result = agg.aggregate(AssetId::BTC, 100);
        assert!(matches!(result, AssetState::Available { .. }));
    }

    #[test]
    fn staleness_one_past_boundary_is_stale() {
        let mut agg = Aggregator::new();
        // Data at t=89, query at t=100. 100-89=11 > 10 → stale.
        agg.update(&make_update(SourceId::Binance, AssetId::BTC, 5_000_000_000_000, 89));
        assert_eq!(agg.aggregate(AssetId::BTC, 100), AssetState::Unavailable);
    }

    #[test]
    fn large_spread_is_degraded() {
        let mut agg = Aggregator::new();
        let base = 100_000_000u64; // $1.00
                                   // Spread > 50bps → Degraded
                                   // 60bps of $1.00 = 600_000 spread
        agg.update(&make_update(SourceId::Binance, AssetId::USDC, base - 300_000, 99));
        agg.update(&make_update(SourceId::Coinbase, AssetId::USDC, base, 99));
        agg.update(&make_update(SourceId::Okx, AssetId::USDC, base + 300_000, 99));

        let result = agg.aggregate(AssetId::USDC, 100);
        match result {
            AssetState::Available { confidence, .. } => {
                assert_eq!(confidence, Confidence::Degraded);
            }
            _ => panic!("expected Available"),
        }
    }

    #[test]
    fn four_sources_no_outlier_trim() {
        let mut agg = Aggregator::new();
        let base = 5_000_000_000_000u64;
        // 4 sources: no outlier trim (only >= 5 triggers trim)
        agg.update(&make_update(SourceId::Binance, AssetId::BTC, base, 99));
        agg.update(&make_update(SourceId::Coinbase, AssetId::BTC, base + 100_000, 99));
        agg.update(&make_update(SourceId::Okx, AssetId::BTC, base + 200_000, 99));
        agg.update(&make_update(SourceId::Bybit, AssetId::BTC, base + 300_000, 99));

        let result = agg.aggregate(AssetId::BTC, 100);
        match result {
            AssetState::Available { price, .. } => {
                // Sorted: [base, base+100000, base+200000, base+300000]. Median = [2] = base+200000
                assert_eq!(price, U256::from(base + 200_000));
            }
            _ => panic!("expected Available"),
        }
    }
}
