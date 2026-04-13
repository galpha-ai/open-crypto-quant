//! Data completeness validation for backtests.
//!
//! This module provides tools for validating that backtest data is complete enough
//! for meaningful strategy evaluation. It checks for:
//!
//! - Sufficient snapshot coverage until market expiry
//! - Minimum snapshot count per outcome
//! - Minimum trade count per outcome
//! - Both outcomes present for binary markets
//!
//! # Example
//!
//! ```ignore
//! let checker = DataCompletenessChecker::new(CompletenessConfig::default());
//! let report = checker.check(&snapshots, &trades);
//!
//! match config.completeness.on_incomplete {
//!     IncompleteDataBehavior::Filter => {
//!         let (filtered_snapshots, filtered_trades) = checker.filter_complete(&snapshots, &trades, &report);
//!     }
//!     IncompleteDataBehavior::Fail => {
//!         if report.incomplete_count > 0 {
//!             return Err(BacktestError::DataCompletenessError(...));
//!         }
//!     }
//!     IncompleteDataBehavior::Warn => {
//!         // Just log, continue with all data
//!     }
//! }
//! ```

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use popeyes_trading_types::{OrderbookSnapshotEvent, PolymarketTradeEvent};

/// Configuration for data completeness checking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletenessConfig {
    /// Whether completeness checking is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Minimum number of snapshots required per outcome
    #[serde(default = "default_min_snapshot_count")]
    pub min_snapshot_count: u64,

    /// Maximum gap in seconds between last snapshot and market expiry
    #[serde(default = "default_max_snapshot_gap_secs")]
    pub max_snapshot_gap_secs: i64,

    /// Minimum number of trades required per outcome
    #[serde(default = "default_min_trade_count")]
    pub min_trade_count: u64,

    /// Behavior when incomplete data is detected
    #[serde(default)]
    pub on_incomplete: IncompleteDataBehavior,
}

fn default_enabled() -> bool {
    true
}

fn default_min_snapshot_count() -> u64 {
    100
}

fn default_max_snapshot_gap_secs() -> i64 {
    120
}

fn default_min_trade_count() -> u64 {
    10
}

impl Default for CompletenessConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_snapshot_count: 100,
            max_snapshot_gap_secs: 120,
            min_trade_count: 10,
            on_incomplete: IncompleteDataBehavior::default(),
        }
    }
}

/// Behavior when incomplete data is detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IncompleteDataBehavior {
    /// Log warning, continue with all data
    Warn,

    /// Remove incomplete markets silently (default)
    #[default]
    Filter,

    /// Return error, abort backtest
    Fail,
}

/// Report of data completeness analysis.
#[derive(Debug, Clone)]
pub struct CompletenessReport {
    /// Per-market completeness status
    pub markets: Vec<MarketCompleteness>,

    /// Number of complete markets
    pub complete_count: usize,

    /// Number of incomplete markets
    pub incomplete_count: usize,
}

impl CompletenessReport {
    /// Get the set of complete tickers.
    pub fn complete_tickers(&self) -> HashSet<&str> {
        self.markets
            .iter()
            .filter(|m| m.is_complete)
            .map(|m| m.ticker.as_str())
            .collect()
    }

    /// Get the set of incomplete tickers.
    pub fn incomplete_tickers(&self) -> HashSet<&str> {
        self.markets
            .iter()
            .filter(|m| !m.is_complete)
            .map(|m| m.ticker.as_str())
            .collect()
    }
}

/// Completeness status for a single market (ticker).
#[derive(Debug, Clone)]
pub struct MarketCompleteness {
    /// Market ticker (e.g., "btc-updown-15m-1764027000")
    pub ticker: String,

    /// Per-outcome completeness status
    pub outcomes: Vec<OutcomeCompleteness>,

    /// Whether the market as a whole is complete (all outcomes pass)
    pub is_complete: bool,

    /// Aggregated failure reasons across all outcomes
    pub failure_reasons: Vec<CompletenessFailure>,
}

/// Completeness status for a single outcome within a market.
#[derive(Debug, Clone)]
pub struct OutcomeCompleteness {
    /// Outcome name (e.g., "Up", "Down")
    pub outcome: String,

    /// Number of snapshots for this outcome
    pub snapshot_count: u64,

    /// Number of trades for this outcome
    pub trade_count: u64,

    /// Timestamp of last snapshot (milliseconds)
    pub last_snapshot_ts: Option<i64>,

    /// Market end date (parsed from metadata)
    pub end_date: Option<DateTime<Utc>>,

    /// Whether this outcome passes all completeness checks
    pub is_complete: bool,

    /// Reasons why this outcome is incomplete
    pub failure_reasons: Vec<CompletenessFailure>,
}

/// Reasons why data is considered incomplete.
#[derive(Debug, Clone, PartialEq)]
pub enum CompletenessFailure {
    /// Data ends before market expiry
    DataEndsBeforeExpiry {
        /// Gap in seconds between last snapshot and expiry
        gap_secs: i64,
    },

    /// Not enough snapshots for meaningful analysis
    InsufficientSnapshots {
        /// Actual snapshot count
        count: u64,
        /// Required minimum
        required: u64,
    },

    /// Not enough trades for realistic fill simulation
    InsufficientTrades {
        /// Actual trade count
        count: u64,
        /// Required minimum
        required: u64,
    },

    /// Missing expected outcome (e.g., binary market missing Up or Down)
    MissingOutcome {
        /// The missing outcome
        outcome: String,
    },
}

impl std::fmt::Display for CompletenessFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompletenessFailure::DataEndsBeforeExpiry { gap_secs } => {
                write!(f, "DATA_ENDS_BEFORE_EXPIRY(gap={}s)", gap_secs)
            }
            CompletenessFailure::InsufficientSnapshots { count, required } => {
                write!(f, "LOW_SNAPSHOT({}/{})", count, required)
            }
            CompletenessFailure::InsufficientTrades { count, required } => {
                write!(f, "LOW_TRADE({}/{})", count, required)
            }
            CompletenessFailure::MissingOutcome { outcome } => {
                write!(f, "MISSING_OUTCOME({})", outcome)
            }
        }
    }
}

/// Data completeness checker for backtest data.
pub struct DataCompletenessChecker {
    config: CompletenessConfig,
}

impl DataCompletenessChecker {
    /// Create a new completeness checker with the given configuration.
    pub fn new(config: CompletenessConfig) -> Self {
        Self { config }
    }

    /// Check completeness of snapshot and trade data.
    ///
    /// Returns a report with per-market and per-outcome completeness status.
    pub fn check(
        &self,
        snapshots: &[OrderbookSnapshotEvent],
        trades: &[PolymarketTradeEvent],
    ) -> CompletenessReport {
        // Group data by (ticker, outcome)
        let mut outcome_data: HashMap<(String, String), OutcomeData> = HashMap::new();

        // Process snapshots
        for snapshot in snapshots {
            if let Some(ref meta) = snapshot.market_metadata {
                let outcome = meta
                    .outcome
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string());
                let key = (meta.ticker.clone(), outcome);

                let data = outcome_data.entry(key).or_insert_with(|| OutcomeData {
                    snapshot_count: 0,
                    trade_count: 0,
                    last_snapshot_ts: None,
                    end_date: parse_end_date(&meta.end_date),
                });

                data.snapshot_count += 1;
                data.last_snapshot_ts = Some(
                    data.last_snapshot_ts
                        .map(|ts| ts.max(snapshot.timestamp))
                        .unwrap_or(snapshot.timestamp),
                );
            }
        }

        // Process trades
        for trade in trades {
            if let Some(ref meta) = trade.market_metadata {
                let outcome = meta
                    .outcome
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string());
                let key = (meta.ticker.clone(), outcome);

                let data = outcome_data.entry(key).or_insert_with(|| OutcomeData {
                    snapshot_count: 0,
                    trade_count: 0,
                    last_snapshot_ts: None,
                    end_date: parse_end_date(&meta.end_date),
                });

                data.trade_count += 1;
            }
        }

        // Group outcomes by ticker
        let mut markets_map: HashMap<String, Vec<(String, OutcomeData)>> = HashMap::new();
        for ((ticker, outcome), data) in outcome_data {
            markets_map.entry(ticker).or_default().push((outcome, data));
        }

        // Build completeness report
        let mut markets = Vec::new();
        for (ticker, outcomes_data) in markets_map {
            let market_completeness = self.check_market(&ticker, outcomes_data);
            markets.push(market_completeness);
        }

        // Sort by ticker for consistent output
        markets.sort_by(|a, b| a.ticker.cmp(&b.ticker));

        let complete_count = markets.iter().filter(|m| m.is_complete).count();
        let incomplete_count = markets.len() - complete_count;

        CompletenessReport {
            markets,
            complete_count,
            incomplete_count,
        }
    }

    /// Check completeness of a single market.
    fn check_market(
        &self,
        ticker: &str,
        outcomes_data: Vec<(String, OutcomeData)>,
    ) -> MarketCompleteness {
        let mut outcomes = Vec::new();
        let mut all_failure_reasons = Vec::new();

        // Check each outcome
        for (outcome_name, data) in &outcomes_data {
            let outcome_completeness = self.check_outcome(outcome_name, data);
            all_failure_reasons.extend(outcome_completeness.failure_reasons.clone());
            outcomes.push(outcome_completeness);
        }

        // Check for missing outcomes (binary markets should have both Up and Down)
        let outcome_names: HashSet<_> = outcomes_data.iter().map(|(o, _)| o.as_str()).collect();
        if outcome_names.contains("Up") || outcome_names.contains("Down") {
            // This looks like a binary market
            if !outcome_names.contains("Up") {
                all_failure_reasons.push(CompletenessFailure::MissingOutcome {
                    outcome: "Up".to_string(),
                });
            }
            if !outcome_names.contains("Down") {
                all_failure_reasons.push(CompletenessFailure::MissingOutcome {
                    outcome: "Down".to_string(),
                });
            }
        }

        // Sort outcomes for consistent output
        outcomes.sort_by(|a, b| a.outcome.cmp(&b.outcome));

        let is_complete = outcomes.iter().all(|o| o.is_complete) && all_failure_reasons.is_empty();

        MarketCompleteness {
            ticker: ticker.to_string(),
            outcomes,
            is_complete,
            failure_reasons: all_failure_reasons,
        }
    }

    /// Check completeness of a single outcome.
    fn check_outcome(&self, outcome_name: &str, data: &OutcomeData) -> OutcomeCompleteness {
        let mut failure_reasons = Vec::new();

        // Check snapshot count
        if data.snapshot_count < self.config.min_snapshot_count {
            failure_reasons.push(CompletenessFailure::InsufficientSnapshots {
                count: data.snapshot_count,
                required: self.config.min_snapshot_count,
            });
        }

        // Check trade count
        if data.trade_count < self.config.min_trade_count {
            failure_reasons.push(CompletenessFailure::InsufficientTrades {
                count: data.trade_count,
                required: self.config.min_trade_count,
            });
        }

        // Check data coverage until expiry
        if let (Some(last_ts), Some(end_date)) = (data.last_snapshot_ts, data.end_date) {
            let last_snapshot_dt = DateTime::from_timestamp_millis(last_ts);
            if let Some(last_dt) = last_snapshot_dt {
                let gap_secs = (end_date - last_dt).num_seconds();
                if gap_secs > self.config.max_snapshot_gap_secs {
                    failure_reasons.push(CompletenessFailure::DataEndsBeforeExpiry { gap_secs });
                }
            }
        }

        let is_complete = failure_reasons.is_empty();

        OutcomeCompleteness {
            outcome: outcome_name.to_string(),
            snapshot_count: data.snapshot_count,
            trade_count: data.trade_count,
            last_snapshot_ts: data.last_snapshot_ts,
            end_date: data.end_date,
            is_complete,
            failure_reasons,
        }
    }

    /// Filter snapshots and trades to only include complete markets.
    ///
    /// Returns (filtered_snapshots, filtered_trades).
    pub fn filter_complete(
        &self,
        snapshots: &[OrderbookSnapshotEvent],
        trades: &[PolymarketTradeEvent],
        report: &CompletenessReport,
    ) -> (Vec<OrderbookSnapshotEvent>, Vec<PolymarketTradeEvent>) {
        let complete_tickers = report.complete_tickers();

        let filtered_snapshots: Vec<_> = snapshots
            .iter()
            .filter(|s| {
                s.market_metadata
                    .as_ref()
                    .map(|m| complete_tickers.contains(m.ticker.as_str()))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        let filtered_trades: Vec<_> = trades
            .iter()
            .filter(|t| {
                t.market_metadata
                    .as_ref()
                    .map(|m| complete_tickers.contains(m.ticker.as_str()))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        (filtered_snapshots, filtered_trades)
    }

    /// Log the completeness report at INFO level.
    pub fn log_report(&self, report: &CompletenessReport) {
        info!(
            complete_markets = report.complete_count,
            incomplete_markets = report.incomplete_count,
            "Data completeness check completed"
        );

        if report.incomplete_count > 0 {
            for market in &report.markets {
                if !market.is_complete {
                    let reasons: Vec<String> = market
                        .failure_reasons
                        .iter()
                        .map(|r| r.to_string())
                        .collect();
                    warn!(
                        ticker = %market.ticker,
                        reasons = %reasons.join(", "),
                        "Incomplete market"
                    );
                }
            }
        }
    }
}

/// Internal data structure for accumulating outcome statistics.
struct OutcomeData {
    snapshot_count: u64,
    trade_count: u64,
    last_snapshot_ts: Option<i64>,
    end_date: Option<DateTime<Utc>>,
}

/// Parse end_date from ISO 8601 string.
fn parse_end_date(end_date_str: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(end_date_str)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use popeyes_trading_types::{OrderbookSource, PolymarketMarketMetadata};

    fn make_snapshot(
        ticker: &str,
        outcome: &str,
        timestamp: i64,
        end_date: &str,
    ) -> OrderbookSnapshotEvent {
        OrderbookSnapshotEvent {
            asset_id: "asset1".to_string(),
            market: "market1".to_string(),
            bids: vec![],
            asks: vec![],
            hash: "hash".to_string(),
            timestamp,
            source: OrderbookSource::Polymarket,
            market_metadata: Some(PolymarketMarketMetadata {
                event_id: "event1".to_string(),
                ticker: ticker.to_string(),
                title: "Test Market".to_string(),
                end_date: end_date.to_string(),
                outcome: Some(outcome.to_string()),
            }),
            observed_at: chrono::Utc::now(),
        }
    }

    fn make_trade(
        ticker: &str,
        outcome: &str,
        timestamp: i64,
        end_date: &str,
    ) -> PolymarketTradeEvent {
        PolymarketTradeEvent {
            asset_id: "asset1".to_string(),
            market: "market1".to_string(),
            price: 0.5,
            size: 100.0,
            side: popeyes_trading_types::TradeSide::Buy,
            timestamp,
            fee_rate_bps: 0,
            market_metadata: Some(PolymarketMarketMetadata {
                event_id: "event1".to_string(),
                ticker: ticker.to_string(),
                title: "Test Market".to_string(),
                end_date: end_date.to_string(),
                outcome: Some(outcome.to_string()),
            }),
            observed_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_default_config() {
        let config = CompletenessConfig::default();
        assert!(config.enabled);
        assert_eq!(config.min_snapshot_count, 100);
        assert_eq!(config.max_snapshot_gap_secs, 120);
        assert_eq!(config.min_trade_count, 10);
        assert_eq!(config.on_incomplete, IncompleteDataBehavior::Filter);
    }

    #[test]
    fn test_empty_data() {
        let checker = DataCompletenessChecker::new(CompletenessConfig::default());
        let report = checker.check(&[], &[]);

        assert_eq!(report.complete_count, 0);
        assert_eq!(report.incomplete_count, 0);
        assert!(report.markets.is_empty());
    }

    #[test]
    fn test_complete_market() {
        let config = CompletenessConfig {
            min_snapshot_count: 2,
            min_trade_count: 2,
            max_snapshot_gap_secs: 120,
            ..Default::default()
        };
        let checker = DataCompletenessChecker::new(config);

        // Market ends at 2025-01-01T12:00:00Z (timestamp 1735732800000)
        let end_date = "2025-01-01T12:00:00Z";
        let end_ts = 1735732800000i64;

        // Last snapshot 60 seconds before expiry (within 120s threshold)
        let last_snapshot_ts = end_ts - 60_000;

        let snapshots = vec![
            make_snapshot("ticker1", "Up", end_ts - 120_000, end_date),
            make_snapshot("ticker1", "Up", last_snapshot_ts, end_date),
            make_snapshot("ticker1", "Down", end_ts - 120_000, end_date),
            make_snapshot("ticker1", "Down", last_snapshot_ts, end_date),
        ];

        let trades = vec![
            make_trade("ticker1", "Up", end_ts - 100_000, end_date),
            make_trade("ticker1", "Up", end_ts - 90_000, end_date),
            make_trade("ticker1", "Down", end_ts - 100_000, end_date),
            make_trade("ticker1", "Down", end_ts - 90_000, end_date),
        ];

        let report = checker.check(&snapshots, &trades);

        assert_eq!(report.complete_count, 1);
        assert_eq!(report.incomplete_count, 0);
        assert!(report.markets[0].is_complete);
    }

    #[test]
    fn test_insufficient_snapshots() {
        let config = CompletenessConfig {
            min_snapshot_count: 100,
            min_trade_count: 1,
            max_snapshot_gap_secs: 120,
            ..Default::default()
        };
        let checker = DataCompletenessChecker::new(config);

        let end_date = "2025-01-01T12:00:00Z";
        let end_ts = 1735732800000i64;

        let snapshots = vec![
            make_snapshot("ticker1", "Up", end_ts - 60_000, end_date),
            make_snapshot("ticker1", "Down", end_ts - 60_000, end_date),
        ];

        let trades = vec![
            make_trade("ticker1", "Up", end_ts - 50_000, end_date),
            make_trade("ticker1", "Down", end_ts - 50_000, end_date),
        ];

        let report = checker.check(&snapshots, &trades);

        assert_eq!(report.incomplete_count, 1);
        assert!(!report.markets[0].is_complete);

        // Check failure reasons
        let up_outcome = report.markets[0]
            .outcomes
            .iter()
            .find(|o| o.outcome == "Up")
            .unwrap();
        assert!(up_outcome.failure_reasons.iter().any(|r| matches!(
            r,
            CompletenessFailure::InsufficientSnapshots {
                count: 1,
                required: 100
            }
        )));
    }

    #[test]
    fn test_data_ends_before_expiry() {
        let config = CompletenessConfig {
            min_snapshot_count: 1,
            min_trade_count: 1,
            max_snapshot_gap_secs: 120,
            ..Default::default()
        };
        let checker = DataCompletenessChecker::new(config);

        let end_date = "2025-01-01T12:00:00Z";
        let end_ts = 1735732800000i64;

        // Last snapshot 300 seconds (5 min) before expiry - exceeds 120s threshold
        let last_snapshot_ts = end_ts - 300_000;

        let snapshots = vec![
            make_snapshot("ticker1", "Up", last_snapshot_ts, end_date),
            make_snapshot("ticker1", "Down", last_snapshot_ts, end_date),
        ];

        let trades = vec![
            make_trade("ticker1", "Up", last_snapshot_ts, end_date),
            make_trade("ticker1", "Down", last_snapshot_ts, end_date),
        ];

        let report = checker.check(&snapshots, &trades);

        assert_eq!(report.incomplete_count, 1);
        assert!(!report.markets[0].is_complete);

        // Check for DATA_ENDS_BEFORE_EXPIRY failure
        let up_outcome = report.markets[0]
            .outcomes
            .iter()
            .find(|o| o.outcome == "Up")
            .unwrap();
        assert!(up_outcome.failure_reasons.iter().any(|r| matches!(
            r,
            CompletenessFailure::DataEndsBeforeExpiry { gap_secs: 300 }
        )));
    }

    #[test]
    fn test_missing_outcome() {
        let config = CompletenessConfig {
            min_snapshot_count: 1,
            min_trade_count: 1,
            max_snapshot_gap_secs: 120,
            ..Default::default()
        };
        let checker = DataCompletenessChecker::new(config);

        let end_date = "2025-01-01T12:00:00Z";
        let end_ts = 1735732800000i64;

        // Only "Up" outcome, missing "Down"
        let snapshots = vec![make_snapshot("ticker1", "Up", end_ts - 60_000, end_date)];
        let trades = vec![make_trade("ticker1", "Up", end_ts - 50_000, end_date)];

        let report = checker.check(&snapshots, &trades);

        assert_eq!(report.incomplete_count, 1);
        assert!(!report.markets[0].is_complete);

        // Check for MISSING_OUTCOME failure
        assert!(report.markets[0].failure_reasons.iter().any(|r| matches!(
            r,
            CompletenessFailure::MissingOutcome { outcome } if outcome == "Down"
        )));
    }

    #[test]
    fn test_filter_complete() {
        let config = CompletenessConfig {
            min_snapshot_count: 2,
            min_trade_count: 2,
            max_snapshot_gap_secs: 120,
            ..Default::default()
        };
        let checker = DataCompletenessChecker::new(config);

        let end_date = "2025-01-01T12:00:00Z";
        let end_ts = 1735732800000i64;

        // Complete market: ticker1
        // Incomplete market: ticker2 (only 1 snapshot each)
        let snapshots = vec![
            make_snapshot("ticker1", "Up", end_ts - 120_000, end_date),
            make_snapshot("ticker1", "Up", end_ts - 60_000, end_date),
            make_snapshot("ticker1", "Down", end_ts - 120_000, end_date),
            make_snapshot("ticker1", "Down", end_ts - 60_000, end_date),
            make_snapshot("ticker2", "Up", end_ts - 60_000, end_date),
            make_snapshot("ticker2", "Down", end_ts - 60_000, end_date),
        ];

        let trades = vec![
            make_trade("ticker1", "Up", end_ts - 100_000, end_date),
            make_trade("ticker1", "Up", end_ts - 90_000, end_date),
            make_trade("ticker1", "Down", end_ts - 100_000, end_date),
            make_trade("ticker1", "Down", end_ts - 90_000, end_date),
            make_trade("ticker2", "Up", end_ts - 50_000, end_date),
            make_trade("ticker2", "Down", end_ts - 50_000, end_date),
        ];

        let report = checker.check(&snapshots, &trades);
        let (filtered_snapshots, filtered_trades) =
            checker.filter_complete(&snapshots, &trades, &report);

        // Should only have ticker1 data
        assert_eq!(filtered_snapshots.len(), 4);
        assert_eq!(filtered_trades.len(), 4);

        // Verify all filtered data is from ticker1
        assert!(filtered_snapshots.iter().all(|s| {
            s.market_metadata
                .as_ref()
                .map(|m| m.ticker == "ticker1")
                .unwrap_or(false)
        }));
        assert!(filtered_trades.iter().all(|t| {
            t.market_metadata
                .as_ref()
                .map(|m| m.ticker == "ticker1")
                .unwrap_or(false)
        }));
    }

    #[test]
    fn test_completeness_failure_display() {
        let failure1 = CompletenessFailure::DataEndsBeforeExpiry { gap_secs: 300 };
        assert_eq!(failure1.to_string(), "DATA_ENDS_BEFORE_EXPIRY(gap=300s)");

        let failure2 = CompletenessFailure::InsufficientSnapshots {
            count: 5,
            required: 100,
        };
        assert_eq!(failure2.to_string(), "LOW_SNAPSHOT(5/100)");

        let failure3 = CompletenessFailure::InsufficientTrades {
            count: 2,
            required: 10,
        };
        assert_eq!(failure3.to_string(), "LOW_TRADE(2/10)");

        let failure4 = CompletenessFailure::MissingOutcome {
            outcome: "Down".to_string(),
        };
        assert_eq!(failure4.to_string(), "MISSING_OUTCOME(Down)");
    }

    #[test]
    fn test_parse_end_date() {
        let valid_date = parse_end_date("2025-01-01T12:00:00Z");
        assert!(valid_date.is_some());
        assert_eq!(valid_date.unwrap().timestamp_millis(), 1735732800000);

        let invalid_date = parse_end_date("not-a-date");
        assert!(invalid_date.is_none());
    }
}
