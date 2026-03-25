# Gas Fee Sponsor System

## Overview

Gas Sponsor enables a third party (sponsor) to pay gas costs for user transactions. Supports two modes:

```
+-------------------------------------------------------------+
|                     SponsorHub (0x0801)                       |
|                                                               |
|  +----------------------+    +-----------------------------+  |
|  |  Mint Sponsor        |    |  Deposit Sponsor            |  |
|  |                      |    |                             |  |
|  |  - Governance-approved|    |  - Anyone can register      |  |
|  |  - Gas paid by minting|    |  - Gas paid from deposit    |  |
|  |  - No deposit needed |    |  - Self-funded              |  |
|  |  - Internal/partner  |    |  - External developers      |  |
|  +----------------------+    +-----------------------------+  |
|                                                               |
|  Shared: SponsorConfig policy (same Limit/Constraint model)   |
+-------------------------------------------------------------+
```

| | Mint Sponsor | Deposit Sponsor |
|---|---|---|
| Registration | Governance (owner) approval required | Anyone, permissionless |
| Gas funding | Protocol mints native token | Sponsor's deposited funds |
| Deposit required | No | Yes |
| Exhaustible | No (policy limit is only cap) | Yes (runs out when balance depleted) |
| Target users | Internal ops + verified partners | External app developers |
| Inflation | Negligible (gas cost level) | None |

### Why Dual-mode

1. **Mint:** Infinite gas subsidy for Pachi's own apps and verified partners. Eliminates genesis pool management, buyback cycles, and token price risk.
2. **Deposit:** External developers can permissionlessly sponsor gas for their apps. Maintains ecosystem openness.

Mint minting amount is deterministically computed from actual EVM gas consumption, not at the sponsor's discretion. `globalFeeLimit` provides an additional upper bound.

### Inflation Analysis

```
Conservative assumptions:
  Average gas/tx:        200,000
  base_fee:              10 gwei
  Daily sponsored txs:   1,000,000 (very aggressive)

Daily minting: 200,000 * 10 gwei * 1,000,000 = 2 TOKEN/day = 730 TOKEN/year
For 100M total supply: 0.00073% annual inflation
```

---

## SponsorConfig (On-chain Registry)

Unlike sessions (hash-only), sponsors are few, so the full policy is stored on-chain. This eliminates the need to include policy in SponsoredTx calldata.

```rust
struct SponsorConfig {
    // -- Target restrictions --
    allowed_senders: Vec<address>,           // Empty = anyone (open sponsor)
    call_policies: Vec<SponsorCallPolicy>,   // Whitelisted contract function calls
    transfer_policies: Vec<SponsorTransferPolicy>, // Whitelisted transfers

    // -- Cost limits --
    global_fee_limit: Limit,                 // Total gas fee cap
    per_sender_fee_limit: Limit,             // Per-sender gas fee cap
    max_gas_per_tx: u256,                    // Max gas units per single tx

    // -- Frequency limits --
    global_tx_limit: Limit,                  // Total tx count cap
    per_sender_tx_limit: Limit,              // Per-sender tx count cap

    // -- Duration --
    valid_until: u64,                        // Policy expiration
}

struct SponsorCallPolicy {
    target: address,
    selector: bytes4,
    constraints: Vec<Constraint>,            // Same Constraint model as Session Key
}

struct SponsorTransferPolicy {
    target: address,
    max_value_per_tx: u256,
}
```

---

## Governance Model

SponsorHub uses a single-owner model. Owner is set at genesis.

```
Owner-only functions:
  approveMint(address sponsor)         // Deposit -> Mint
  revokeMint(address sponsor)          // Mint -> Deposit (balance = 0)
  transferOwnership(address newOwner)  // Owner change

Evolution path:
  Initial: Single EOA (fast operations)
  Growth: transferOwnership() to multisig
  Mature: Validator-vote-based governance if needed
```

---

## Gas Fee Settlement Flow

```
SponsoredTx execution flow:

1. [Pre-Execution Verification]
   +-- Lookup SponsorRecord from on-chain registry
   +-- status == Active
   +-- block.timestamp <= validUntil
   +-- allowedSenders check (skip if empty)
   +-- callPolicy / transferPolicy matching + constraint verification
   +-- globalFeeLimit, perSenderFeeLimit, globalTxLimit, perSenderTxLimit checks
   +-- tx.gas <= maxGasPerTx
   |
   +-- [Deposit Mode]
   |   +-- sponsor.balance >= max_possible_gas_cost
   |   +-- Lock max_possible_gas_cost from sponsor.balance
   |
   +-- [Mint Mode]
       +-- No balance check needed (policy limits only)

2. [Execution]
   +-- msg.sender = original sender (NOT sponsor)
   +-- tx.origin = original sender
   +-- Normal EVM execution

3. [Post-Execution Settlement]
   +-- actual_gas_cost = gas_used * effective_gas_price
   |
   +-- [Deposit Mode]
   |   +-- sponsor.balance -= actual_gas_cost
   |   +-- Unlock: max_gas_cost - actual_gas_cost returned
   |
   +-- [Mint Mode]
   |   +-- Mint actual_gas_cost of native token -> pay to sequencer
   |
   +-- globalFeeLimit usage update
   +-- perSenderFeeLimit usage update
   +-- globalTxLimit usage update (+1)
   +-- perSenderTxLimit usage update (+1)

4. [Sender Burden]
   +-- Sender pays 0 gas cost
```

---

## Precompile: SponsorHub (0x0801)

```
Address: 0x0000000000000000000000000000000000000801

-- Public Functions --

registerPolicy(SponsorConfig config)
  -> Store policy in on-chain registry
  -> Register as Deposit type
  -> Replaces previous policy if exists

deactivatePolicy()
  -> Deactivate current policy (deposit preserved)

deposit()
  -> payable: Deposit-mode sponsor funds gas sponsorship
  -> Mint-mode sponsor call reverts (unnecessary)

withdraw(uint256 amount)
  -> Deposit mode: Withdraw unlocked balance
  -> Mint mode: reverts (no balance to withdraw)

-- Owner-only (Governance) --

approveMint(address sponsor)
  -> Convert sponsor to Mint mode
  -> Sponsor must have registered policy first
  -> Existing deposit remains withdrawable

revokeMint(address sponsor)
  -> Mint -> Deposit conversion
  -> balance = 0, sponsor must deposit themselves

transferOwnership(address newOwner)
  -> Owner change

-- View Functions --

getBalance(address sponsor) -> uint256
getPolicy(address sponsor) -> SponsorConfig
getSponsorType(address sponsor) -> uint8 (0=Deposit, 1=Mint)
isActive(address sponsor) -> bool
canSponsor(address sponsor, address sender, address to, bytes4 selector) -> bool
owner() -> address

-- Events --

Deposited(address indexed sponsor, uint256 amount, uint256 newBalance)
Withdrawn(address indexed sponsor, uint256 amount, uint256 newBalance)
PolicyRegistered(address indexed sponsor, uint8 sponsorType, uint64 validUntil)
PolicyDeactivated(address indexed sponsor)
MintApproved(address indexed sponsor, address indexed approvedBy)
MintRevoked(address indexed sponsor, address indexed revokedBy)
Sponsored(address indexed sponsor, address indexed sender, bytes32 txHash, uint256 gasCost, uint8 sponsorType)
OwnershipTransferred(address indexed previousOwner, address indexed newOwner)
```

---

## Transaction Type: SponsoredTx (0x05)

Sponsor policy is on-chain, so tx does NOT include the policy.

```
SponsoredTx = 0x05 || rlp([
  chain_id,
  nonce,
  max_priority_fee_per_gas,
  max_fee_per_gas,
  gas_limit,
  to,
  value,
  data,
  access_list,

  // -- Sponsor extension field --
  sponsor,                      // address (lookup policy from on-chain registry)

  // -- Signature --
  sender_v, sender_r, sender_s
])

Gas payer: sponsor (Deposit: balance deduction / Mint: protocol minting)
msg.sender: sender
```

Additional calldata: only +20 bytes (sponsor address).

---

## On-chain Storage Layout

```
Sponsor Record:
  Key: keccak256("sponsor_record", sponsor_address)
  Value: rlp(SponsorRecord { status, sponsorType, balance, config, createdAt })

Governance:
  Key: keccak256("sponsor_hub_owner")
  Value: address

Limit State (global):
  Key: keccak256("sponsor_limit", sponsor, limit_context)
  Value: rlp(LimitState { used, lastWindow })

Limit State (per-sender):
  Key: keccak256("sponsor_sender_limit", sponsor, sender, limit_context)
  Value: rlp(LimitState { used, lastWindow })
```

---

## Gas Cost Overhead

| Item | Additional Gas | Description |
|------|---------------|-------------|
| SponsoredTx overhead (Deposit) | 20,000 | Registry lookup + policy check + balance check + settlement |
| SponsoredTx overhead (Mint) | 15,000 | Registry lookup + policy check (no balance logic) |
| Sponsor policy registration | 100,000+ | Full SponsorConfig on-chain write (size-proportional) |
| Sponsor deposit | 20,000 | Balance update |
| approveMint / revokeMint | 10,000 | sponsorType update |

---

## Security Considerations

| Threat | Mitigation |
|--------|-----------|
| Sponsor Griefing (malicious users exhaust deposit/mint limits) | perSenderFeeLimit + perSenderTxLimit cap per sender. allowedSenders whitelist |
| Deposit Sponsor insufficient balance | Pre-verify balance at mempool entry, re-verify at block building. Remove from mempool if insufficient |
| MEV-based Sponsor exploitation | maxGasPerTx caps single tx gas. globalFeeLimit caps total |
| Sybil Attack (multiple addresses bypass perSenderLimit) | globalFeeLimit + globalTxLimit as final defense. allowedSenders if needed |
| Mint Sponsor abuse (governance key theft) | Minting amount is deterministic from actual gas consumption. globalFeeLimit caps total. Owner key management (future multisig) |

---

## Genesis Configuration

```json
{
  "sponsorConfig": {
    "minDepositSponsorDeposit": "100000000000000000",
    "sponsorGasOverhead": 20000,
    "maxCallPoliciesPerSponsor": 50,
    "maxConstraintsPerCallPolicy": 10,
    "maxAllowedSenders": 10000,
    "sponsorHubAddress": "0x0000000000000000000000000000000000000801",
    "sponsorHubOwner": "0x<INITIAL_OWNER_ADDRESS>"
  }
}
```
