//! `WebSocket` price source connectors for each supported CEX.
//!
//! Each source connects to its exchange's `WebSocket` API, subscribes to trade/ticker
//! channels for all supported assets, and forwards price updates to the aggregator.

use alloy_primitives::U256;
use futures::{SinkExt, StreamExt};
use pachi_primitives::AssetId;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// Identifies a price data source (CEX).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceId {
    /// Binance — global highest volume.
    Binance,
    /// Coinbase — USD pair reference.
    Coinbase,
    /// OKX — Asia coverage.
    Okx,
    /// Bybit — derivatives reflection.
    Bybit,
    /// Kraken — Europe coverage.
    Kraken,
}

impl SourceId {
    /// All sources in order.
    pub const ALL: [Self; 5] =
        [Self::Binance, Self::Coinbase, Self::Okx, Self::Bybit, Self::Kraken];

    /// Returns the `WebSocket` endpoint URL.
    pub const fn ws_url(&self) -> &'static str {
        match self {
            Self::Binance => "wss://stream.binance.com:9443/ws",
            Self::Coinbase => "wss://ws-feed.exchange.coinbase.com",
            Self::Okx => "wss://ws.okx.com:8443/ws/v5/public",
            Self::Bybit => "wss://stream.bybit.com/v5/public/spot",
            Self::Kraken => "wss://ws.kraken.com/v2",
        }
    }
}

impl std::fmt::Display for SourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Binance => write!(f, "Binance"),
            Self::Coinbase => write!(f, "Coinbase"),
            Self::Okx => write!(f, "OKX"),
            Self::Bybit => write!(f, "Bybit"),
            Self::Kraken => write!(f, "Kraken"),
        }
    }
}

/// A single price update from a source.
#[derive(Debug, Clone)]
pub struct PriceUpdate {
    /// Which source produced this update.
    pub source: SourceId,
    /// Asset this price is for.
    pub asset: AssetId,
    /// Price in 8-decimal USD representation.
    pub price: U256,
    /// Unix timestamp of when this price was observed (seconds).
    pub timestamp: u64,
}

/// Message sent from a source connector to the engine.
#[derive(Debug)]
pub enum SourceMessage {
    /// A price update from a source.
    Price(PriceUpdate),
    /// Source disconnected (will attempt reconnect).
    Disconnected(SourceId),
    /// Source reconnected.
    Connected(SourceId),
}

/// Configuration for a source connector.
#[derive(Debug, Clone)]
pub struct Source {
    /// Source identifier.
    pub id: SourceId,
    /// Maximum reconnection backoff duration.
    pub max_backoff: Duration,
}

impl Source {
    /// Creates a new source with default backoff.
    pub const fn new(id: SourceId) -> Self {
        Self { id, max_backoff: Duration::from_secs(30) }
    }
}

/// Manages a `WebSocket` connection to a single CEX, handling reconnection with backoff.
#[derive(Debug)]
pub struct SourceConnector {
    source: Source,
    tx: mpsc::Sender<SourceMessage>,
}

impl SourceConnector {
    /// Creates a new connector.
    pub const fn new(source: Source, tx: mpsc::Sender<SourceMessage>) -> Self {
        Self { source, tx }
    }

    /// Runs the connector loop (connect → subscribe → read → reconnect on failure).
    pub async fn run(self) {
        let mut backoff = Duration::from_millis(500);

        loop {
            match self.connect_and_stream().await {
                Ok(()) => {
                    // Clean disconnect, reset backoff
                    backoff = Duration::from_millis(500);
                }
                Err(e) => {
                    tracing::warn!(
                        target: "pachi::oracle",
                        source = %self.source.id,
                        error = %e,
                        "source connection failed, reconnecting"
                    );
                }
            }

            let _ = self.tx.send(SourceMessage::Disconnected(self.source.id)).await;
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(self.source.max_backoff);
        }
    }

    /// Connects, subscribes, and streams prices until disconnection.
    async fn connect_and_stream(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (mut ws, _) = tokio_tungstenite::connect_async(self.source.id.ws_url()).await?;

        let _ = self.tx.send(SourceMessage::Connected(self.source.id)).await;
        tracing::info!(
            target: "pachi::oracle",
            source = %self.source.id,
            "connected"
        );

        // Send subscription message
        let sub_msg = self.build_subscribe_message();
        ws.send(Message::Text(sub_msg.into())).await?;

        // Read messages
        while let Some(msg) = ws.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Some(updates) = self.parse_message(&text) {
                        for update in updates {
                            if self.tx.send(SourceMessage::Price(update)).await.is_err() {
                                return Ok(());
                            }
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    let _ = ws.send(Message::Pong(data)).await;
                }
                Ok(Message::Close(_)) => break,
                Err(e) => return Err(e.into()),
                _ => {}
            }
        }

        Ok(())
    }

    /// Builds the exchange-specific subscription message.
    fn build_subscribe_message(&self) -> String {
        match self.source.id {
            SourceId::Binance => {
                let streams: Vec<String> =
                    Self::binance_symbols().iter().map(|s| format!("\"{}@trade\"", s)).collect();
                format!(r#"{{"method":"SUBSCRIBE","params":[{}],"id":1}}"#, streams.join(","))
            }
            SourceId::Coinbase => {
                let product_ids: Vec<String> =
                    Self::coinbase_products().iter().map(|s| format!("\"{}\"", s)).collect();
                format!(
                    r#"{{"type":"subscribe","channels":["ticker"],"product_ids":[{}]}}"#,
                    product_ids.join(",")
                )
            }
            SourceId::Okx => {
                let args: Vec<String> = Self::okx_inst_ids()
                    .iter()
                    .map(|s| format!(r#"{{"channel":"tickers","instId":"{}"}}"#, s))
                    .collect();
                format!(r#"{{"op":"subscribe","args":[{}]}}"#, args.join(","))
            }
            SourceId::Bybit => {
                let args: Vec<String> =
                    Self::bybit_symbols().iter().map(|s| format!("\"tickers.{}\"", s)).collect();
                format!(r#"{{"op":"subscribe","args":[{}]}}"#, args.join(","))
            }
            SourceId::Kraken => {
                let pairs: Vec<String> =
                    Self::kraken_pairs().iter().map(|s| format!("\"{}\"", s)).collect();
                format!(
                    r#"{{"method":"subscribe","params":{{"channel":"ticker","symbol":[{}]}}}}"#,
                    pairs.join(",")
                )
            }
        }
    }

    /// Parses an exchange-specific message into price updates.
    fn parse_message(&self, text: &str) -> Option<Vec<PriceUpdate>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        match self.source.id {
            SourceId::Binance => Self::parse_binance(text, now),
            SourceId::Coinbase => Self::parse_coinbase(text, now),
            SourceId::Okx => Self::parse_okx(text, now),
            SourceId::Bybit => Self::parse_bybit(text, now),
            SourceId::Kraken => Self::parse_kraken(text, now),
        }
    }

    // --- Symbol mappings ---

    const fn binance_symbols() -> &'static [&'static str] {
        // FDUSD pair used as USD proxy for USDT price on Binance
        &["btcusdt", "ethusdt", "solusdt", "usdcusdt", "fdusdusdt", "daiusdt"]
    }

    const fn coinbase_products() -> &'static [&'static str] {
        &["BTC-USD", "ETH-USD", "SOL-USD", "USDC-USD", "USDT-USD", "DAI-USD"]
    }

    const fn okx_inst_ids() -> &'static [&'static str] {
        &["BTC-USDT", "ETH-USDT", "SOL-USDT", "USDC-USDT", "USDT-USDC", "DAI-USDT"]
    }

    const fn bybit_symbols() -> &'static [&'static str] {
        &["BTCUSDT", "ETHUSDT", "SOLUSDT", "USDCUSDT", "USDTDAI", "DAIUSDT"]
    }

    const fn kraken_pairs() -> &'static [&'static str] {
        &["BTC/USD", "ETH/USD", "SOL/USD", "USDC/USD", "USDT/USD", "DAI/USD"]
    }

    // --- Message parsers ---

    fn parse_binance(text: &str, now: u64) -> Option<Vec<PriceUpdate>> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;
        let symbol = v.get("s")?.as_str()?;
        let price_str = v.get("p")?.as_str()?;
        let asset = Self::map_binance_symbol(symbol)?;
        let price = parse_price_str(price_str)?;
        Some(vec![PriceUpdate { source: SourceId::Binance, asset, price, timestamp: now }])
    }

    fn parse_coinbase(text: &str, now: u64) -> Option<Vec<PriceUpdate>> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;
        if v.get("type")?.as_str()? != "ticker" {
            return None;
        }
        let product_id = v.get("product_id")?.as_str()?;
        let price_str = v.get("price")?.as_str()?;
        let asset = Self::map_coinbase_product(product_id)?;
        let price = parse_price_str(price_str)?;
        Some(vec![PriceUpdate { source: SourceId::Coinbase, asset, price, timestamp: now }])
    }

    fn parse_okx(text: &str, now: u64) -> Option<Vec<PriceUpdate>> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;
        let data = v.get("data")?.as_array()?;
        let mut updates = Vec::new();
        for item in data {
            let inst_id = item.get("instId")?.as_str()?;
            let price_str = item.get("last")?.as_str()?;
            if let Some(asset) = Self::map_okx_inst(inst_id) &&
                let Some(price) = parse_price_str(price_str)
            {
                updates.push(PriceUpdate { source: SourceId::Okx, asset, price, timestamp: now });
            }
        }
        if updates.is_empty() {
            None
        } else {
            Some(updates)
        }
    }

    fn parse_bybit(text: &str, now: u64) -> Option<Vec<PriceUpdate>> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;
        let data = v.get("data")?;
        let symbol = data.get("symbol")?.as_str()?;
        let price_str = data.get("lastPrice")?.as_str()?;
        let asset = Self::map_bybit_symbol(symbol)?;
        let price = parse_price_str(price_str)?;
        Some(vec![PriceUpdate { source: SourceId::Bybit, asset, price, timestamp: now }])
    }

    fn parse_kraken(text: &str, now: u64) -> Option<Vec<PriceUpdate>> {
        let v: serde_json::Value = serde_json::from_str(text).ok()?;
        let channel = v.get("channel")?.as_str()?;
        if channel != "ticker" {
            return None;
        }
        let data = v.get("data")?.as_array()?;
        let mut updates = Vec::new();
        for item in data {
            let symbol = item.get("symbol")?.as_str()?;
            let price_str = item.get("last")?.as_number()?.to_string();
            if let Some(asset) = Self::map_kraken_pair(symbol) &&
                let Some(price) = parse_price_str(&price_str)
            {
                updates.push(PriceUpdate {
                    source: SourceId::Kraken,
                    asset,
                    price,
                    timestamp: now,
                });
            }
        }
        if updates.is_empty() {
            None
        } else {
            Some(updates)
        }
    }

    // --- Symbol → AssetId mappers ---

    fn map_binance_symbol(s: &str) -> Option<AssetId> {
        match s {
            "BTCUSDT" => Some(AssetId::BTC),
            "ETHUSDT" => Some(AssetId::ETH),
            "SOLUSDT" => Some(AssetId::SOL),
            "USDCUSDT" => Some(AssetId::USDC),
            "FDUSDUSDT" => Some(AssetId::USDT),
            "DAIUSDT" => Some(AssetId::DAI),
            _ => None,
        }
    }

    fn map_coinbase_product(s: &str) -> Option<AssetId> {
        match s {
            "BTC-USD" => Some(AssetId::BTC),
            "ETH-USD" => Some(AssetId::ETH),
            "SOL-USD" => Some(AssetId::SOL),
            "USDC-USD" => Some(AssetId::USDC),
            "USDT-USD" => Some(AssetId::USDT),
            "DAI-USD" => Some(AssetId::DAI),
            _ => None,
        }
    }

    fn map_okx_inst(s: &str) -> Option<AssetId> {
        match s {
            "BTC-USDT" => Some(AssetId::BTC),
            "ETH-USDT" => Some(AssetId::ETH),
            "SOL-USDT" => Some(AssetId::SOL),
            "USDC-USDT" => Some(AssetId::USDC),
            "USDT-USDC" => Some(AssetId::USDT),
            "DAI-USDT" => Some(AssetId::DAI),
            _ => None,
        }
    }

    fn map_bybit_symbol(s: &str) -> Option<AssetId> {
        match s {
            "BTCUSDT" => Some(AssetId::BTC),
            "ETHUSDT" => Some(AssetId::ETH),
            "SOLUSDT" => Some(AssetId::SOL),
            "USDCUSDT" => Some(AssetId::USDC),
            "USDTDAI" => Some(AssetId::USDT),
            "DAIUSDT" => Some(AssetId::DAI),
            _ => None,
        }
    }

    fn map_kraken_pair(s: &str) -> Option<AssetId> {
        match s {
            "BTC/USD" => Some(AssetId::BTC),
            "ETH/USD" => Some(AssetId::ETH),
            "SOL/USD" => Some(AssetId::SOL),
            "USDC/USD" => Some(AssetId::USDC),
            "USDT/USD" => Some(AssetId::USDT),
            "DAI/USD" => Some(AssetId::DAI),
            _ => None,
        }
    }
}

/// Parses a decimal price string into a U256 with 8 decimal places.
///
/// Examples: "50000.12345678" → 5000012345678, "0.99" → 99000000
fn parse_price_str(s: &str) -> Option<U256> {
    let parts: Vec<&str> = s.split('.').collect();
    let integer_part: u128 = parts.first()?.parse().ok()?;
    let decimal_str = parts.get(1).copied().unwrap_or("0");

    // Pad or truncate to 8 decimal places
    let decimal_str = if decimal_str.len() >= 8 {
        &decimal_str[..8]
    } else {
        // Need to pad with zeros — use a temporary buffer
        return Some(U256::from(
            integer_part * 100_000_000 +
                decimal_str.parse::<u128>().ok()? * 10u128.pow(8 - decimal_str.len() as u32),
        ));
    };
    let decimal_part: u128 = decimal_str.parse().ok()?;
    Some(U256::from(integer_part * 100_000_000 + decimal_part))
}

#[cfg(test)]
mod source_tests {
    use super::*;

    #[test]
    fn parse_price_str_basic() {
        // "50000.12345678" → 5000012345678
        assert_eq!(parse_price_str("50000.12345678"), Some(U256::from(5_000_012_345_678u64)));
        // "0.99" → 99000000
        assert_eq!(parse_price_str("0.99"), Some(U256::from(99_000_000u64)));
        // "1.0" → 100000000
        assert_eq!(parse_price_str("1.0"), Some(U256::from(100_000_000u64)));
        // "100" → 10000000000
        assert_eq!(parse_price_str("100"), Some(U256::from(10_000_000_000u64)));
    }

    #[test]
    fn parse_binance_trade() {
        let msg = r#"{"e":"trade","E":1234567890,"s":"BTCUSDT","t":1,"p":"50000.12","q":"0.001","T":1234567890,"m":true}"#;
        let updates = SourceConnector::parse_binance(msg, 1234567890);
        assert!(updates.is_some());
        let updates = updates.unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].asset, AssetId::BTC);
        assert_eq!(updates[0].price, U256::from(5_000_012_000_000u64));
    }

    #[test]
    fn parse_coinbase_ticker() {
        let msg = r#"{"type":"ticker","product_id":"ETH-USD","price":"3000.50","time":"2024-01-01T00:00:00Z"}"#;
        let updates = SourceConnector::parse_coinbase(msg, 1700000000);
        assert!(updates.is_some());
        let updates = updates.unwrap();
        assert_eq!(updates[0].asset, AssetId::ETH);
        assert_eq!(updates[0].price, U256::from(300_050_000_000u64));
    }

    #[test]
    fn parse_okx_ticker() {
        let msg = r#"{"arg":{"channel":"tickers"},"data":[{"instId":"SOL-USDT","last":"100.25"}]}"#;
        let updates = SourceConnector::parse_okx(msg, 1700000000);
        assert!(updates.is_some());
        let updates = updates.unwrap();
        assert_eq!(updates[0].asset, AssetId::SOL);
        assert_eq!(updates[0].price, U256::from(10_025_000_000u64));
    }

    #[test]
    fn parse_bybit_ticker() {
        let msg =
            r#"{"topic":"tickers.BTCUSDT","data":{"symbol":"BTCUSDT","lastPrice":"50000.00"}}"#;
        let updates = SourceConnector::parse_bybit(msg, 1700000000);
        assert!(updates.is_some());
        let updates = updates.unwrap();
        assert_eq!(updates[0].asset, AssetId::BTC);
        assert_eq!(updates[0].price, U256::from(5_000_000_000_000u64));
    }

    #[test]
    fn parse_kraken_ticker() {
        let msg = r#"{"channel":"ticker","data":[{"symbol":"ETH/USD","last":3000.50}]}"#;
        let updates = SourceConnector::parse_kraken(msg, 1700000000);
        assert!(updates.is_some());
        let updates = updates.unwrap();
        assert_eq!(updates[0].asset, AssetId::ETH);
        assert_eq!(updates[0].price, U256::from(300_050_000_000u64));
    }

    #[test]
    fn parse_kraken_non_ticker_ignored() {
        let msg = r#"{"channel":"heartbeat"}"#;
        assert!(SourceConnector::parse_kraken(msg, 1700000000).is_none());
    }

    #[test]
    fn binance_usdt_via_fdusd() {
        let msg = r#"{"e":"trade","s":"FDUSDUSDT","p":"1.0001","q":"100"}"#;
        let updates = SourceConnector::parse_binance(msg, 1700000000);
        assert!(updates.is_some());
        assert_eq!(updates.unwrap()[0].asset, AssetId::USDT);
    }

    #[test]
    fn coinbase_usdc_mapping() {
        let msg = r#"{"type":"ticker","product_id":"USDC-USD","price":"0.9999"}"#;
        let updates = SourceConnector::parse_coinbase(msg, 1700000000);
        assert!(updates.is_some());
        assert_eq!(updates.unwrap()[0].asset, AssetId::USDC);
    }

    #[test]
    fn okx_usdt_via_usdc_pair() {
        let msg = r#"{"data":[{"instId":"USDT-USDC","last":"1.0000"}]}"#;
        let updates = SourceConnector::parse_okx(msg, 1700000000);
        assert!(updates.is_some());
        assert_eq!(updates.unwrap()[0].asset, AssetId::USDT);
    }

    #[test]
    fn parse_malformed_json_returns_none() {
        assert!(SourceConnector::parse_binance("not json", 100).is_none());
        assert!(SourceConnector::parse_coinbase("{}", 100).is_none());
        assert!(SourceConnector::parse_okx("{}", 100).is_none());
        assert!(SourceConnector::parse_bybit("{}", 100).is_none());
        assert!(SourceConnector::parse_kraken("{}", 100).is_none());
    }
}
