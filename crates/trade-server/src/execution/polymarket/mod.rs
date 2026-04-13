//! Polymarket order executor for CLOB trading.
//!
//! This module provides a `PolymarketOrderExecutor` implementation of the `OrderExecutor` trait
//! for trading on Polymarket's Central Limit Order Book (CLOB).
//!
//! # Architecture
//!
//! The executor uses [polyfill-rs](https://github.com/floor-licker/polyfill-rs) for high-performance
//! orderbook operations with fixed-point arithmetic.
//!
//! ## Key Components
//!
//! - `PolymarketOrderExecutor` - Main executor implementing `OrderExecutor` trait
//! - `PolymarketOrderExecutorBuilder` - Builder pattern for configuration
//! - `PolymarketConfig` - Configuration for the executor
//! - `PendingOrder` - Tracks pending limit orders awaiting fill
//! - `OrderStatusPoller` - REST API polling for fill detection
//!
//! See `docs/specs/005-polymarket-executor/design.md` for full design details.

mod builder;
mod config;
mod executor;
pub mod poller;
mod types;

#[cfg(test)]
mod executor_test;

pub use builder::PolymarketOrderExecutorBuilder;
pub use config::PolymarketConfig;
pub use executor::PolymarketOrderExecutor;
pub use poller::{MonitoredOrder, OrderStatusPoller, PollerConfig, PollerMetrics};
pub use types::{DataApiPosition, PendingOrder};
