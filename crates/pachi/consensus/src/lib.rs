//! Pachi Chain consensus validation.
//!
//! Wraps [`EthBeaconConsensus`] with additional block validation rules:
//! - `OracleUpdate` system tx must exist in every block
//! - All supported assets present, no unsupported assets
//! - Engine version match
//! - VRF fulfill ordering (each fulfill after its request)
//! - Custom tx type structural validation

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod error;
mod validation;

pub use error::PachiConsensusError;
pub use validation::{
    validate_oracle_update, validate_system_tx_ordering, validate_vrf_fulfill_ordering,
};

use alloc::fmt::Debug;
use reth_chainspec::EthChainSpec;
use reth_consensus::{
    Consensus, ConsensusError, FullConsensus, HeaderValidator, ReceiptRootBloom, TransactionRoot,
};
use reth_ethereum_consensus::EthBeaconConsensus;
use reth_ethereum_forks::EthereumHardforks;
use reth_execution_types::BlockExecutionResult;
use reth_primitives_traits::{
    Block, BlockBody, BlockHeader, NodePrimitives, RecoveredBlock, SealedBlock, SealedHeader,
};
use std::sync::Arc;

extern crate alloc;

/// Pachi consensus engine wrapping [`EthBeaconConsensus`] with custom validation rules.
#[derive(Debug, Clone)]
pub struct PachiConsensus<ChainSpec> {
    /// Inner Ethereum consensus.
    inner: EthBeaconConsensus<ChainSpec>,
}

impl<ChainSpec> PachiConsensus<ChainSpec>
where
    ChainSpec: EthChainSpec + EthereumHardforks,
{
    /// Creates a new [`PachiConsensus`] wrapping the Ethereum beacon consensus.
    pub const fn new(chain_spec: Arc<ChainSpec>) -> Self {
        Self { inner: EthBeaconConsensus::new(chain_spec) }
    }
}

impl<ChainSpec, N> FullConsensus<N> for PachiConsensus<ChainSpec>
where
    ChainSpec: Send + Sync + EthChainSpec<Header = N::BlockHeader> + EthereumHardforks + Debug,
    N: NodePrimitives,
{
    fn validate_block_post_execution(
        &self,
        block: &RecoveredBlock<N::Block>,
        result: &BlockExecutionResult<N::Receipt>,
        receipt_root_bloom: Option<ReceiptRootBloom>,
    ) -> Result<(), ConsensusError> {
        <EthBeaconConsensus<ChainSpec> as FullConsensus<N>>::validate_block_post_execution(
            &self.inner,
            block,
            result,
            receipt_root_bloom,
        )
    }
}

impl<B, ChainSpec> Consensus<B> for PachiConsensus<ChainSpec>
where
    B: Block,
    ChainSpec: EthChainSpec<Header = B::Header> + EthereumHardforks + Debug + Send + Sync,
{
    fn validate_body_against_header(
        &self,
        body: &B::Body,
        header: &SealedHeader<B::Header>,
    ) -> Result<(), ConsensusError> {
        <EthBeaconConsensus<ChainSpec> as Consensus<B>>::validate_body_against_header(
            &self.inner,
            body,
            header,
        )
    }

    fn validate_block_pre_execution(&self, block: &SealedBlock<B>) -> Result<(), ConsensusError> {
        <EthBeaconConsensus<ChainSpec> as Consensus<B>>::validate_block_pre_execution(
            &self.inner,
            block,
        )?;

        // Pachi-specific validation on the raw transactions
        let txs = block.body().transactions();
        validation::validate_pachi_block_pre_execution(txs)
    }

    fn validate_block_pre_execution_with_tx_root(
        &self,
        block: &SealedBlock<B>,
        transaction_root: Option<TransactionRoot>,
    ) -> Result<(), ConsensusError> {
        <EthBeaconConsensus<ChainSpec> as Consensus<B>>::validate_block_pre_execution_with_tx_root(
            &self.inner,
            block,
            transaction_root,
        )?;

        let txs = block.body().transactions();
        validation::validate_pachi_block_pre_execution(txs)
    }
}

impl<H, ChainSpec> HeaderValidator<H> for PachiConsensus<ChainSpec>
where
    H: BlockHeader,
    ChainSpec: EthChainSpec<Header = H> + EthereumHardforks + Debug + Send + Sync,
{
    fn validate_header(&self, header: &SealedHeader<H>) -> Result<(), ConsensusError> {
        self.inner.validate_header(header)
    }

    fn validate_header_against_parent(
        &self,
        header: &SealedHeader<H>,
        parent: &SealedHeader<H>,
    ) -> Result<(), ConsensusError> {
        self.inner.validate_header_against_parent(header, parent)
    }
}
