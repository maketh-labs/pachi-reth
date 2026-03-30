//! [`PachiEvmConfig`]: implements [`ConfigureEvm`] for Pachi Chain.
//!
//! Wraps the standard Ethereum block executor and block assembler with [`PachiEvmFactory`],
//! which registers all Pachi precompiles into the EVM.

extern crate alloc;

use alloc::{borrow::Cow, sync::Arc};
use alloy_consensus::Header;
use alloy_evm::eth::{spec::EthExecutorSpec, EthBlockExecutionCtx, EthBlockExecutorFactory};
use core::convert::Infallible;
use reth_chainspec::{ChainSpec, EthChainSpec};
use reth_ethereum_forks::Hardforks;
use reth_ethereum_primitives::{Block, EthPrimitives};
use reth_evm::{eth::NextEvmEnvAttributes, ConfigureEvm, EvmEnv, NextBlockEnvAttributes};
use reth_primitives_traits::{SealedBlock, SealedHeader};
use revm::primitives::hardfork::SpecId;

use crate::factory::PachiEvmFactory;

// Re-export from reth-evm-ethereum. We use the workspace dependency for this.
use reth_evm_ethereum::{EthBlockAssembler, RethReceiptBuilder};

/// Pachi Chain EVM configuration.
///
/// Reuses the standard Ethereum block executor and assembler, but swaps in
/// [`PachiEvmFactory`] which extends the precompile set with all Pachi precompiles.
#[derive(Debug, Clone)]
pub struct PachiEvmConfig<C = ChainSpec> {
    /// Inner block executor factory with Pachi precompiles.
    pub executor_factory: EthBlockExecutorFactory<RethReceiptBuilder, Arc<C>, PachiEvmFactory>,
    /// Ethereum block assembler (unchanged from Ethereum).
    pub block_assembler: EthBlockAssembler<C>,
}

impl<C> PachiEvmConfig<C> {
    /// Creates a new Pachi EVM configuration with the given chain spec.
    pub fn new(chain_spec: Arc<C>) -> Self {
        Self {
            block_assembler: EthBlockAssembler::new(chain_spec.clone()),
            executor_factory: EthBlockExecutorFactory::new(
                RethReceiptBuilder::default(),
                chain_spec,
                PachiEvmFactory::default(),
            ),
        }
    }

    /// Returns the chain spec.
    pub const fn chain_spec(&self) -> &Arc<C> {
        self.executor_factory.spec()
    }
}

impl<C> ConfigureEvm for PachiEvmConfig<C>
where
    C: EthExecutorSpec + EthChainSpec<Header = Header> + Hardforks + 'static,
{
    type Primitives = EthPrimitives;
    type Error = Infallible;
    type NextBlockEnvCtx = NextBlockEnvAttributes;
    type BlockExecutorFactory =
        EthBlockExecutorFactory<RethReceiptBuilder, Arc<C>, PachiEvmFactory>;
    type BlockAssembler = EthBlockAssembler<C>;

    fn block_executor_factory(&self) -> &Self::BlockExecutorFactory {
        &self.executor_factory
    }

    fn block_assembler(&self) -> &Self::BlockAssembler {
        &self.block_assembler
    }

    fn evm_env(&self, header: &Header) -> Result<EvmEnv<SpecId>, Self::Error> {
        Ok(EvmEnv::for_eth_block(
            header,
            self.chain_spec(),
            self.chain_spec().chain().id(),
            self.chain_spec().blob_params_at_timestamp(header.timestamp),
        ))
    }

    fn next_evm_env(
        &self,
        parent: &Header,
        attributes: &NextBlockEnvAttributes,
    ) -> Result<EvmEnv, Self::Error> {
        Ok(EvmEnv::for_eth_next_block(
            parent,
            NextEvmEnvAttributes {
                timestamp: attributes.timestamp,
                suggested_fee_recipient: attributes.suggested_fee_recipient,
                prev_randao: attributes.prev_randao,
                gas_limit: attributes.gas_limit,
            },
            self.chain_spec().next_block_base_fee(parent, attributes.timestamp).unwrap_or_default(),
            self.chain_spec(),
            self.chain_spec().chain().id(),
            self.chain_spec().blob_params_at_timestamp(attributes.timestamp),
        ))
    }

    fn context_for_block<'a>(
        &self,
        block: &'a SealedBlock<Block>,
    ) -> Result<EthBlockExecutionCtx<'a>, Self::Error> {
        Ok(EthBlockExecutionCtx {
            tx_count_hint: Some(block.transaction_count()),
            parent_hash: block.header().parent_hash,
            parent_beacon_block_root: block.header().parent_beacon_block_root,
            ommers: &block.body().ommers,
            withdrawals: block.body().withdrawals.as_ref().map(Cow::Borrowed),
            extra_data: block.header().extra_data.clone(),
        })
    }

    fn context_for_next_block(
        &self,
        parent: &SealedHeader,
        attributes: Self::NextBlockEnvCtx,
    ) -> Result<EthBlockExecutionCtx<'_>, Self::Error> {
        Ok(EthBlockExecutionCtx {
            tx_count_hint: None,
            parent_hash: parent.hash(),
            parent_beacon_block_root: attributes.parent_beacon_block_root,
            ommers: &[],
            withdrawals: attributes.withdrawals.map(Cow::Owned),
            extra_data: attributes.extra_data,
        })
    }
}
