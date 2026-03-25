# Native Price Oracle System

## Overview

Pachi Chain provides native price data for major assets via a precompile-based system call. Every node runs an identical **Oracle Engine** that collects and aggregates prices, and the block proposer includes a price snapshot in each block. Smart contracts read prices with a single precompile call.

**Design principles:**

- **Deterministic Engine:** Same source data produces the same result. Inter-node differences only arise from WebSocket receive timing; IBFT2 2/3+ consensus verifies this (Phase 2).
- **Chain Liveness First:** Block production never stops regardless of source failures or network issues. Quality degradation is expressed via `confidence`, and consumer contracts decide their own thresholds.
- **Phase 1 to 2 Evolution:** Phase 1 uses a single-sequencer trust model; Phase 2 uses IBFT2 consensus-based validation.

---

## Supported Assets

Hard-coded. Additions require a hard fork.

| Asset ID | Pair     | Notes            |
| -------- | -------- | ---------------- |
| `0x01`   | BTC/USD  |                  |
| `0x02`   | ETH/USD  |                  |
| `0x03`   | SOL/USD  |                  |
| `0x04`   | USDC/USD | Depeg detection  |
| `0x05`   | USDT/USD | Depeg detection  |
| `0x06`   | DAI/USD  | Depeg detection  |

**Price representation:** `uint256`, 8 decimal precision (industry standard, matching Chainlink).

**TWAP:** Not provided at the protocol level. Contracts that need TWAP should record spot prices per block and compute it themselves.

---

## Oracle Engine

### Architecture

The Oracle Engine is a subsystem embedded in the reth node. **All nodes (sequencer, full nodes)** run the same engine.

```
+----------------------------------------------+
|                  Pachi Node                   |
|                                               |
|  +---------------------------------------+   |
|  |            Oracle Engine               |   |
|  |                                        |   |
|  |  +--------+ +--------+ +--------+     |   |
|  |  |Binance | |Coinbase| |  ...N  |     |   |
|  |  | (WS)   | | (WS)   | |        |     |   |
|  |  +---+----+ +---+----+ +---+----+     |   |
|  |      +------+---+----------+          |   |
|  |             v                          |   |
|  |        Aggregator                      |   |
|  |     (filter -> trim -> median)         |   |
|  |             |                          |   |
|  |             v                          |   |
|  |      Local Price State                 |   |
|  |   { asset -> AssetState }              |   |
|  +-------------+-------------------------+   |
|                |                             |
|    +-----------+-----------+                 |
|    v                       v                 |
| Block Builder           Validator            |
| (propose snapshot)   (verify snapshot)       |
+----------------------------------------------+
```

### Data Sources

Hard-coded in protocol config. Changes require a hard fork.

| Source   | Type      | Coverage                      |
| -------- | --------- | ----------------------------- |
| Binance  | WebSocket | Global highest volume         |
| Coinbase | WebSocket | USD pair reference            |
| OKX      | WebSocket | Asia                          |
| Bybit    | WebSocket | Derivatives reflection        |
| Kraken   | WebSocket | Europe                        |

All are large CEXs with sufficient source reliability. Median-based aggregation across 5 sources defends against single-source anomalies, assuming a majority are honest.

### Aggregation Algorithm

Determinism is critical. Every step must be deterministic to guarantee identical input produces identical output.

```rust
fn aggregate(asset_id: u8, raw: Vec<(Source, Price, Timestamp)>) -> AssetState {
    // 1. Staleness filter
    let fresh: Vec<_> = raw.iter()
        .filter(|(_, _, ts)| now() - ts <= STALENESS_THRESHOLD)
        .collect();

    // 2. Active source count check
    if fresh.is_empty() {
        return AssetState::Unavailable;
    }

    if fresh.len() < MIN_ACTIVE_SOURCES {
        // Data exists but insufficient sources -> compute but mark Degraded
        let mut prices: Vec<_> = fresh.iter().map(|(_, p, _)| p).collect();
        prices.sort();
        let median = prices[prices.len() / 2];
        return AssetState::Available {
            price: median,
            timestamp: fresh.iter().map(|(_, _, ts)| ts).max(),
            confidence: Confidence::Degraded,
        };
    }

    // 3. Normal path: sufficient sources
    let mut prices: Vec<_> = fresh.iter().map(|(_, p, _)| p).collect();
    prices.sort();

    // 4. Outlier removal: trim highest/lowest if 5+ sources
    if prices.len() >= 5 {
        prices = prices[1..prices.len()-1].to_vec();
    }

    // 5. Median
    let median = prices[prices.len() / 2];

    // 6. Spread-based confidence
    let spread_bps = (prices.last().unwrap() - prices.first().unwrap()) * 10000 / median;
    let confidence = if spread_bps <= SPREAD_HIGH_BPS {
        Confidence::High
    } else if spread_bps <= SPREAD_MEDIUM_BPS {
        Confidence::Medium
    } else {
        Confidence::Degraded
    };

    AssetState::Available { price: median, timestamp: max_ts, confidence }
}
```

**Why median:** Mean is vulnerable to single outliers and can introduce non-determinism in integer division. Median is guaranteed accurate when a majority of sources are honest, and is perfectly deterministic via sorted-array index access.

### AssetState

```rust
enum AssetState {
    Available {
        price: u256,              // 8 decimals
        timestamp: u64,           // unix seconds
        confidence: Confidence,   // High | Medium | Degraded
    },
    Unavailable,                  // All sources down, no price available
}

enum Confidence {
    High = 0,       // spread <= 10bps, sources >= 3
    Medium = 1,     // spread <= 50bps, sources >= 3
    Degraded = 2,   // spread > 50bps OR sources < 3
}
```

### Constants

```
STALENESS_THRESHOLD = 10s     // Ignore source data older than this
HEARTBEAT_INTERVAL  = 1s      // Regular aggregation cycle
MIN_ACTIVE_SOURCES  = 3       // Below this -> Degraded
SPREAD_HIGH_BPS     = 10      // High confidence upper bound
SPREAD_MEDIUM_BPS   = 50      // Medium confidence upper bound
```

---

## Block Integration

### PriceSnapshot

```rust
struct PriceSnapshot {
    entries: Map<AssetId, SnapshotEntry>,
    engine_version: u16,
}

enum SnapshotEntry {
    Updated {
        price: u256,
        timestamp: u64,
        confidence: Confidence,
    },
    Unavailable,
}
```

### OracleUpdate System Transaction

**OracleUpdate is mandatory in every block. A block without it is invalid.**

System transaction properties:
- `from`: `SYSTEM_ADDRESS (0x0...0)`
- Zero gas cost
- Cannot be submitted by users

**Why mandatory:** If OracleUpdate were optional, a proposer could intentionally omit it to maintain stale prices. In a gambling context, this directly enables profit manipulation.

**Blocks are produced even during total source failure:**

```
// Worst case: all sources down
OracleUpdate {
    snapshot: {
        BTC: Unavailable, ETH: Unavailable, SOL: Unavailable,
        USDC: Unavailable, USDT: Unavailable, DAI: Unavailable,
    },
    engine_version: 1,
}
// -> Block is valid. Chain does not stop.
```

In this case, the precompile retains the last valid price for each asset but returns `confidence = 3 (Unavailable)` on query.

**Genesis:** All assets start at `(0, 0, Unavailable)`. The first block's OracleUpdate naturally fills prices.

### Block Validation Rules

```rust
fn validate_oracle_update(block: &Block) -> Result<(), ValidationError> {
    let update = block.oracle_update();

    // Rule 1: OracleUpdate must exist
    if update.is_none() { return Err(MissingOracleUpdate) }

    // Rule 2: All supported assets must be included
    for asset_id in SUPPORTED_ASSETS {
        if !update.snapshot.entries.contains_key(&asset_id) {
            return Err(MissingAsset { asset_id })
        }
    }

    // Rule 3: No unsupported assets
    for asset_id in update.snapshot.entries.keys() {
        if !SUPPORTED_ASSETS.contains(asset_id) {
            return Err(UnsupportedAsset { asset_id })
        }
    }

    // Rule 4: Engine version match
    if update.engine_version != CURRENT_ENGINE_VERSION {
        return Err(EngineVersionMismatch)
    }

    // Rule 5: Per-asset validation
    for (asset_id, entry) in &update.snapshot.entries {
        match entry {
            Updated { price, timestamp, confidence } =>
                validate_updated(asset_id, price, timestamp, confidence, block)?,
            Unavailable =>
                validate_unavailable(asset_id)?,
        }
    }
    Ok(())
}
```

**`VALIDATION_TOLERANCE_BPS = 100` (1%):** Nodes have slightly different WebSocket receive timing. Validation uses a wide band to catch only "obvious deviations." Natural timing-induced variance is tolerated; intentional manipulation (hundreds of bps) is rejected.

### Phase 1 vs Phase 2 Behavior

**Phase 1 (Single Sequencer):**
- Sequencer is the sole block producer
- Full nodes run `validate_oracle_update()` for obvious deviation checks
- Sequencer infrastructure failure -> Unavailable submitted, full nodes log warnings but accept

**Phase 2 (IBFT2):**
- Proposer submitting unjustified Unavailable is rejected by 2/3+ validators
- Next proposer rotates in via IBFT2 round timeout
- Global source failures pass consensus (all validators also see Unavailable)
- Individual node infrastructure issues are naturally rejected

---

## Precompile Interface

### Address

```
PriceOracle: 0x0000000000000000000000000000000000000802
```

### Functions

```solidity
interface IPriceOracle {
    /// @param assetId Asset identifier (0x01 ~ 0x06)
    /// @return price 8-decimal USD price (last valid price, or 0 if never set)
    /// @return timestamp Price reference time, unix seconds (0 if never set)
    /// @return confidence 0=High, 1=Medium, 2=Degraded, 3=Unavailable
    function getPrice(uint8 assetId)
        external view returns (uint256 price, uint64 timestamp, uint8 confidence);

    function getPriceBatch(uint8[] calldata assetIds)
        external view returns (
            uint256[] memory prices,
            uint64[] memory timestamps,
            uint8[] memory confidences
        );

    function isSupported(uint8 assetId) external view returns (bool supported);
}
```

### Gas Costs

| Function        | Gas             |
| --------------- | --------------- |
| `getPrice`      | 200             |
| `getPriceBatch` | 200 + 100 * len |
| `isSupported`   | 100             |

### Error Handling

- Unsupported `assetId` -> revert
- Unavailable asset -> does NOT revert. Returns `confidence = 3`. Gatekeeping is the consumer contract's responsibility.

---

## Safety Mechanisms

### Circuit Breaker

```rust
CIRCUIT_BREAKER_BPS    = 1000  // 10%
CIRCUIT_BREAKER_WINDOW = 5     // blocks

fn apply_circuit_breaker(
    asset_id: u8, new_price: u256, recent: &[PriceEntry]
) -> Confidence {
    for entry in recent.last(CIRCUIT_BREAKER_WINDOW) {
        let delta_bps = abs(new_price - entry.price) * 10000 / entry.price;
        if delta_bps > CIRCUIT_BREAKER_BPS {
            return Confidence::Degraded  // Force Degraded
        }
    }
    calculate_normal_confidence()
}
```

The circuit breaker does NOT block price updates. Prices are updated to the latest value, but confidence is forced to `Degraded`. In real market crashes, keeping a stale price is more dangerous than an inaccurate fresh price.

Auto-release: If price changes stay below `CIRCUIT_BREAKER_BPS` for `CIRCUIT_BREAKER_WINDOW` consecutive blocks, confidence returns to normal.

### Staleness Protection

Applied at precompile query time, independent of OracleUpdate.

```rust
MAX_PRICE_AGE = 60  // seconds

fn precompile_get_price(asset_id: u8) -> (u256, u64, u8) {
    let entry = state.prices[asset_id];
    let mut conf = entry.confidence;
    if current_block_timestamp() - entry.timestamp > MAX_PRICE_AGE {
        conf = max(conf, Confidence::Degraded)
    }
    (entry.price, entry.timestamp, conf)
}
```

---

## Precompile State Storage

```
Storage Layout (PriceOracle @ 0x0802):

// Per-asset (2 slots per asset)
slot(asset_id * 2)     -> price: uint256
slot(asset_id * 2 + 1) -> packed {
    timestamp:  uint64   [0:63]
    confidence: uint8    [64:71]
}
```

No history. Only current prices are maintained.

**OracleUpdate application logic:**

```rust
fn apply_oracle_update(snapshot: &PriceSnapshot) {
    for (asset_id, entry) in &snapshot.entries {
        match entry {
            Updated { price, timestamp, confidence } =>
                state.prices[asset_id] = PriceEntry { price, timestamp, confidence },
            Unavailable =>
                // Keep price/timestamp, only change confidence to Unavailable(3)
                state.prices[asset_id].confidence = 3,
        }
    }
}
```

---

## Engine Version Upgrades

Handled via hard fork. The permissioned network has low coordination cost. `engine_version` in blocks is checked against the node's current version; mismatches cause block rejection.

---

## Liveness Scenarios

| Scenario | OracleUpdate Content | Block | Precompile |
| --- | --- | --- | --- |
| Normal operation | All assets Updated | Valid | Latest prices, High/Medium |
| Some sources down (<3 remain) | Asset Updated + Degraded | Valid | Latest price, Degraded |
| All sources down for one asset | Asset Unavailable | Valid | Last valid price, confidence=3 |
| All sources down for all assets | All Unavailable | **Valid** | Last valid prices, all confidence=3 |
| Price spike (>10%) | Updated + Degraded (circuit breaker) | Valid | Latest price, Degraded |
| Proposer price >1% from validators | -- | **Rejected** (rotation) | -- |
| Proposer-only source failure | Unjustified Unavailable | **Rejected** (rotation) | -- |
| Post-genesis (before first update) | -- | -- | (0, 0, 3) for all |

**Key: Source failures never make a block invalid. Only proposer dishonesty or infrastructure failure causes block rejection, triggering rotation to the next proposer.**
