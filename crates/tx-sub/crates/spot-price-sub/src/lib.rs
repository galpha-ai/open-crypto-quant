//! Spot price subscriber library
//!
//! This crate subscribes to real-time cryptocurrency spot prices from multiple sources
//! (Polymarket RTDS API, Binance WebSocket) and publishes them to Redis.

pub mod binance_ws_client;
pub mod config;
pub mod metrics;
pub mod parser;
pub mod ws_client;

// Re-export main types for external use
pub use binance_ws_client::BinanceWebSocketClient;
pub use config::Config;
pub use metrics::Metrics;
pub use parser::SpotPriceParser;
pub use ws_client::RtdsWebSocketClient;
