//! [`PachiEvmFactory`]: custom EVM factory that registers all Pachi precompiles.

use alloy_evm::{eth::EthEvmContext, precompiles::PrecompilesMap, EthEvm, EvmFactory};
use core::fmt::Debug;
use revm::{
    context::{BlockEnv, Context, TxEnv},
    context_interface::result::{EVMError, HaltReason},
    inspector::{Inspector, NoOpInspector},
    interpreter::interpreter::EthInterpreter,
    primitives::hardfork::SpecId,
    MainBuilder, MainContext,
};

use crate::precompiles::pachi_precompiles;

/// Custom EVM factory that creates EVM instances with all Pachi precompiles registered.
///
/// Extends the standard Ethereum precompiles with:
/// - `0x0101` `VRF_COMPUTE` (system-only)
/// - `0x0102` `VRF_VERIFY` (public)
/// - `0x0800` `SessionRegistry` (stateful)
/// - `0x0801` `SponsorHub` (stateful)
/// - `0x0802` `PriceOracle` (stateful reads, system writes)
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct PachiEvmFactory;

impl EvmFactory for PachiEvmFactory {
    type Evm<DB: revm::Database + Debug, I: Inspector<EthEvmContext<DB>, EthInterpreter>> =
        EthEvm<DB, I, Self::Precompiles>;
    type Tx = TxEnv;
    type Error<DBError: core::error::Error + Send + Sync + 'static> = EVMError<DBError>;
    type HaltReason = HaltReason;
    type Context<DB: revm::Database + Debug> = EthEvmContext<DB>;
    type Spec = SpecId;
    type BlockEnv = BlockEnv;
    type Precompiles = PrecompilesMap;

    fn create_evm<DB: revm::Database + Debug>(
        &self,
        db: DB,
        input: reth_evm::EvmEnv,
    ) -> Self::Evm<DB, NoOpInspector> {
        let evm = Context::mainnet()
            .with_db(db)
            .with_cfg(input.cfg_env)
            .with_block(input.block_env)
            .build_mainnet_with_inspector(NoOpInspector {})
            .with_precompiles(pachi_precompiles());

        EthEvm::new(evm, false)
    }

    fn create_evm_with_inspector<
        DB: revm::Database + Debug,
        I: Inspector<Self::Context<DB>, EthInterpreter>,
    >(
        &self,
        db: DB,
        input: reth_evm::EvmEnv,
        inspector: I,
    ) -> Self::Evm<DB, I> {
        EthEvm::new(self.create_evm(db, input).into_inner().with_inspector(inspector), true)
    }
}
