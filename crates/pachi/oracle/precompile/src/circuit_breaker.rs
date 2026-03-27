//! Circuit breaker logic for oracle price updates.
//!
//! Forces confidence to `Degraded` when a price moves more than
//! `CIRCUIT_BREAKER_BPS` (10%) relative to any of the last
//! `CIRCUIT_BREAKER_WINDOW` (5) block prices.

use alloy_primitives::U256;
use pachi_primitives::{Confidence, CIRCUIT_BREAKER_BPS, CIRCUIT_BREAKER_WINDOW};

/// Maintains a sliding window of recent prices per asset for circuit breaker checks.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// Ring buffer of recent prices. Entries are `(price, block_number)`.
    /// Only stores prices for blocks where the asset was `Available`.
    recent: Vec<(U256, u64)>,
    /// Max entries to keep.
    window: usize,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl CircuitBreaker {
    /// Creates a new circuit breaker with the default window size.
    pub fn new() -> Self {
        Self {
            recent: Vec::with_capacity(CIRCUIT_BREAKER_WINDOW as usize),
            window: CIRCUIT_BREAKER_WINDOW as usize,
        }
    }

    /// Checks whether the new price triggers the circuit breaker.
    ///
    /// Returns `true` if the price moved more than `CIRCUIT_BREAKER_BPS` relative
    /// to any entry in the window, meaning confidence should be forced to `Degraded`.
    pub fn is_triggered(&self, new_price: U256) -> bool {
        if new_price.is_zero() {
            return false;
        }

        for &(ref_price, _) in &self.recent {
            if ref_price.is_zero() {
                continue;
            }
            let delta =
                if new_price > ref_price { new_price - ref_price } else { ref_price - new_price };
            // delta_bps = delta * 10000 / ref_price
            let delta_bps = delta * U256::from(10_000) / ref_price;
            if delta_bps > U256::from(CIRCUIT_BREAKER_BPS) {
                return true;
            }
        }
        false
    }

    /// Applies the circuit breaker check and adjusts confidence if triggered.
    ///
    /// Returns the (possibly degraded) confidence level.
    pub fn apply(&self, new_price: U256, confidence: Confidence) -> Confidence {
        if self.is_triggered(new_price) {
            confidence.worse(Confidence::Degraded)
        } else {
            confidence
        }
    }

    /// Records a price observation, maintaining the sliding window.
    pub fn record(&mut self, price: U256, block_number: u64) {
        if self.recent.len() >= self.window {
            self.recent.remove(0);
        }
        self.recent.push((price, block_number));
    }

    /// Returns the number of entries currently in the window.
    pub const fn len(&self) -> usize {
        self.recent.len()
    }

    /// Returns `true` if the window has no entries.
    pub const fn is_empty(&self) -> bool {
        self.recent.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_window_never_triggers() {
        let cb = CircuitBreaker::new();
        assert!(!cb.is_triggered(U256::from(50_000_00000000u64)));
    }

    #[test]
    fn small_move_does_not_trigger() {
        let mut cb = CircuitBreaker::new();
        let base = U256::from(50_000_00000000u64); // $50,000
        cb.record(base, 1);

        // 5% move (below 10% threshold)
        let new_price = base + base * U256::from(500) / U256::from(10_000);
        assert!(!cb.is_triggered(new_price));
    }

    #[test]
    fn large_move_triggers() {
        let mut cb = CircuitBreaker::new();
        let base = U256::from(50_000_00000000u64);
        cb.record(base, 1);

        // 15% move (above 10% threshold)
        let new_price = base + base * U256::from(1_500) / U256::from(10_000);
        assert!(cb.is_triggered(new_price));
    }

    #[test]
    fn large_drop_triggers() {
        let mut cb = CircuitBreaker::new();
        let base = U256::from(50_000_00000000u64);
        cb.record(base, 1);

        // -12% move
        let new_price = base - base * U256::from(1_200) / U256::from(10_000);
        assert!(cb.is_triggered(new_price));
    }

    #[test]
    fn window_slides() {
        let mut cb = CircuitBreaker::new();
        let base = U256::from(100_00000000u64); // $100

        // Fill window with 5 entries at $100
        for i in 0..5 {
            cb.record(base, i);
        }
        assert_eq!(cb.len(), 5);

        // Gradually move price up with small steps (+1.5% each)
        // After 5 steps, the old $100 entries are all evicted
        let mut price = base;
        for i in 5..10 {
            price += price * U256::from(150) / U256::from(10_000);
            cb.record(price, i);
        }

        // After sliding, old $100 entries are gone
        assert_eq!(cb.len(), 5);
        // Another 1.5% step should NOT trigger (window entries are within ~6% total)
        let next = price + price * U256::from(150) / U256::from(10_000);
        assert!(!cb.is_triggered(next));
    }

    #[test]
    fn apply_degrades_confidence() {
        let mut cb = CircuitBreaker::new();
        let base = U256::from(50_000_00000000u64);
        cb.record(base, 1);

        let spike = base * U256::from(2); // 100% move
        assert_eq!(cb.apply(spike, Confidence::High), Confidence::Degraded);
    }

    #[test]
    fn apply_preserves_confidence_on_small_move() {
        let mut cb = CircuitBreaker::new();
        let base = U256::from(50_000_00000000u64);
        cb.record(base, 1);

        let small_move = base + U256::from(100_00000000u64); // ~0.2%
        assert_eq!(cb.apply(small_move, Confidence::High), Confidence::High);
    }

    #[test]
    fn zero_price_does_not_trigger() {
        let mut cb = CircuitBreaker::new();
        cb.record(U256::from(100u64), 1);
        assert!(!cb.is_triggered(U256::ZERO));
    }

    #[test]
    fn zero_ref_price_skipped() {
        let mut cb = CircuitBreaker::new();
        cb.record(U256::ZERO, 1);
        assert!(!cb.is_triggered(U256::from(50_000_00000000u64)));
    }
}
