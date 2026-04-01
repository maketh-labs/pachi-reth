//! Genesis configuration extensions for Pachi-specific parameters.

use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

mod serde_u128_string {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S>(value: &u128, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<u128, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            u128::from_str_radix(hex, 16).map_err(serde::de::Error::custom)
        } else {
            s.parse::<u128>().map_err(serde::de::Error::custom)
        }
    }
}

/// Pachi-specific genesis configuration.
///
/// Embedded in the genesis `extra_fields` under `"pachiConfig"`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PachiGenesisConfig {
    /// Session key system configuration.
    pub session_config: SessionGenesisConfig,
    /// Gas sponsor system configuration.
    pub sponsor_config: SponsorGenesisConfig,
    /// Oracle system configuration.
    pub oracle_config: OracleGenesisConfig,
    /// Block number or timestamp at which Phase 1 activates.
    #[serde(default)]
    pub phase1_block: u64,
    /// Block number or timestamp at which Phase 2 activates (0 = not scheduled).
    #[serde(default)]
    pub phase2_block: u64,
}

/// Session key system genesis parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionGenesisConfig {
    /// Max concurrent sessions per account.
    pub max_sessions_per_account: u64,
    /// Max session duration in seconds.
    pub max_session_duration: u64,
    /// Max call policies per session.
    pub max_call_policies_per_session: u64,
    /// Max constraints per call policy.
    pub max_constraints_per_call_policy: u64,
    /// Max transfer policies per session.
    pub max_transfer_policies_per_session: u64,
    /// Max policy size in bytes.
    pub max_policy_size: u64,
}

impl Default for SessionGenesisConfig {
    fn default() -> Self {
        Self {
            max_sessions_per_account: 10,
            max_session_duration: 2_592_000, // 30 days
            max_call_policies_per_session: 20,
            max_constraints_per_call_policy: 10,
            max_transfer_policies_per_session: 10,
            max_policy_size: 4096,
        }
    }
}

/// Gas sponsor system genesis parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SponsorGenesisConfig {
    /// Minimum initial deposit for deposit-mode sponsors (wei), as a decimal string.
    #[serde(with = "serde_u128_string")]
    pub min_deposit_sponsor_deposit: u128,
    /// Additional gas overhead charged per sponsored tx.
    pub sponsor_gas_overhead: u64,
    /// Max call policies per sponsor.
    pub max_call_policies_per_sponsor: u64,
    /// Max constraints per call policy.
    pub max_constraints_per_call_policy: u64,
    /// Max entries in `allowed_senders`.
    pub max_allowed_senders: u64,
    /// Initial owner of the `SponsorHub`.
    pub sponsor_hub_owner: Address,
}

impl Default for SponsorGenesisConfig {
    fn default() -> Self {
        Self {
            min_deposit_sponsor_deposit: 100_000_000_000_000_000, // 0.1 ETH
            sponsor_gas_overhead: 20_000,
            max_call_policies_per_sponsor: 50,
            max_constraints_per_call_policy: 10,
            max_allowed_senders: 10_000,
            sponsor_hub_owner: Address::ZERO,
        }
    }
}

/// Oracle system genesis parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OracleGenesisConfig {
    /// Oracle engine version expected at genesis.
    pub engine_version: u16,
}

impl Default for OracleGenesisConfig {
    fn default() -> Self {
        Self { engine_version: 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_serde_roundtrip() {
        let config = PachiGenesisConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let deserialized: PachiGenesisConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn default_session_values() {
        let config = SessionGenesisConfig::default();
        assert_eq!(config.max_sessions_per_account, 10);
        assert_eq!(config.max_session_duration, 2_592_000);
        assert_eq!(config.max_call_policies_per_session, 20);
        assert_eq!(config.max_constraints_per_call_policy, 10);
        assert_eq!(config.max_transfer_policies_per_session, 10);
        assert_eq!(config.max_policy_size, 4096);
    }

    #[test]
    fn default_sponsor_values() {
        let config = SponsorGenesisConfig::default();
        assert_eq!(config.min_deposit_sponsor_deposit, 100_000_000_000_000_000);
        assert_eq!(config.sponsor_gas_overhead, 20_000);
        assert_eq!(config.sponsor_hub_owner, Address::ZERO);
    }

    #[test]
    fn deserialize_from_json() {
        let json = r#"{
            "sessionConfig": {
                "maxSessionsPerAccount": 5,
                "maxSessionDuration": 86400,
                "maxCallPoliciesPerSession": 10,
                "maxConstraintsPerCallPolicy": 5,
                "maxTransferPoliciesPerSession": 5,
                "maxPolicySize": 2048
            },
            "sponsorConfig": {
                "minDepositSponsorDeposit": "100000000000000000",
                "sponsorGasOverhead": 15000,
                "maxCallPoliciesPerSponsor": 25,
                "maxConstraintsPerCallPolicy": 5,
                "maxAllowedSenders": 5000,
                "sponsorHubOwner": "0x0000000000000000000000000000000000000001"
            },
            "oracleConfig": {
                "engineVersion": 1
            },
            "phase1Block": 0,
            "phase2Block": 100000
        }"#;

        let config: PachiGenesisConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.session_config.max_sessions_per_account, 5);
        assert_eq!(config.sponsor_config.sponsor_gas_overhead, 15000);
        assert_eq!(config.phase2_block, 100000);
    }
}
