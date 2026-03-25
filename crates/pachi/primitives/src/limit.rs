//! Limit model — shared rate-limiting primitive for Session Key and Gas Sponsor systems.

use alloy_primitives::U256;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Limit type discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum LimitType {
    /// No restriction.
    Unlimited = 0,
    /// Cumulative cap over entire lifetime.
    Lifetime = 1,
    /// Refilling cap per time period.
    Allowance = 2,
}

impl LimitType {
    /// Try to convert a `u8` into a [`LimitType`].
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Unlimited),
            1 => Some(Self::Lifetime),
            2 => Some(Self::Allowance),
            _ => None,
        }
    }
}

/// A limit configuration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Limit {
    /// 0 = Unlimited, 1 = Lifetime, 2 = Allowance.
    pub limit_type: LimitType,
    /// Quantity cap.
    pub limit: U256,
    /// Allowance only: refill period in seconds.
    pub period: u64,
}

impl Limit {
    /// Creates an unlimited limit.
    pub const fn unlimited() -> Self {
        Self { limit_type: LimitType::Unlimited, limit: U256::ZERO, period: 0 }
    }

    /// Creates a lifetime limit.
    pub const fn lifetime(limit: U256) -> Self {
        Self { limit_type: LimitType::Lifetime, limit, period: 0 }
    }

    /// Creates an allowance limit with given period (seconds).
    pub const fn allowance(limit: U256, period: u64) -> Self {
        Self { limit_type: LimitType::Allowance, limit, period }
    }
}

/// Mutable on-chain state for a limit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct LimitState {
    /// Amount consumed in the current window or lifetime.
    pub used: U256,
    /// Allowance mode: last reset window number.
    pub last_window: u64,
}

/// Errors returned by the [`LimitEngine`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LimitError {
    /// The requested amount would exceed the limit.
    #[error("limit exceeded: used {used} + amount {amount} > limit {limit}")]
    Exceeded {
        /// Amount already used.
        used: U256,
        /// Amount being requested.
        amount: U256,
        /// Configured limit cap.
        limit: U256,
    },
    /// Period is zero for an allowance limit (invalid config).
    #[error("allowance limit has zero period")]
    ZeroPeriod,
}

/// Engine for checking and updating limits.
#[derive(Debug)]
pub struct LimitEngine;

impl LimitEngine {
    /// Check whether `amount` is within the limit and, if so, update `state` in place.
    ///
    /// `block_timestamp` is the current block's unix timestamp (seconds).
    pub fn check_and_update(
        limit: &Limit,
        state: &mut LimitState,
        amount: U256,
        block_timestamp: u64,
    ) -> Result<(), LimitError> {
        match limit.limit_type {
            LimitType::Unlimited => Ok(()),
            LimitType::Lifetime => {
                let new_used = state.used + amount;
                if new_used > limit.limit {
                    return Err(LimitError::Exceeded {
                        used: state.used,
                        amount,
                        limit: limit.limit,
                    });
                }
                state.used = new_used;
                Ok(())
            }
            LimitType::Allowance => {
                if limit.period == 0 {
                    return Err(LimitError::ZeroPeriod);
                }
                let current_window = block_timestamp / limit.period;
                if state.last_window != current_window {
                    // Window transition — reset usage.
                    state.used = U256::ZERO;
                    state.last_window = current_window;
                }
                let new_used = state.used + amount;
                if new_used > limit.limit {
                    return Err(LimitError::Exceeded {
                        used: state.used,
                        amount,
                        limit: limit.limit,
                    });
                }
                state.used = new_used;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_always_passes() {
        let limit = Limit::unlimited();
        let mut state = LimitState::default();
        assert!(LimitEngine::check_and_update(&limit, &mut state, U256::MAX, 0).is_ok());
        // State unchanged for unlimited
        assert_eq!(state.used, U256::ZERO);
    }

    #[test]
    fn lifetime_basic() {
        let limit = Limit::lifetime(U256::from(100));
        let mut state = LimitState::default();

        // Use 60
        LimitEngine::check_and_update(&limit, &mut state, U256::from(60), 0).unwrap();
        assert_eq!(state.used, U256::from(60));

        // Use 40 more — exactly at limit
        LimitEngine::check_and_update(&limit, &mut state, U256::from(40), 0).unwrap();
        assert_eq!(state.used, U256::from(100));

        // Use 1 more — exceeds
        let err = LimitEngine::check_and_update(&limit, &mut state, U256::from(1), 0).unwrap_err();
        assert_eq!(
            err,
            LimitError::Exceeded {
                used: U256::from(100),
                amount: U256::from(1),
                limit: U256::from(100),
            }
        );
    }

    #[test]
    fn lifetime_does_not_reset_on_timestamp() {
        let limit = Limit::lifetime(U256::from(50));
        let mut state = LimitState::default();

        LimitEngine::check_and_update(&limit, &mut state, U256::from(50), 1000).unwrap();
        // Even with much later timestamp, state persists
        let err =
            LimitEngine::check_and_update(&limit, &mut state, U256::from(1), 999_999).unwrap_err();
        assert!(matches!(err, LimitError::Exceeded { .. }));
    }

    #[test]
    fn allowance_window_transition() {
        let limit = Limit::allowance(U256::from(100), 3600); // 100 per hour
        let mut state = LimitState::default();

        // Window 0 (timestamp 0..3599)
        LimitEngine::check_and_update(&limit, &mut state, U256::from(100), 0).unwrap();
        assert_eq!(state.used, U256::from(100));
        assert_eq!(state.last_window, 0);

        // Still window 0 — should fail
        let err =
            LimitEngine::check_and_update(&limit, &mut state, U256::from(1), 3599).unwrap_err();
        assert!(matches!(err, LimitError::Exceeded { .. }));

        // Window 1 (timestamp 3600) — reset
        LimitEngine::check_and_update(&limit, &mut state, U256::from(50), 3600).unwrap();
        assert_eq!(state.used, U256::from(50));
        assert_eq!(state.last_window, 1);
    }

    #[test]
    fn allowance_skips_windows() {
        let limit = Limit::allowance(U256::from(10), 100);
        let mut state = LimitState::default();

        LimitEngine::check_and_update(&limit, &mut state, U256::from(10), 50).unwrap();
        assert_eq!(state.last_window, 0);

        // Jump to window 5 — usage resets
        LimitEngine::check_and_update(&limit, &mut state, U256::from(10), 500).unwrap();
        assert_eq!(state.used, U256::from(10));
        assert_eq!(state.last_window, 5);
    }

    #[test]
    fn allowance_zero_period_errors() {
        let limit = Limit::allowance(U256::from(10), 0);
        let mut state = LimitState::default();
        assert_eq!(
            LimitEngine::check_and_update(&limit, &mut state, U256::from(1), 0),
            Err(LimitError::ZeroPeriod)
        );
    }

    #[test]
    fn lifetime_zero_amount_succeeds() {
        let limit = Limit::lifetime(U256::from(0));
        let mut state = LimitState::default();
        // Zero amount against zero limit
        LimitEngine::check_and_update(&limit, &mut state, U256::ZERO, 0).unwrap();
        // Any non-zero fails
        assert!(LimitEngine::check_and_update(&limit, &mut state, U256::from(1), 0).is_err());
    }

    #[test]
    fn limit_serde_roundtrip() {
        let limit = Limit::allowance(U256::from(500), 86400);
        let json = serde_json::to_string(&limit).unwrap();
        let deserialized: Limit = serde_json::from_str(&json).unwrap();
        assert_eq!(limit, deserialized);

        let state = LimitState { used: U256::from(42), last_window: 7 };
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: LimitState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, deserialized);
    }
}
