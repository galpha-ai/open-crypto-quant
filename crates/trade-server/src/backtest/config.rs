//! Configuration types for backtest execution.
//!
//! Provides strongly-typed configuration for running backtests, including
//! data paths, position management settings, and output configuration.

use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use crate::config::LatencySimulationConfig;
use crate::domain::SystemEvent;

use super::completeness::CompletenessConfig;
use popeyes_trading_types::MarketDataEvent;

/// Exit-strategy mode for backtests.
///
/// - `Configurable`: enable take-profit / stop-loss / timeout / max-sell-failures exits.
/// - `Noop`: disable automatic exits (useful for market-making strategies that manage exits explicitly).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExitStrategyMode {
    Configurable,
    Noop,
}

impl Default for ExitStrategyMode {
    fn default() -> Self {
        Self::Configurable
    }
}

/// Configuration for a backtest run.
///
/// This struct contains all settings needed to execute a backtest, including:
/// - Paths to input data files (Parquet format)
/// - Position management parameters
/// - Timer intervals and output configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfig {
    /// Path to Parquet file containing orderbook snapshots
    pub snapshot_path: PathBuf,

    /// Path to Parquet file containing orderbook updates
    pub update_path: PathBuf,

    /// Path to Parquet file containing trade events
    pub trade_path: PathBuf,

    /// Optional path to JSONL file containing spot trade events (e.g., BTC/USDC prices)
    /// If provided, spot events will be loaded and integrated into the backtest timeline
    #[serde(default)]
    pub spot_event_path: Option<PathBuf>,

    /// Filter events by outcome (e.g., "Up", "Down")
    /// None means process all outcomes
    #[serde(default)]
    pub outcome_filter: Option<String>,

    /// Filter events by ticker patterns (glob patterns, e.g., "eth-updown-15m-*")
    /// None or empty means process all tickers
    #[serde(default)]
    pub ticker_patterns: Option<Vec<String>>,

    /// Enable time-range filtering when ticker patterns are exact Polymarket tickers.
    /// When enabled, the loader derives a [start, end] window from tickers and
    /// applies it as a timestamp filter.
    #[serde(default = "default_true")]
    pub ticker_time_range_filter: bool,

    /// Buffer applied before/after the derived ticker time range.
    #[serde(with = "humantime_serde", default = "default_ticker_time_range_buffer")]
    pub ticker_time_range_buffer: Duration,

    /// Position manager configuration
    pub position: PositionConfig,

    /// Timer event interval (for checking exit conditions, etc.)
    #[serde(with = "humantime_serde")]
    pub timer_interval: Duration,

    /// Output path for JSONL event log
    pub output_path: PathBuf,

    /// Optional path for signal persistence (JSONL format)
    /// When set, all generated signals (with fair_price) will be recorded here
    #[serde(default)]
    pub signal_output_path: Option<PathBuf>,

    /// Buy slippage for market orders (e.g., 0.01 for 1%)
    #[serde(default = "default_buy_slippage")]
    pub buy_slippage: f64,

    /// Sell slippage for market orders (e.g., 0.01 for 1%)
    #[serde(default = "default_sell_slippage")]
    pub sell_slippage: f64,

    /// Latency simulation configuration (optional).
    /// When enabled, orders are not eligible for fills until after a simulated latency period.
    /// This models realistic data reception and order placement delays.
    #[serde(default)]
    pub latency: Option<LatencySimulationConfig>,

    /// Data completeness checking configuration.
    /// When enabled, validates that data is complete enough for meaningful backtest results.
    #[serde(default)]
    pub completeness: CompletenessConfig,

    /// Controls which events are persisted to the backtest JSONL output.
    ///
    /// This only affects event capture/persistence; it does not change which events are
    /// delivered to the strategy.
    #[serde(default)]
    pub capture: CaptureConfig,
}

fn default_buy_slippage() -> f64 {
    0.0
}

fn default_sell_slippage() -> f64 {
    0.0
}

fn default_true() -> bool {
    true
}

fn default_ticker_time_range_buffer() -> Duration {
    Duration::from_secs(300)
}

/// Controls which events are captured/persisted during backtests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    #[serde(default = "default_true")]
    pub token_events: bool,

    #[serde(default = "default_true")]
    pub market_data_orderbook_snapshots: bool,
    #[serde(default = "default_true")]
    pub market_data_orderbook_updates: bool,
    #[serde(default = "default_true")]
    pub market_data_polymarket_trades: bool,
    #[serde(default = "default_true")]
    pub market_data_spot_prices: bool,

    #[serde(default = "default_true")]
    pub timer_events: bool,
    #[serde(default = "default_true")]
    pub signal_events: bool,
    #[serde(default = "default_true")]
    pub position_events: bool,
    #[serde(default = "default_true")]
    pub execution_events: bool,
    #[serde(default = "default_true")]
    pub limit_order_events: bool,
    #[serde(default = "default_true")]
    pub redemption_events: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            token_events: true,
            market_data_orderbook_snapshots: true,
            market_data_orderbook_updates: true,
            market_data_polymarket_trades: true,
            market_data_spot_prices: true,
            timer_events: true,
            signal_events: true,
            position_events: true,
            execution_events: true,
            limit_order_events: true,
            redemption_events: true,
        }
    }
}

impl CaptureConfig {
    pub fn should_capture(&self, event: &SystemEvent) -> bool {
        match event {
            SystemEvent::Token(_) => self.token_events,
            SystemEvent::MarketData(market_data) => match market_data {
                MarketDataEvent::OrderbookSnapshot(_) => self.market_data_orderbook_snapshots,
                MarketDataEvent::OrderbookUpdate(_) => self.market_data_orderbook_updates,
                MarketDataEvent::PolymarketTrade(_) => self.market_data_polymarket_trades,
                MarketDataEvent::SpotPrice(_) => self.market_data_spot_prices,
            },
            SystemEvent::Timer(_) => self.timer_events,
            SystemEvent::Signal(_) => self.signal_events,
            SystemEvent::Position(_) => self.position_events,
            SystemEvent::Execution(_) => self.execution_events,
            SystemEvent::LimitOrder(_) => self.limit_order_events,
            SystemEvent::Redemption(_) => self.redemption_events,
        }
    }
}

impl BacktestConfig {
    /// Create a new BacktestConfig with the specified data paths.
    ///
    /// Uses default values for position config, timer interval, and output path.
    pub fn new(
        snapshot_path: PathBuf,
        update_path: PathBuf,
        trade_path: PathBuf,
        output_path: PathBuf,
    ) -> Self {
        Self {
            snapshot_path,
            update_path,
            trade_path,
            spot_event_path: None,
            outcome_filter: None,
            ticker_patterns: None,
            ticker_time_range_filter: true,
            ticker_time_range_buffer: default_ticker_time_range_buffer(),
            position: PositionConfig::default(),
            timer_interval: Duration::from_secs(1),
            output_path,
            signal_output_path: None,
            buy_slippage: 0.0,
            sell_slippage: 0.0,
            latency: None,
            completeness: CompletenessConfig::default(),
            capture: CaptureConfig::default(),
        }
    }

    /// Set the spot event path.
    pub fn with_spot_event_path(mut self, path: PathBuf) -> Self {
        self.spot_event_path = Some(path);
        self
    }

    /// Set the outcome filter.
    pub fn with_outcome_filter(mut self, filter: impl Into<String>) -> Self {
        self.outcome_filter = Some(filter.into());
        self
    }

    /// Set the ticker patterns filter.
    pub fn with_ticker_patterns(mut self, patterns: Vec<String>) -> Self {
        self.ticker_patterns = Some(patterns);
        self
    }

    /// Enable/disable ticker-derived time range filtering.
    pub fn with_ticker_time_range_filter(mut self, enabled: bool) -> Self {
        self.ticker_time_range_filter = enabled;
        self
    }

    /// Set the buffer applied to ticker-derived time ranges.
    pub fn with_ticker_time_range_buffer(mut self, buffer: Duration) -> Self {
        self.ticker_time_range_buffer = buffer;
        self
    }

    /// Set the position configuration.
    pub fn with_position_config(mut self, position: PositionConfig) -> Self {
        self.position = position;
        self
    }

    /// Set the timer interval.
    pub fn with_timer_interval(mut self, interval: Duration) -> Self {
        self.timer_interval = interval;
        self
    }

    /// Set the slippage values.
    pub fn with_slippage(mut self, buy_slippage: f64, sell_slippage: f64) -> Self {
        self.buy_slippage = buy_slippage;
        self.sell_slippage = sell_slippage;
        self
    }

    /// Set the latency simulation configuration.
    pub fn with_latency(mut self, latency: LatencySimulationConfig) -> Self {
        self.latency = Some(latency);
        self
    }

    /// Set the completeness checking configuration.
    pub fn with_completeness(mut self, completeness: CompletenessConfig) -> Self {
        self.completeness = completeness;
        self
    }

    /// Set the event capture configuration.
    pub fn with_capture(mut self, capture: CaptureConfig) -> Self {
        self.capture = capture;
        self
    }

    /// Validate the configuration.
    ///
    /// Returns an error if any paths don't exist or values are invalid.
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        if !self.snapshot_path.exists() {
            return Err(ConfigValidationError::FileNotFound(
                self.snapshot_path.clone(),
            ));
        }

        if !self.update_path.exists() {
            return Err(ConfigValidationError::FileNotFound(
                self.update_path.clone(),
            ));
        }

        if !self.trade_path.exists() {
            return Err(ConfigValidationError::FileNotFound(self.trade_path.clone()));
        }

        if self.position.initial_balance <= 0.0 {
            return Err(ConfigValidationError::InvalidValue(
                "initial_balance must be positive".to_string(),
            ));
        }

        if self.position.trade_amount <= 0.0 {
            return Err(ConfigValidationError::InvalidValue(
                "trade_amount must be positive".to_string(),
            ));
        }

        if self.buy_slippage < 0.0 || self.buy_slippage > 1.0 {
            return Err(ConfigValidationError::InvalidValue(
                "buy_slippage must be between 0 and 1".to_string(),
            ));
        }

        if self.sell_slippage < 0.0 || self.sell_slippage > 1.0 {
            return Err(ConfigValidationError::InvalidValue(
                "sell_slippage must be between 0 and 1".to_string(),
            ));
        }

        Ok(())
    }
}

/// Configuration for position management during backtest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionConfig {
    /// Initial balance in quote currency (e.g., USDC)
    pub initial_balance: f64,

    /// Maximum number of open positions allowed simultaneously
    pub max_open_positions: u32,

    /// Maximum holding period before forced exit
    #[serde(with = "humantime_serde")]
    pub max_holding_period: Duration,

    /// Trade amount per order (in quote currency)
    pub trade_amount: f64,

    /// Take profit threshold (as decimal, e.g., 0.1 for 10%)
    #[serde(default = "default_take_profit")]
    pub take_profit_threshold: f64,

    /// Stop loss threshold (as decimal, e.g., 0.05 for 5%)
    #[serde(default = "default_stop_loss")]
    pub stop_loss_threshold: f64,

    /// Maximum number of sell failures before giving up
    #[serde(default = "default_max_sell_failures")]
    pub max_sell_failures: u32,

    /// Which exit strategy to use during backtest.
    ///
    /// Defaults to `configurable` to preserve historical behavior.
    #[serde(default)]
    pub exit_strategy_mode: ExitStrategyMode,
}

fn default_take_profit() -> f64 {
    0.2 // 20%
}

fn default_stop_loss() -> f64 {
    0.1 // 10%
}

fn default_max_sell_failures() -> u32 {
    3
}

impl Default for PositionConfig {
    fn default() -> Self {
        Self {
            initial_balance: 10000.0,
            max_open_positions: 10,
            max_holding_period: Duration::from_secs(3600), // 1 hour
            trade_amount: 100.0,
            take_profit_threshold: 0.2,
            stop_loss_threshold: 0.1,
            max_sell_failures: 3,
            exit_strategy_mode: ExitStrategyMode::Configurable,
        }
    }
}

impl PositionConfig {
    /// Create a new PositionConfig with custom values.
    pub fn new(
        initial_balance: f64,
        max_open_positions: u32,
        max_holding_period: Duration,
        trade_amount: f64,
    ) -> Self {
        Self {
            initial_balance,
            max_open_positions,
            max_holding_period,
            trade_amount,
            ..Default::default()
        }
    }

    /// Set take profit threshold.
    pub fn with_take_profit(mut self, threshold: f64) -> Self {
        self.take_profit_threshold = threshold;
        self
    }

    /// Set stop loss threshold.
    pub fn with_stop_loss(mut self, threshold: f64) -> Self {
        self.stop_loss_threshold = threshold;
        self
    }

    /// Set the exit strategy mode.
    pub fn with_exit_strategy_mode(mut self, mode: ExitStrategyMode) -> Self {
        self.exit_strategy_mode = mode;
        self
    }

    /// Convenience for disabling automatic exits (TP/SL/timeout/etc).
    pub fn with_noop_exit_strategy(self) -> Self {
        self.with_exit_strategy_mode(ExitStrategyMode::Noop)
    }

    /// Convert to chrono Duration for use with position manager.
    pub fn max_holding_period_chrono(&self) -> chrono::Duration {
        chrono::Duration::from_std(self.max_holding_period).unwrap_or(chrono::Duration::hours(1))
    }
}

/// Errors that can occur during configuration validation.
#[derive(Debug, thiserror::Error)]
pub enum ConfigValidationError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    #[error("Invalid value: {0}")]
    InvalidValue(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_position_config() {
        let config = PositionConfig::default();
        assert_eq!(config.initial_balance, 10000.0);
        assert_eq!(config.max_open_positions, 10);
        assert_eq!(config.trade_amount, 100.0);
        assert_eq!(config.take_profit_threshold, 0.2);
        assert_eq!(config.stop_loss_threshold, 0.1);
        assert_eq!(config.exit_strategy_mode, ExitStrategyMode::Configurable);
    }

    #[test]
    fn test_position_config_builder() {
        let config = PositionConfig::new(5000.0, 5, Duration::from_secs(1800), 50.0)
            .with_take_profit(0.15)
            .with_stop_loss(0.05);

        assert_eq!(config.initial_balance, 5000.0);
        assert_eq!(config.max_open_positions, 5);
        assert_eq!(config.trade_amount, 50.0);
        assert_eq!(config.take_profit_threshold, 0.15);
        assert_eq!(config.stop_loss_threshold, 0.05);
    }

    #[test]
    fn test_backtest_config_validation_file_not_found() {
        let config = BacktestConfig::new(
            PathBuf::from("/nonexistent/snapshot.parquet"),
            PathBuf::from("/nonexistent/updates.parquet"),
            PathBuf::from("/nonexistent/trades.parquet"),
            PathBuf::from("/tmp/output.jsonl"),
        );

        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigValidationError::FileNotFound(_)
        ));
    }

    #[test]
    fn test_backtest_config_validation_invalid_balance() {
        // Create temp files for testing
        let snapshot_file = NamedTempFile::new().unwrap();
        let update_file = NamedTempFile::new().unwrap();
        let trade_file = NamedTempFile::new().unwrap();

        let config = BacktestConfig::new(
            snapshot_file.path().to_path_buf(),
            update_file.path().to_path_buf(),
            trade_file.path().to_path_buf(),
            PathBuf::from("/tmp/output.jsonl"),
        )
        .with_position_config(PositionConfig {
            initial_balance: -100.0, // Invalid
            ..Default::default()
        });

        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigValidationError::InvalidValue(_)
        ));
    }

    #[test]
    fn test_backtest_config_validation_invalid_slippage() {
        let snapshot_file = NamedTempFile::new().unwrap();
        let update_file = NamedTempFile::new().unwrap();
        let trade_file = NamedTempFile::new().unwrap();

        let config = BacktestConfig::new(
            snapshot_file.path().to_path_buf(),
            update_file.path().to_path_buf(),
            trade_file.path().to_path_buf(),
            PathBuf::from("/tmp/output.jsonl"),
        )
        .with_slippage(1.5, 0.0); // Invalid buy slippage > 1

        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_backtest_config_builder_pattern() {
        let snapshot_file = NamedTempFile::new().unwrap();
        let update_file = NamedTempFile::new().unwrap();
        let trade_file = NamedTempFile::new().unwrap();

        let config = BacktestConfig::new(
            snapshot_file.path().to_path_buf(),
            update_file.path().to_path_buf(),
            trade_file.path().to_path_buf(),
            PathBuf::from("/tmp/output.jsonl"),
        )
        .with_outcome_filter("Up")
        .with_timer_interval(Duration::from_secs(5))
        .with_slippage(0.001, 0.001);

        assert_eq!(config.outcome_filter, Some("Up".to_string()));
        assert_eq!(config.timer_interval, Duration::from_secs(5));
        assert_eq!(config.buy_slippage, 0.001);
        assert_eq!(config.sell_slippage, 0.001);
    }

    #[test]
    fn test_position_config_chrono_conversion() {
        let config = PositionConfig::new(10000.0, 10, Duration::from_secs(3600), 100.0);
        let chrono_duration = config.max_holding_period_chrono();
        assert_eq!(chrono_duration.num_seconds(), 3600);
    }
}
