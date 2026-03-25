//! Pachi Chain hardfork definitions.

use reth_chainspec::hardfork;

hardfork!(
    /// Pachi Chain protocol upgrades.
    PachiHardfork {
        /// Phase 1: Testnet — single sequencer, all features active.
        Phase1,
        /// Phase 2: Mainnet — multi-sequencer with IBFT2, threshold VRF, slashing.
        Phase2,
    }
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardfork_names() {
        assert_eq!(PachiHardfork::Phase1.name(), "Phase1");
        assert_eq!(PachiHardfork::Phase2.name(), "Phase2");
    }

    #[test]
    fn hardfork_display() {
        assert_eq!(format!("{}", PachiHardfork::Phase1), "Phase1");
        assert_eq!(format!("{}", PachiHardfork::Phase2), "Phase2");
    }
}
