//! Constraint model — per-argument verification for call policies.

use alloy_primitives::{Bytes, B256, U256};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::limit::{Limit, LimitEngine, LimitError, LimitState};

/// Comparison condition for a constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ConditionType {
    /// No condition — only the limit is checked.
    Unconstrained = 0,
    /// `arg == ref_value`
    Equal = 1,
    /// `arg > ref_value`
    Greater = 2,
    /// `arg < ref_value`
    Less = 3,
    /// `arg >= ref_value`
    GreaterEqual = 4,
    /// `arg <= ref_value`
    LessEqual = 5,
    /// `arg != ref_value`
    NotEqual = 6,
}

impl ConditionType {
    /// Try to convert a `u8` into a [`ConditionType`].
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Unconstrained),
            1 => Some(Self::Equal),
            2 => Some(Self::Greater),
            3 => Some(Self::Less),
            4 => Some(Self::GreaterEqual),
            5 => Some(Self::LessEqual),
            6 => Some(Self::NotEqual),
            _ => None,
        }
    }
}

/// A constraint on a single calldata argument.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Constraint {
    /// Calldata argument index (0-based).
    pub index: u8,
    /// Comparison condition.
    pub condition: ConditionType,
    /// Reference value for comparison (32 bytes, left-padded).
    pub ref_value: B256,
    /// Cumulative/periodic cap on this argument's value.
    pub limit: Limit,
}

/// Errors returned by the [`ConstraintEngine`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConstraintError {
    /// Calldata too short to extract the argument at the given index.
    #[error("calldata too short: need {needed} bytes, got {got}")]
    CalldataTooShort {
        /// Minimum required calldata length.
        needed: usize,
        /// Actual calldata length.
        got: usize,
    },
    /// The condition comparison failed.
    #[error("condition failed: arg {arg} {condition:?} ref {ref_value}")]
    ConditionFailed {
        /// Extracted argument value.
        arg: B256,
        /// The condition that was not met.
        condition: ConditionType,
        /// Reference value.
        ref_value: B256,
    },
    /// The limit check failed.
    #[error("constraint limit error: {0}")]
    LimitError(#[from] LimitError),
}

/// Engine for verifying constraints on calldata arguments.
#[derive(Debug)]
pub struct ConstraintEngine;

impl ConstraintEngine {
    /// Extract the ABI-encoded argument at `index` from `calldata`.
    ///
    /// Calldata layout: `[0..4]` = selector, then 32-byte words.
    /// `arg[index]` = `calldata[4 + index * 32 .. 4 + (index + 1) * 32]`
    pub fn extract_argument(calldata: &Bytes, index: u8) -> Result<B256, ConstraintError> {
        let start = 4 + (index as usize) * 32;
        let end = start + 32;
        if calldata.len() < end {
            return Err(ConstraintError::CalldataTooShort { needed: end, got: calldata.len() });
        }
        let mut word = [0u8; 32];
        word.copy_from_slice(&calldata[start..end]);
        Ok(B256::from(word))
    }

    /// Verify the condition of a constraint against a calldata argument.
    pub fn verify_condition(
        condition: ConditionType,
        arg: B256,
        ref_value: B256,
    ) -> Result<(), ConstraintError> {
        // Interpret as U256 for ordered comparisons.
        let arg_u256 = U256::from_be_bytes(arg.0);
        let ref_u256 = U256::from_be_bytes(ref_value.0);

        let passed = match condition {
            ConditionType::Unconstrained => true,
            ConditionType::Equal => arg == ref_value,
            ConditionType::Greater => arg_u256 > ref_u256,
            ConditionType::Less => arg_u256 < ref_u256,
            ConditionType::GreaterEqual => arg_u256 >= ref_u256,
            ConditionType::LessEqual => arg_u256 <= ref_u256,
            ConditionType::NotEqual => arg != ref_value,
        };

        if passed {
            Ok(())
        } else {
            Err(ConstraintError::ConditionFailed { arg, condition, ref_value })
        }
    }

    /// Verify a single constraint against calldata and update the limit state.
    ///
    /// Steps:
    /// 1. Extract the argument at the specified index.
    /// 2. Check the condition.
    /// 3. Check and update the limit (using the argument value as the amount for value-tracking
    ///    limits).
    pub fn verify_constraint(
        constraint: &Constraint,
        calldata: &Bytes,
        limit_state: &mut LimitState,
        block_timestamp: u64,
    ) -> Result<(), ConstraintError> {
        let arg = Self::extract_argument(calldata, constraint.index)?;
        Self::verify_condition(constraint.condition, arg, constraint.ref_value)?;
        // Use the argument value as the "amount" for the limit check.
        let amount = U256::from_be_bytes(arg.0);
        LimitEngine::check_and_update(&constraint.limit, limit_state, amount, block_timestamp)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build calldata: 4-byte selector + 32-byte words.
    fn make_calldata(selector: [u8; 4], args: &[U256]) -> Bytes {
        let mut data = selector.to_vec();
        for arg in args {
            data.extend_from_slice(&arg.to_be_bytes::<32>());
        }
        Bytes::from(data)
    }

    #[test]
    fn extract_argument_basic() {
        let calldata = make_calldata([0xAB, 0xCD, 0x00, 0x00], &[U256::from(42), U256::from(99)]);
        let arg0 = ConstraintEngine::extract_argument(&calldata, 0).unwrap();
        assert_eq!(U256::from_be_bytes(arg0.0), U256::from(42));

        let arg1 = ConstraintEngine::extract_argument(&calldata, 1).unwrap();
        assert_eq!(U256::from_be_bytes(arg1.0), U256::from(99));
    }

    #[test]
    fn extract_argument_too_short() {
        let calldata = make_calldata([0x00; 4], &[U256::from(1)]);
        // Index 0 is fine (needs 36 bytes, we have 36)
        assert!(ConstraintEngine::extract_argument(&calldata, 0).is_ok());
        // Index 1 needs 68 bytes
        let err = ConstraintEngine::extract_argument(&calldata, 1).unwrap_err();
        assert!(matches!(err, ConstraintError::CalldataTooShort { needed: 68, got: 36 }));
    }

    #[test]
    fn condition_equal() {
        let val = B256::from(U256::from(10).to_be_bytes::<32>());
        assert!(ConstraintEngine::verify_condition(ConditionType::Equal, val, val).is_ok());

        let other = B256::from(U256::from(11).to_be_bytes::<32>());
        assert!(ConstraintEngine::verify_condition(ConditionType::Equal, val, other).is_err());
    }

    #[test]
    fn condition_greater() {
        let ten = B256::from(U256::from(10).to_be_bytes::<32>());
        let five = B256::from(U256::from(5).to_be_bytes::<32>());

        assert!(ConstraintEngine::verify_condition(ConditionType::Greater, ten, five).is_ok());
        assert!(ConstraintEngine::verify_condition(ConditionType::Greater, five, ten).is_err());
        assert!(ConstraintEngine::verify_condition(ConditionType::Greater, ten, ten).is_err());
    }

    #[test]
    fn condition_less() {
        let ten = B256::from(U256::from(10).to_be_bytes::<32>());
        let five = B256::from(U256::from(5).to_be_bytes::<32>());

        assert!(ConstraintEngine::verify_condition(ConditionType::Less, five, ten).is_ok());
        assert!(ConstraintEngine::verify_condition(ConditionType::Less, ten, five).is_err());
        assert!(ConstraintEngine::verify_condition(ConditionType::Less, ten, ten).is_err());
    }

    #[test]
    fn condition_greater_equal() {
        let ten = B256::from(U256::from(10).to_be_bytes::<32>());
        let five = B256::from(U256::from(5).to_be_bytes::<32>());

        assert!(ConstraintEngine::verify_condition(ConditionType::GreaterEqual, ten, five).is_ok());
        assert!(ConstraintEngine::verify_condition(ConditionType::GreaterEqual, ten, ten).is_ok());
        assert!(ConstraintEngine::verify_condition(ConditionType::GreaterEqual, five, ten).is_err());
    }

    #[test]
    fn condition_less_equal() {
        let ten = B256::from(U256::from(10).to_be_bytes::<32>());
        let five = B256::from(U256::from(5).to_be_bytes::<32>());

        assert!(ConstraintEngine::verify_condition(ConditionType::LessEqual, five, ten).is_ok());
        assert!(ConstraintEngine::verify_condition(ConditionType::LessEqual, ten, ten).is_ok());
        assert!(ConstraintEngine::verify_condition(ConditionType::LessEqual, ten, five).is_err());
    }

    #[test]
    fn condition_not_equal() {
        let ten = B256::from(U256::from(10).to_be_bytes::<32>());
        let five = B256::from(U256::from(5).to_be_bytes::<32>());

        assert!(ConstraintEngine::verify_condition(ConditionType::NotEqual, ten, five).is_ok());
        assert!(ConstraintEngine::verify_condition(ConditionType::NotEqual, ten, ten).is_err());
    }

    #[test]
    fn condition_unconstrained_always_passes() {
        let any = B256::from(U256::MAX.to_be_bytes::<32>());
        let zero = B256::ZERO;
        assert!(ConstraintEngine::verify_condition(ConditionType::Unconstrained, any, zero).is_ok());
    }

    #[test]
    fn verify_constraint_full() {
        // play(uint8 mode, uint256 amount)
        // Constraint: index=1 (amount), condition=LessEqual, ref=1000, limit=lifetime 5000
        let constraint = Constraint {
            index: 1,
            condition: ConditionType::LessEqual,
            ref_value: B256::from(U256::from(1000).to_be_bytes::<32>()),
            limit: Limit::lifetime(U256::from(5000)),
        };

        let calldata = make_calldata([0x12, 0x34, 0x56, 0x78], &[U256::from(1), U256::from(500)]);
        let mut state = LimitState::default();

        // 500 <= 1000 ✓, 500 <= 5000 ✓
        ConstraintEngine::verify_constraint(&constraint, &calldata, &mut state, 0).unwrap();
        assert_eq!(state.used, U256::from(500));

        // Another 500 — total 1000, still within 5000 lifetime
        ConstraintEngine::verify_constraint(&constraint, &calldata, &mut state, 0).unwrap();
        assert_eq!(state.used, U256::from(1000));
    }

    #[test]
    fn verify_constraint_condition_fails() {
        let constraint = Constraint {
            index: 0,
            condition: ConditionType::Equal,
            ref_value: B256::from(U256::from(42).to_be_bytes::<32>()),
            limit: Limit::unlimited(),
        };

        let calldata = make_calldata([0x00; 4], &[U256::from(99)]);
        let mut state = LimitState::default();

        let err =
            ConstraintEngine::verify_constraint(&constraint, &calldata, &mut state, 0).unwrap_err();
        assert!(matches!(err, ConstraintError::ConditionFailed { .. }));
    }

    #[test]
    fn verify_constraint_limit_exceeded() {
        let constraint = Constraint {
            index: 0,
            condition: ConditionType::Unconstrained,
            ref_value: B256::ZERO,
            limit: Limit::lifetime(U256::from(100)),
        };

        let calldata = make_calldata([0x00; 4], &[U256::from(101)]);
        let mut state = LimitState::default();

        let err =
            ConstraintEngine::verify_constraint(&constraint, &calldata, &mut state, 0).unwrap_err();
        assert!(matches!(err, ConstraintError::LimitError(_)));
    }

    #[test]
    fn condition_type_from_u8_all() {
        assert_eq!(ConditionType::from_u8(0), Some(ConditionType::Unconstrained));
        assert_eq!(ConditionType::from_u8(1), Some(ConditionType::Equal));
        assert_eq!(ConditionType::from_u8(2), Some(ConditionType::Greater));
        assert_eq!(ConditionType::from_u8(3), Some(ConditionType::Less));
        assert_eq!(ConditionType::from_u8(4), Some(ConditionType::GreaterEqual));
        assert_eq!(ConditionType::from_u8(5), Some(ConditionType::LessEqual));
        assert_eq!(ConditionType::from_u8(6), Some(ConditionType::NotEqual));
        assert_eq!(ConditionType::from_u8(7), None);
    }
}
