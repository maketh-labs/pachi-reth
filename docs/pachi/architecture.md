# Pachi Chain Architecture

## Overview

Pachi Chain is a custom EVM-compatible L1 built on reth, targeting blockchain-based gambling infrastructure. It extends standard Ethereum with three native features:

1. **Native Price Oracle** -- Precompile-based price feeds from CEX WebSockets
2. **VRF (Verifiable Random Function)** -- Cryptographically provable randomness for games
3. **Session Key + Gas Sponsor** -- Web2-grade UX with delegated signing and gas sponsorship

All features are implemented as **parallel crates** under `crates/pachi/`, leveraging reth's trait-based modularity without modifying core reth code.

---

## Crate Structure

```
crates/pachi/
|
+-- primitives/              Shared types across all Pachi features
|   Types: Limit, Constraint, CallPolicy, TransferPolicy,
|          AssetId, Confidence, SystemAddress, PachiSystemTx
|
+-- oracle/
|   +-- engine/              Oracle Engine subsystem
|   |   - WebSocket connections to 5 CEXs (Binance, Coinbase, OKX, Bybit, Kraken)
|   |   - Aggregator (filter -> trim -> median)
|   |   - Local Price State management
|   |   - PriceSnapshot generation for block building
|   +-- precompile/          PriceOracle precompile (0x0802)
|       - getPrice, getPriceBatch, isSupported
|       - State storage read/write
|       - OracleUpdate application logic
|
+-- vrf/
|   +-- core/                Pure ECVRF cryptography
|   |   - secp256k1 VRF compute/verify (using k256 crate)
|   |   - No I/O, fully unit-testable
|   +-- precompile/          VRF precompiles
|   |   - 0x0101 VRF_COMPUTE (system-only)
|   |   - 0x0102 VRF_VERIFY (public)
|   +-- dealer/              Dealer contract state machine
|       - requestVRF / fulfill / getResult
|       - Gas prepayment and refund logic
|       - Request tracking and key derivation
|
+-- session/
|   +-- types/               Session-specific types
|   |   - SessionConfig, SessionRecord, slot management
|   +-- precompile/          SessionRegistry precompile (0x0800)
|       - createSession, revokeSession, getSession, etc.
|       - Session nonce management
|
+-- sponsor/
|   +-- types/               Sponsor-specific types
|   |   - SponsorConfig, SponsorRecord, governance types
|   +-- precompile/          SponsorHub precompile (0x0801)
|       - registerPolicy, deposit, withdraw
|       - approveMint, revokeMint, transferOwnership
|       - Dual-mode settlement (Mint vs Deposit)
|
+-- evm/                     Pachi EVM configuration
|   - PachiEvmFactory: registers all precompiles into PrecompilesMap
|   - Pre-execution handler: session validation, sponsor balance lock
|   - Post-execution handler: limit updates, sponsor settlement, minting
|   - Extends ConfigureEvm trait
|
+-- tx/                      Custom transaction types
|   - 0x04 SessionTx: RLP encoding/decoding
|   - 0x05 SponsoredTx: RLP encoding/decoding
|   - 0x06 SessionSponsoredTx: RLP encoding/decoding
|   - 0x50 PachiSystemTx: system transaction type
|   - Custom EIP-2718 envelope
|
+-- payload/                 Block building
|   - PachiPayloadBuilder: wraps EthereumPayloadBuilder
|   - After user tx execution: scan for VRF request events
|   - Generate VRF fulfill system txs
|   - Generate OracleUpdate system tx (every block)
|   - Inject system txs at block end
|
+-- consensus/               Block validation
|   - PachiConsensus: wraps EthBeaconConsensus
|   - Validates OracleUpdate presence and correctness
|   - Validates VRF fulfill ordering
|   - Validates custom tx type rules
|
+-- pool/                    Transaction pool extensions
|   - SessionTx mempool validation (policy checks)
|   - SponsoredTx mempool validation (sponsor balance, policy)
|   - Session nonce ordering
|
+-- hardforks/               Pachi hardfork definitions
|   - Phase 1 activation block
|   - Phase 2 activation block
|   - Future feature activation blocks
|
+-- rpc/                     JSON-RPC extensions
|   - session_* namespace (prepareCreate, getSession, listSessions, simulateTx)
|   - sponsor_* namespace (getInfo, canSponsor, estimateGas)
|   - oracle_* namespace (getPrice, getStatus)
|
+-- node/                    Node assembly
|   - PachiNode: NodeTypes implementation
|   - PachiExecutorBuilder: configures PachiEvmFactory
|   - PachiPayloadBuilder: injects system txs
|   - PachiConsensusBuilder: custom validation
|   - Component wiring via reth's ComponentsBuilder
|
+-- genesis/                 Genesis configuration
    - Pachi-specific genesis fields
    - Initial precompile state setup
    - Session/Sponsor parameter defaults
```

---

## Dependency Graph (Implementation Order)

```
Layer 0: Foundation (no dependencies)
  primitives  |  hardforks  |  vrf/core

Layer 1: Precompile State (depends on: primitives)
  oracle/precompile  |  vrf/precompile  |  session/precompile  |  sponsor/precompile

Layer 2: Transaction Types (depends on: primitives)
  tx (0x04, 0x05, 0x06, 0x50)

Layer 3: EVM Integration (depends on: Layer 1 + Layer 2)
  evm (PachiEvmFactory + handlers)

Layer 4: Block Building & Validation (depends on: Layer 3)
  oracle/engine  |  payload  |  consensus  |  pool

Layer 5: Node Assembly (depends on: all layers)
  node  |  rpc  |  genesis
```

Each layer must be completed before the next can begin. Within a layer, crates are independent and can be developed in parallel.

---

## Integration Points with reth

| reth Trait | Pachi Implementation | Purpose |
|-----------|---------------------|---------|
| `ConfigureEvm` | `PachiEvmConfig` | Register precompiles, configure handlers |
| `PayloadBuilder` | `PachiPayloadBuilder` | Inject system txs into blocks |
| `FullConsensus` + `Consensus` + `HeaderValidator` | `PachiConsensus` | Custom block validation rules |
| `NodeTypes` | `PachiNode` | Type aliases for Pachi primitives |
| `ExecutorBuilder` | `PachiExecutorBuilder` | Provide PachiEvmConfig |
| `PayloadServiceBuilder` | `PachiPayloadServiceBuilder` | Create payload builder service |
| `ConsensusBuilder` | `PachiConsensusBuilder` | Create consensus engine |

---

## Precompile Address Map

| Address | Name | Access | Feature |
|---------|------|--------|---------|
| `0x0101` | VRF_COMPUTE | System-only | VRF |
| `0x0102` | VRF_VERIFY | Public | VRF |
| `0x0800` | SessionRegistry | Public | Session Key |
| `0x0801` | SponsorHub | Public (governance functions owner-only) | Gas Sponsor |
| `0x0802` | PriceOracle | Public (state writes system-only) | Oracle |

---

## Transaction Type Map

| Type | Name | Feature |
|------|------|---------|
| `0x04` | SessionTx | Session Key |
| `0x05` | SponsoredTx | Gas Sponsor |
| `0x06` | SessionSponsoredTx | Session Key + Gas Sponsor |
| `0x50` | PachiSystemTx | Oracle Update, VRF Fulfill |

---

## Phase Overview

### Phase 1 (Testnet)
- Single sequencer, reth dev mode (auto-seal, 1s blocks)
- Single VRF key held in process
- Sequencer trust model (sequencer = house)
- All features fully functional
- Full nodes validate but cannot reject blocks

### Phase 2 (Mainnet)
- IBFT2 multi-sequencer (3-7 nodes)
- DKG + Threshold VRF
- Slashing system
- Full consensus-based price validation
- Seasonal key rotation
