# VRF System

## Core Philosophy

> "The moment a user determines the seed, the result is already decided. Nobody -- not even the chain operator -- could have seen that result in advance."

Pachi Chain is blockchain-based gambling infrastructure. VRF (Verifiable Random Function) is natively embedded in the chain to provide verifiable randomness without external oracle dependency.

---

## What is VRF

A function that produces a random value with a proof that anyone can verify it was computed correctly.

```
secret_key + seed -> (random_value, proof)
```

Three properties:
- **Deterministic:** Same key + same seed always produces the same result.
- **Unpredictable:** Without the secret_key, the result cannot be predicted from the seed alone.
- **Verifiable:** Without the secret_key, anyone can verify the result using the public_key and proof.

---

## Stakeholder Structure

```
Sequencer: Block production, VRF execution
House:     Capital provider, takes opposite side of user bets
User:      Plays games
```

Phase 1: Sequencer = House (own capital, no abuse incentive).
Phase 2: Sequencer and House separable, with economic guarantees (slashing) for mutual accountability.

---

## Attack Vector Analysis

### Revert Attack
If bet and result are in the same tx, an attacker can check the result and revert if they lose.
- **Defense:** Bet (requestVRF) and result (fulfill) MUST be separate transactions.

### Sequencer Preview (Front-running)
The sequencer holds the VRF key and can precompute results before including a tx in a block.
- **Phase 1 Defense:** Sequencer trust model (same as all L2s and PerpDEXs).
- **Phase 2 Defense:** Threshold VRF (no single party can compute alone) + slashing (economic deterrence).

### Selective Censorship
Sequencer excludes bets unfavorable to the house.
- **Phase 1 Defense:** Sequencer trust + transparency dashboard for statistical anomaly detection.
- **Phase 2 Defense:** Multi-sequencer (other sequencers include the tx) + IBFT2 leader rotation.

### Seed Reuse
Repeated requests with the same seed to predict results.
- **Defense:** `final_seed = keccak256(msg.sender || user_seed)` -- msg.sender (game contract address) provides automatic namespace separation. Duplicate seeds are rejected by the Dealer contract.

### VRF Result Peeking via Revert (Phase 2)
Malicious block builder includes a reverting requestVRF tx to get partials for free.
- **Defense:** Slashing. Slashing amount >> maximum single-bet gain, making the attack economically irrational.

---

## Core Components

### Dealer Contract (Singleton, System Contract)

Single entry point for all VRF requests and results. Game contracts only interact with Dealer.

```solidity
interface IDealer {
    /// Request VRF -- called by game contracts
    /// key = keccak256(msg.sender || seed)
    /// Gas for fulfill + resolver_fee is prepaid at this point
    function requestVRF(bytes32 seed) external payable returns (bytes32 key);

    /// Fulfill VRF -- only callable by block builder (sequencer)
    /// Phase 1: Single sequencer submits full result
    /// Phase 2: Threshold partials combined into result
    function fulfill(
        bytes32 key,
        bytes32 randomValue,
        bytes calldata proof,
        bytes[] calldata partials,
        address[] calldata partialOwners
    ) external;

    /// Query result -- anyone
    function getResult(bytes32 key) external view returns (bytes32 randomValue);
    function isFulfilled(bytes32 key) external view returns (bool);
}
```

Key design principles:
- **key = keccak256(msg.sender || seed):** No betId needed; msg.sender + seed is the identifier.
- **Namespace separation:** msg.sender (game contract address) is automatic prefix.
- **Duplicate prevention:** Re-requests with the same key are rejected.
- **fulfill is builder-only:** Builder collects partials, verifies, combines, submits.
- **Interface invariant:** No interface changes from Phase 1 to Phase 2.

### VRF Precompiles

```
0x0101: VRF_COMPUTE (system-only)
  - Only callable from system account
  - Reverts if called by regular users/contracts
  - Phase 1: Single-key VRF
  - Phase 2: Threshold VRF (partial combination -> result)
  - Gas: Fixed (~3,000 gas, ecrecover level)

0x0102: VRF_VERIFY (public)
  - Result verification, safe so no access restriction
  - Input: public_key, seed, random_value, proof -> Output: valid/invalid
  - Gas: Fixed (~1,500 gas)
```

### System Transaction

```
EIP-2718 extension: type = 0x50 (PACHI_SYSTEM_TX)
- from: System account (fixed address, no private key)
- gasPrice: 0 (gas prepaid at requestVRF)
- No signature (generated directly by reth block builder)
```

Same pattern as OP Stack's L1 Attributes Depositor and Arbitrum's ArbOS.

### Block Validity Conditions

- **Order guarantee:** All fulfill txs must appear after their corresponding requestVRF tx.
- **Tx success required:** If the tx containing requestVRF reverts, the corresponding fulfill is also invalid.
- Blocks violating these conditions are rejected by other sequencers.

---

## Phase 1 -- Single Sequencer, Trust Model

### Trust Model
Same model as all L2 chains (Base, Arbitrum, Optimism) and PerpDEXs (Hyperliquid, dYdX). Sequencer = House, so no incentive to cheat against own capital. Transparency dashboard publishes all bets/results for statistical anomaly detection.

### Architecture

```
Chain: Single sequencer, reth dev mode
VRF: Single key, held in reth process
Consensus: auto-seal (1-second blocks)
VRF Algorithm: ECVRF (secp256k1) -- leveraging reth's existing k256 crate
```

### Betting Lifecycle

```
Block N:
  [...user tx: placeBet() -> Dealer.requestVRF(seed)...]
  [...system tx: Dealer.fulfill(key, random, proof, [], [])...]
  -> Same block, separate txs, request always before fulfill

User: claim() -> receive winnings if won
```

User experience: Bet -> instant result -> claim. Two txs but auto-handled by frontend.

### Block Builder Modification (reth)

```
Standard reth:
  1. Collect txs from mempool
  2. Execute txs
  3. Finalize block

Pachi reth:
  1. Collect txs from mempool
  2. Execute txs
  3. Scan executed txs for VRF request events        <- Added
  4. Generate system fulfill tx for each request     <- Added
  5. Insert system txs at end of block               <- Added
  6. Finalize block
```

### Gas Fee Structure

```
At requestVRF call time:
  - requestVRF gas: User pays directly (normal tx)
  - fulfill estimated gas + resolver_fee: Prepaid from user
  - fulfill only does Dealer storage write, no external contract calls
  - Precompile call = fixed gas (predictable)
  - Excess refunded
```

---

## Phase 2 -- Multi-Sequencer, Economic Security

### Components

1. IBFT2 consensus (BFT-based, instant finality)
2. DKG + Threshold VRF (distributed random generation)
3. Internal P2P for partial exchange (invisible to users)
4. Fulfill includes partials + owners (on-chain verification + incentives)
5. Slashing system (economic deterrence)
6. Seasonal key rotation

### Why IBFT2

Optimal for permitted small-set (3-7) sequencer model:
- Permissioned validator set fits seasonal operation model
- Instant finality essential for gambling ("bet confirmed" guarantee)
- 2/3+1 consensus tolerates f malicious nodes (from 3f+1)
- Leader rotation prevents sustained censorship
- O(n^2) message complexity is irrelevant for small sequencer count

### Betting Lifecycle (Same-Block Fulfill)

```
Block N building process:
  1. Builder (Sequencer A): Collect txs from mempool, execute
  2. Detect requestVRF events
  3. Create signed_request: {block_number, seed, signature_A}
  4. Request partials from other sequencers via internal P2P
  5. Sequencer B: Store signed_request + respond with partial_B
  6. Sequencer C: Store signed_request + respond with partial_C
  7. Builder A: Threshold reached -> combine partials -> generate VRF result
  8. Create fulfill tx (with partials + owner info)
  9. Block: [user txs...][fulfill tx]
  10. BFT consensus -> block finalized
```

### Incentive Structure

```
User: requestVRF() -> prepay gas_fee + resolver_fee

At fulfill tx execution, resolver_fee distribution:
  - Block builder: 40% (partial collection + fulfill inclusion labor)
  - Partial providers: Remaining split equally

Example (3 sequencers, resolver_fee = 100):
  Builder A: 40, Provider B: 30, Provider C: 30
```

### Slashing Conditions

1. **signed_request sent but requestVRF not in block:** Undeniable via builder's signature.
2. **requestVRF-containing tx reverted:** Partials already received, result obtained for free.

### Season Key Rotation

```
Block 99,000: Season transition announcement (on-chain event)
              -> Dealer rejects new requestVRF
Block 99,000 ~ 99,999: Grace period (fulfill all pending, auto-refund remainder)
Block 100,000: Confirm 0 pending -> destroy old key -> activate new key -> resume
```

### Performance Targets

```
              Phase 1          Phase 2 (same region)  Phase 2 (same continent)
Block time    1s               1s                     1-2s
Result wait   0s (same block)  0s (same block)        0s (same block)
Bets/sec      ~1000+           ~1000+ (batched)       ~1000+ (batched)
```

---

## Phase 1 Implementation Items

| Item | Difficulty | Description |
|------|-----------|-------------|
| VRF precompiles (0x0101, 0x0102) | ** | ECVRF secp256k1, reuse reth k256 crate |
| System account & tx type (0x50) | ** | Reference OP Stack pattern |
| Block builder event scanning | *** | reth payload builder modification, core difficulty |
| Dealer contract (precompile state) | ** | requestVRF/fulfill/getResult state machine |
| Gas prepayment logic | * | Collect fulfill gas at requestVRF time |
