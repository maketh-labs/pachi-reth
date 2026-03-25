//! Session call and transfer policy types.

use alloy_primitives::{Address, FixedBytes, U256};
use serde::{Deserialize, Serialize};

use crate::{Constraint, Limit};

/// A whitelisted contract function call within a session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CallPolicy {
    /// Target contract address.
    pub target: Address,
    /// Target function selector (4 bytes).
    pub selector: FixedBytes<4>,
    /// Native value cap for this function.
    pub value_limit: Limit,
    /// Max native value per single call.
    pub max_value_per_use: U256,
    /// Per-argument constraints.
    pub constraints: Vec<Constraint>,
}

/// A whitelisted native transfer within a session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransferPolicy {
    /// Transfer recipient.
    pub target: Address,
    /// Max per transfer.
    pub max_value_per_use: U256,
    /// Total transfer cap.
    pub value_limit: Limit,
}
