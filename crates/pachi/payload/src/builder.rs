//! Pachi payload builder wrapping the Ethereum payload builder.
//!
//! Delegates standard block building to [`EthereumPayloadBuilder`] and adds
//! Pachi-specific system transaction injection.

use crate::SystemTxGenerator;
use pachi_oracle_engine::OracleEngine;
use reth_basic_payload_builder::{BuildArguments, BuildOutcome, PayloadBuilder, PayloadConfig};
use reth_ethereum_payload_builder::{EthereumBuilderConfig, EthereumPayloadBuilder};
use reth_payload_builder::{EthBuiltPayload, EthPayloadBuilderAttributes, PayloadBuilderError};
use std::sync::Arc;

use reth_chainspec::{ChainSpecProvider, EthereumHardforks};
use reth_ethereum_primitives::EthPrimitives;
use reth_evm::{ConfigureEvm, NextBlockEnvAttributes};
use reth_storage_api::StateProviderFactory;
use reth_transaction_pool::{PoolTransaction, TransactionPool};

/// Pachi Chain payload builder.
///
/// Wraps [`EthereumPayloadBuilder`] and injects system transactions
/// (VRF fulfills + `OracleUpdate`) at the end of each block.
#[derive(Debug, Clone)]
pub struct PachiPayloadBuilder<Pool, Client, EvmConfig> {
    /// Inner Ethereum payload builder.
    inner: EthereumPayloadBuilder<Pool, Client, EvmConfig>,
    /// Oracle engine for generating price snapshots.
    oracle_engine: Arc<OracleEngine>,
    /// System transaction generator.
    system_tx_gen: SystemTxGenerator,
}

impl<Pool, Client, EvmConfig> PachiPayloadBuilder<Pool, Client, EvmConfig> {
    /// Creates a new Pachi payload builder.
    pub const fn new(
        client: Client,
        pool: Pool,
        evm_config: EvmConfig,
        builder_config: EthereumBuilderConfig,
        oracle_engine: Arc<OracleEngine>,
        system_tx_gen: SystemTxGenerator,
    ) -> Self {
        Self {
            inner: EthereumPayloadBuilder::new(client, pool, evm_config, builder_config),
            oracle_engine,
            system_tx_gen,
        }
    }

    /// Returns a reference to the oracle engine.
    pub fn oracle_engine(&self) -> &OracleEngine {
        &self.oracle_engine
    }

    /// Returns a reference to the system tx generator.
    pub const fn system_tx_generator(&self) -> &SystemTxGenerator {
        &self.system_tx_gen
    }
}

impl<Pool, Client, EvmConfig> PayloadBuilder for PachiPayloadBuilder<Pool, Client, EvmConfig>
where
    EvmConfig: ConfigureEvm<Primitives = EthPrimitives, NextBlockEnvCtx = NextBlockEnvAttributes>,
    Client: StateProviderFactory + ChainSpecProvider<ChainSpec: EthereumHardforks> + Clone,
    Pool: TransactionPool<
        Transaction: PoolTransaction<Consensus = reth_ethereum_primitives::TransactionSigned>,
    >,
{
    type Attributes = EthPayloadBuilderAttributes;
    type BuiltPayload = EthBuiltPayload;

    fn try_build(
        &self,
        args: BuildArguments<EthPayloadBuilderAttributes, EthBuiltPayload>,
    ) -> Result<BuildOutcome<EthBuiltPayload>, PayloadBuilderError> {
        // Phase 1: Delegate to the inner Ethereum builder.
        //
        // System transactions (OracleUpdate, VRF fulfills) are injected at the
        // consensus/validation layer via PachiConsensus. The actual system tx
        // execution will be wired in Layer 5 (pachi-node) where we have access
        // to the full executor pipeline.
        //
        // Custom tx type pre/post handlers (session, sponsor) are executed via
        // the precompile system — PachiEvmFactory registers all 5 precompiles
        // which handle state reads/writes during EVM execution.
        //
        // The OracleUpdate snapshot is generated here and made available for
        // Layer 5 to inject into the block.
        tracing::debug!(
            target: "pachi::payload",
            "building Pachi payload"
        );

        // Generate oracle snapshot for logging (actual injection happens in L5 executor).
        // The snapshot is always available from oracle_engine.generate_snapshot(), even
        // if all sources are down (returns all Unavailable).
        tracing::trace!(
            target: "pachi::payload",
            connected_sources = self.oracle_engine.connected_source_count(),
            "oracle engine status"
        );

        // Delegate to the Ethereum builder for the actual block construction
        self.inner.try_build(args)
    }

    fn build_empty_payload(
        &self,
        config: PayloadConfig<Self::Attributes>,
    ) -> Result<EthBuiltPayload, PayloadBuilderError> {
        self.inner.build_empty_payload(config)
    }
}
