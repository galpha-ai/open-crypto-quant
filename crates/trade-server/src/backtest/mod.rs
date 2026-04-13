//! Backtest infrastructure for market making strategies.
//!
//! This module provides tools for backtesting trading strategies using
//! historical orderbook and trade data stored in Parquet files.
//!
//! # Components
//!
//! - [`loader::ParquetLoader`]: Loads orderbook snapshots and trade events from Parquet files
//! - [`timeline::BacktestTimeline`]: Merges events chronologically into ticks
//! - [`coordinator::BacktestEventCoordinator`]: Delivers events in correct priority order
//! - [`config::BacktestConfig`]: Configuration for a backtest run
//! - [`runner::BacktestRunner`]: Main entry point for executing backtests
//! - [`types::BacktestTick`]: A single tick grouping a snapshot with associated trades
//! - [`error::BacktestError`]: Error types for backtest operations
//!
//! # Event Capture
//!
//! Event capture is now handled by the unified `CapturingEventCoordinator` decorator
//! and `InMemoryEventCollector` from the `event_coordinator` module. The backtest
//! runner automatically wraps the coordinator with the capturing decorator.

mod completeness;
mod config;
mod coordinator;
#[cfg(test)]
mod e2e_test;
mod error;
#[cfg(test)]
mod integration_test;
mod loader;
#[cfg(test)]
mod loader_integration_test;
mod processor;
mod runner;
mod synthetic_snapshots;
mod timeline;
mod types;

pub use completeness::{
    CompletenessConfig, CompletenessFailure, CompletenessReport, DataCompletenessChecker,
    IncompleteDataBehavior, MarketCompleteness, OutcomeCompleteness,
};
pub use config::{
    BacktestConfig, CaptureConfig, ConfigValidationError, ExitStrategyMode, PositionConfig,
};
pub use coordinator::BacktestEventCoordinator;
pub use error::BacktestError;
pub use loader::ParquetLoader;
pub use runner::{BacktestMetrics, BacktestResult, BacktestRunner};
pub use synthetic_snapshots::{
    SyntheticBboSnapshotWriteOptions, SyntheticSnapshotWriteStats,
    write_synthetic_bbo_snapshots_parquet, write_synthetic_bbo_snapshots_parquet_with_options,
};
pub use timeline::BacktestTimeline;
pub use types::BacktestTick;
