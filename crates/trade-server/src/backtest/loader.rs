//! Parquet file loader for backtest data.
//!
//! Loads orderbook snapshots and trade events from Parquet files.

use std::fs::File;
use std::path::Path;
use std::time::Duration;

use arrow::array::{
    Array, BooleanBuilder, Float64Array, StringArray, TimestampMillisecondArray,
    TimestampNanosecondArray,
};
use arrow::error::ArrowError;
use chrono::{DateTime, TimeZone, Utc};
use glob_match::glob_match;
use parquet::arrow::ProjectionMask;
use parquet::arrow::arrow_reader::{
    ArrowPredicateFn, ArrowReaderOptions, ParquetRecordBatchReaderBuilder, RowFilter,
};
use popeyes_trading_types::{
    OrderSummary, OrderbookSnapshotEvent, OrderbookSource, OrderbookUpdateEvent,
    PolymarketMarketMetadata, PolymarketTradeEvent, SpotPriceUpdate, TradeSide,
};
use tracing::{debug, info, warn};

use super::error::BacktestError;

/// Large batch sizes significantly reduce per-batch overhead for Parquet scans.
/// When combined with `RowFilter` pushdown, this still keeps memory bounded
/// because only filter columns are scanned for non-matching rows.
const PARQUET_BATCH_SIZE: usize = 65_536;

#[derive(Clone)]
enum AnyStringArray {
    Utf8(StringArray),
    LargeUtf8(arrow::array::LargeStringArray),
}

impl AnyStringArray {
    fn value(&self, i: usize) -> &str {
        match self {
            AnyStringArray::Utf8(a) => a.value(i),
            AnyStringArray::LargeUtf8(a) => a.value(i),
        }
    }
}

#[derive(Debug, Clone)]
enum TickerMatcher {
    All,
    ExactOne(String),
    Exact(Vec<String>),
    Glob {
        exact: Vec<String>,
        globs: Vec<String>,
    },
}

impl TickerMatcher {
    fn new(patterns: &Option<Vec<String>>) -> Self {
        let Some(patterns) = patterns else {
            return Self::All;
        };

        let patterns: Vec<String> = patterns
            .iter()
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .map(|p| p.to_string())
            .collect();

        if patterns.is_empty() {
            return Self::All;
        }

        let mut exact = Vec::new();
        let mut globs = Vec::new();
        for p in patterns {
            if has_glob_chars(&p) {
                globs.push(p);
            } else {
                exact.push(p);
            }
        }

        match (exact.len(), globs.is_empty()) {
            (0, _) => Self::Glob {
                exact: Vec::new(),
                globs,
            },
            (1, true) => Self::ExactOne(exact.into_iter().next().unwrap()),
            (_, true) => Self::Exact(exact),
            (_, false) => Self::Glob { exact, globs },
        }
    }

    fn enabled(&self) -> bool {
        !matches!(self, Self::All)
    }

    fn matches(&self, ticker: &str) -> bool {
        match self {
            Self::All => true,
            Self::ExactOne(p) => ticker == p,
            Self::Exact(patterns) => patterns.iter().any(|p| p == ticker),
            Self::Glob { exact, globs } => {
                if exact.iter().any(|p| p == ticker) {
                    return true;
                }
                globs.iter().any(|p| glob_match(p, ticker))
            }
        }
    }
}

fn has_glob_chars(pattern: &str) -> bool {
    // Keep this aligned with the glob syntax supported by the `glob-match` crate.
    // See `glob_match_internal` implementation for details.
    pattern.contains('*')
        || pattern.contains('?')
        || pattern.contains('[')
        || pattern.contains('{')
        || pattern.contains('}')
        || pattern.contains(',')
        || pattern.contains('!')
        || pattern.contains('\\')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimeRangeMs {
    start_ms: i64,
    end_ms: i64,
}

impl TimeRangeMs {
    fn contains(&self, ts_ms: i64) -> bool {
        ts_ms >= self.start_ms && ts_ms <= self.end_ms
    }
}

const DEFAULT_TICKER_TIME_BUFFER: Duration = Duration::from_secs(300);

fn parse_duration_seconds(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    if raw.len() < 2 {
        return None;
    }
    let (value_str, unit) = raw.split_at(raw.len() - 1);
    let value: i64 = value_str.parse().ok()?;
    let multiplier = match unit.to_ascii_lowercase().as_str() {
        "s" => 1,
        "m" => 60,
        "h" => 60 * 60,
        "d" => 60 * 60 * 24,
        _ => return None,
    };
    value.checked_mul(multiplier)
}

fn parse_ticker_time_range_ms(ticker: &str) -> Option<TimeRangeMs> {
    let parts: Vec<&str> = ticker.split('-').collect();
    if parts.len() < 4 {
        return None;
    }

    let start_str = *parts.last()?;
    let duration_str = *parts.get(parts.len().saturating_sub(2))?;

    let start_seconds: i64 = start_str.parse().ok()?;
    let duration_seconds = parse_duration_seconds(duration_str)?;
    let end_seconds = start_seconds.checked_add(duration_seconds)?;

    let start_ms = start_seconds.checked_mul(1000)?;
    let end_ms = end_seconds.checked_mul(1000)?;

    Some(TimeRangeMs { start_ms, end_ms })
}

fn duration_to_ms_i64(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn derive_time_range_from_patterns(
    patterns: &Option<Vec<String>>,
    buffer: Duration,
) -> Option<TimeRangeMs> {
    let Some(patterns) = patterns else {
        return None;
    };

    let mut ranges = Vec::new();
    let mut saw_pattern = false;

    for pattern in patterns.iter() {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            continue;
        }
        saw_pattern = true;
        if has_glob_chars(trimmed) {
            debug!(
                pattern = trimmed,
                "Ticker pattern uses glob syntax; skipping time-range derivation"
            );
            return None;
        }

        match parse_ticker_time_range_ms(trimmed) {
            Some(range) => ranges.push(range),
            None => {
                debug!(
                    pattern = trimmed,
                    "Failed to parse ticker time range; skipping time-range derivation"
                );
                return None;
            }
        }
    }

    if !saw_pattern || ranges.is_empty() {
        return None;
    }

    let min_start = ranges.iter().map(|r| r.start_ms).min().unwrap_or(0);
    let max_end = ranges.iter().map(|r| r.end_ms).max().unwrap_or(0);
    let buffer_ms = duration_to_ms_i64(buffer);

    Some(TimeRangeMs {
        start_ms: min_start.saturating_sub(buffer_ms),
        end_ms: max_end.saturating_add(buffer_ms),
    })
}

#[derive(Clone, Copy)]
enum FilterColumn {
    Ticker,
    Outcome,
    Timestamp,
}

impl FilterColumn {
    fn name(self) -> &'static str {
        match self {
            Self::Ticker => "ticker",
            Self::Outcome => "outcome",
            Self::Timestamp => "ts",
        }
    }
}

/// Build a Parquet `RowFilter` for ticker/outcome filters.
///
/// This pushes down filtering into the Parquet scan so we don't decode non-matching rows
/// for all other columns (critical for large updates datasets).
fn build_row_filter(
    parquet_schema: &parquet::schema::types::SchemaDescriptor,
    arrow_schema: &arrow::datatypes::SchemaRef,
    ticker_matcher: TickerMatcher,
    outcome_filter: Option<String>,
    time_range_ms: Option<TimeRangeMs>,
) -> Option<RowFilter> {
    let has_ticker_col =
        ticker_matcher.enabled() && arrow_schema.index_of(FilterColumn::Ticker.name()).is_ok();
    let has_outcome_col =
        outcome_filter.is_some() && arrow_schema.index_of(FilterColumn::Outcome.name()).is_ok();
    let has_ts_col = time_range_ms.is_some()
        && arrow_schema
            .index_of(FilterColumn::Timestamp.name())
            .is_ok();

    if !(has_ticker_col || has_outcome_col || has_ts_col) {
        return None;
    }

    let mut indices = Vec::new();
    if has_ticker_col {
        if let Ok(idx) = arrow_schema.index_of(FilterColumn::Ticker.name()) {
            indices.push(idx);
        }
    }
    if has_outcome_col {
        if let Ok(idx) = arrow_schema.index_of(FilterColumn::Outcome.name()) {
            indices.push(idx);
        }
    }
    if has_ts_col {
        if let Ok(idx) = arrow_schema.index_of(FilterColumn::Timestamp.name()) {
            indices.push(idx);
        }
    }

    if indices.is_empty() {
        return None;
    }

    let projection = ProjectionMask::roots(parquet_schema, indices);

    let predicate = ArrowPredicateFn::new(projection, move |batch| {
        let ticker_col = if has_ticker_col {
            Some(get_string_column_from_record_batch(
                &batch,
                FilterColumn::Ticker.name(),
            )?)
        } else {
            None
        };
        let outcome_col = if has_outcome_col {
            Some(get_string_column_from_record_batch(
                &batch,
                FilterColumn::Outcome.name(),
            )?)
        } else {
            None
        };
        let ts_col = if has_ts_col {
            Some(get_timestamp_column_from_record_batch(
                &batch,
                FilterColumn::Timestamp.name(),
            )?)
        } else {
            None
        };

        let mut builder = BooleanBuilder::with_capacity(batch.num_rows());
        for i in 0..batch.num_rows() {
            let mut keep = true;

            if let (Some(ts_col), Some(range)) = (ts_col.as_ref(), time_range_ms.as_ref()) {
                if ts_col.is_null(i) || !range.contains(ts_col.value(i)) {
                    keep = false;
                }
            }

            if let Some(ticker_col) = ticker_col.as_ref() {
                if !ticker_matcher.matches(ticker_col.value(i)) {
                    keep = false;
                }
            }

            if keep {
                if let (Some(outcome_col), Some(outcome_filter)) =
                    (outcome_col.as_ref(), outcome_filter.as_ref())
                {
                    if outcome_col.value(i) != outcome_filter {
                        keep = false;
                    }
                }
            }

            builder.append_value(keep);
        }

        Ok(builder.finish())
    });

    Some(RowFilter::new(vec![Box::new(predicate)]))
}

#[derive(Clone, Copy)]
enum AnyStringArrayRef<'a> {
    Utf8(&'a StringArray),
    LargeUtf8(&'a arrow::array::LargeStringArray),
}

impl<'a> AnyStringArrayRef<'a> {
    fn value(&self, i: usize) -> &str {
        match self {
            AnyStringArrayRef::Utf8(a) => a.value(i),
            AnyStringArrayRef::LargeUtf8(a) => a.value(i),
        }
    }
}

fn get_string_column_from_record_batch<'a>(
    batch: &'a arrow::record_batch::RecordBatch,
    name: &str,
) -> Result<AnyStringArrayRef<'a>, ArrowError> {
    let col_idx = batch
        .schema()
        .index_of(name)
        .map_err(|_| ArrowError::SchemaError(format!("Column '{name}' not found")))?;

    let col = batch.column(col_idx);
    if let Some(arr) = col.as_any().downcast_ref::<StringArray>() {
        return Ok(AnyStringArrayRef::Utf8(arr));
    }
    if let Some(arr) = col
        .as_any()
        .downcast_ref::<arrow::array::LargeStringArray>()
    {
        return Ok(AnyStringArrayRef::LargeUtf8(arr));
    }

    Err(ArrowError::SchemaError(format!(
        "Column '{name}' is not a string type"
    )))
}

fn get_timestamp_column_from_record_batch<'a>(
    batch: &'a arrow::record_batch::RecordBatch,
    name: &str,
) -> Result<&'a TimestampMillisecondArray, ArrowError> {
    let col_idx = batch
        .schema()
        .index_of(name)
        .map_err(|_| ArrowError::SchemaError(format!("Column '{name}' not found")))?;

    let col = batch.column(col_idx);
    col.as_any()
        .downcast_ref::<TimestampMillisecondArray>()
        .ok_or_else(|| ArrowError::SchemaError(format!("Column '{name}' is not a timestamp type")))
}

/// Loader for Parquet files containing orderbook snapshots and trade events.
///
/// Supports optional outcome/ticker filtering and optional ticker-derived time-range filtering.
pub struct ParquetLoader {
    /// Optional filter to only include events for a specific outcome
    outcome_filter: Option<String>,
    /// Optional glob patterns to filter by ticker
    ticker_patterns: Option<Vec<String>>,
    /// Optional time range filter derived from exact ticker patterns
    time_range_ms: Option<TimeRangeMs>,
}

impl ParquetLoader {
    /// Create a new ParquetLoader with optional outcome filtering.
    pub fn new(outcome_filter: Option<String>) -> Self {
        Self {
            outcome_filter,
            ticker_patterns: None,
            time_range_ms: None,
        }
    }

    /// Create a new ParquetLoader with both outcome and ticker filtering.
    pub fn with_filters(
        outcome_filter: Option<String>,
        ticker_patterns: Option<Vec<String>>,
    ) -> Self {
        Self::with_filters_and_time_range(
            outcome_filter,
            ticker_patterns,
            true,
            DEFAULT_TICKER_TIME_BUFFER,
        )
    }

    /// Create a new ParquetLoader with explicit time-range filter settings.
    pub fn with_filters_and_time_range(
        outcome_filter: Option<String>,
        ticker_patterns: Option<Vec<String>>,
        enable_time_range_filter: bool,
        time_range_buffer: Duration,
    ) -> Self {
        let time_range_ms = if enable_time_range_filter {
            derive_time_range_from_patterns(&ticker_patterns, time_range_buffer)
        } else {
            None
        };
        if let Some(range) = time_range_ms {
            info!(
                start_ms = range.start_ms,
                end_ms = range.end_ms,
                buffer_ms = duration_to_ms_i64(time_range_buffer),
                "Derived Parquet time-range filter from ticker patterns"
            );
        } else if enable_time_range_filter {
            debug!("No ticker-derived time range filter applied");
        }
        Self {
            outcome_filter,
            ticker_patterns,
            time_range_ms,
        }
    }

    /// Load orderbook snapshots from a Parquet file.
    ///
    /// Expected schema:
    /// - `ts` (TIMESTAMP_MS): Snapshot timestamp in milliseconds
    /// - `ticker` (STRING): Market ticker identifier
    /// - `outcome` (STRING): Outcome name (e.g., "Up", "Down")
    /// - `bids` (STRING/JSON): Array of {price, size} objects
    /// - `asks` (STRING/JSON): Array of {price, size} objects
    /// - `end_date` (TIMESTAMP_NS): Market end/maturity date
    ///
    /// # Arguments
    /// * `path` - Path to the Parquet file
    ///
    /// # Returns
    /// Vector of `OrderbookSnapshotEvent` sorted by timestamp
    pub fn load_snapshots(
        &self,
        path: &Path,
    ) -> Result<Vec<OrderbookSnapshotEvent>, BacktestError> {
        info!("Loading orderbook snapshots from: {}", path.display());

        let file = File::open(path)?;
        let mut builder = ParquetRecordBatchReaderBuilder::try_new_with_options(
            file,
            ArrowReaderOptions::new().with_page_index(true),
        )?
        .with_batch_size(PARQUET_BATCH_SIZE);

        // Use column projection to only read the columns we need
        // Required columns: ts, asset_id, ticker, outcome, bids, asks, end_date
        // Optional columns: observed_at (falls back to ts if not present)
        let needed_columns = [
            "ts",
            "asset_id",
            "ticker",
            "outcome",
            "bids",
            "asks",
            "end_date",
            "observed_at",
        ];
        let arrow_schema = builder.schema();
        let indices: Vec<usize> = needed_columns
            .iter()
            .filter_map(|name| arrow_schema.index_of(name).ok())
            .collect();

        let projection = ProjectionMask::roots(builder.parquet_schema(), indices);
        let ticker_matcher = TickerMatcher::new(&self.ticker_patterns);
        if let Some(filter) = build_row_filter(
            builder.parquet_schema(),
            arrow_schema,
            ticker_matcher.clone(),
            self.outcome_filter.clone(),
            self.time_range_ms,
        ) {
            builder = builder.with_row_filter(filter);
        }

        let reader = builder.with_projection(projection).build()?;

        let mut snapshots = Vec::new();

        for batch_result in reader {
            let batch = batch_result?;

            // Get columns
            let ts_col = get_timestamp_column_by_name(&batch, "ts")?;
            let asset_id_col = get_string_column(&batch, "asset_id")?;
            let ticker_col = get_string_column(&batch, "ticker")?;
            let outcome_col = get_string_column(&batch, "outcome")?;
            let bids_col = get_string_column(&batch, "bids")?;
            let asks_col = get_string_column(&batch, "asks")?;

            let end_date_col = get_end_date_column(&batch)?;
            // observed_at is optional - falls back to ts if not present
            let observed_at_col = get_timestamp_column_by_name(&batch, "observed_at").ok();

            for i in 0..batch.num_rows() {
                let ticker = ticker_col.value(i);

                // Apply ticker filter first (before expensive JSON parsing)
                if !ticker_matcher.matches(ticker) {
                    continue;
                }

                let outcome = outcome_col.value(i);

                // Apply outcome filter if specified
                if let Some(ref filter) = self.outcome_filter {
                    if outcome != filter {
                        continue;
                    }
                }

                let timestamp = ts_col.value(i);
                if let Some(range) = self.time_range_ms {
                    if !range.contains(timestamp) {
                        continue;
                    }
                }
                let observed_at = observed_at_col
                    .as_ref()
                    .map(|col| col.value(i))
                    .unwrap_or(timestamp);
                let bids_json = bids_col.value(i);
                let asks_json = asks_col.value(i);

                let bids = parse_bid_levels(bids_json)?;
                let asks = parse_ask_levels(asks_json)?;

                let end_date = timestamp_ns_to_rfc3339(end_date_col.value(i));

                // Create market metadata
                let market_metadata = Some(PolymarketMarketMetadata {
                    event_id: String::new(),
                    ticker: ticker.to_string(),
                    title: String::new(),
                    end_date,
                    outcome: Some(outcome.to_string()),
                });

                let snapshot = OrderbookSnapshotEvent {
                    asset_id: asset_id_col.value(i).to_string(),
                    market: ticker.to_string(),
                    bids,
                    asks,
                    hash: String::new(),
                    timestamp,
                    observed_at: Utc.timestamp_millis_opt(observed_at).unwrap(),
                    source: OrderbookSource::Polymarket,
                    market_metadata,
                };

                snapshots.push(snapshot);
            }
        }

        if snapshots.is_empty() {
            return Err(BacktestError::EmptyDataError(
                "No snapshots found in file".to_string(),
            ));
        }

        // Sort by timestamp
        snapshots.sort_by_key(|s| s.timestamp);

        info!(
            "Loaded {} snapshots, date range: {} to {}",
            snapshots.len(),
            snapshots.first().map(|s| s.timestamp).unwrap_or(0),
            snapshots.last().map(|s| s.timestamp).unwrap_or(0)
        );

        Ok(snapshots)
    }

    /// Load orderbook updates from a Parquet file.
    ///
    /// Expected schema:
    /// - `ts` (TIMESTAMP_MS): Update timestamp in milliseconds
    /// - `asset_id` (STRING): Asset identifier
    /// - `market` (STRING): Market identifier
    /// - `price` (FLOAT64): Price level
    /// - `size` (FLOAT64): Size at this level
    /// - `side` (STRING): "BUY" or "SELL"
    /// - `hash` (STRING): Orderbook hash
    /// - `best_bid` (FLOAT64): Current best bid
    /// - `best_ask` (FLOAT64): Current best ask
    /// - `source` (STRING): Source identifier (e.g., "polymarket")
    /// - `event_id` (STRING): Event identifier
    /// - `ticker` (STRING): Market ticker
    /// - `title` (STRING): Market title
    /// - `end_date` (TIMESTAMP_NS): Market end/maturity date
    /// - `outcome` (STRING): Outcome name
    ///
    /// # Arguments
    /// * `path` - Path to the Parquet file
    ///
    /// # Returns
    /// Vector of `OrderbookUpdateEvent` sorted by timestamp
    pub fn load_updates(&self, path: &Path) -> Result<Vec<OrderbookUpdateEvent>, BacktestError> {
        info!("Loading orderbook updates from: {}", path.display());

        let file = File::open(path)?;
        let mut builder = ParquetRecordBatchReaderBuilder::try_new_with_options(
            file,
            ArrowReaderOptions::new().with_page_index(true),
        )?
        .with_batch_size(PARQUET_BATCH_SIZE);

        let needed_columns = [
            "ts",
            "asset_id",
            "market",
            "price",
            "size",
            "side",
            "hash",
            "best_bid",
            "best_ask",
            "source",
            "event_id",
            "ticker",
            "title",
            "end_date",
            "outcome",
            "observed_at",
        ];
        let arrow_schema = builder.schema();
        let indices: Vec<usize> = needed_columns
            .iter()
            .filter_map(|name| arrow_schema.index_of(name).ok())
            .collect();

        let projection = ProjectionMask::roots(builder.parquet_schema(), indices);
        let ticker_matcher = TickerMatcher::new(&self.ticker_patterns);
        if let Some(filter) = build_row_filter(
            builder.parquet_schema(),
            arrow_schema,
            ticker_matcher.clone(),
            self.outcome_filter.clone(),
            self.time_range_ms,
        ) {
            builder = builder.with_row_filter(filter);
        }

        let reader = builder.with_projection(projection).build()?;

        let mut updates = Vec::new();

        for batch_result in reader {
            let batch = batch_result?;

            let ts_col = get_timestamp_column_by_name(&batch, "ts")?;
            let asset_id_col = get_string_column(&batch, "asset_id")?;
            let market_col = get_string_column(&batch, "market")?;
            let price_col = get_float_column(&batch, "price")?;
            let size_col = get_float_column(&batch, "size")?;
            let side_col = get_string_column(&batch, "side")?;
            let hash_col = get_string_column(&batch, "hash")?;
            let best_bid_col = get_float_column(&batch, "best_bid")?;
            let best_ask_col = get_float_column(&batch, "best_ask")?;
            let source_col = get_string_column(&batch, "source")?;
            let event_id_col = get_string_column(&batch, "event_id")?;
            let ticker_col = get_string_column(&batch, "ticker")?;
            let title_col = get_string_column(&batch, "title")?;
            let outcome_col = get_string_column(&batch, "outcome")?;
            let end_date_col = get_end_date_column(&batch)?;
            let observed_at_col = get_timestamp_column_by_name(&batch, "observed_at").ok();

            for i in 0..batch.num_rows() {
                let ticker = ticker_col.value(i);

                if !ticker_matcher.matches(ticker) {
                    continue;
                }

                let outcome = outcome_col.value(i);

                if let Some(ref filter) = self.outcome_filter {
                    if outcome != filter {
                        continue;
                    }
                }

                let timestamp = ts_col.value(i);
                if let Some(range) = self.time_range_ms {
                    if !range.contains(timestamp) {
                        continue;
                    }
                }
                let observed_at = observed_at_col
                    .as_ref()
                    .map(|col| col.value(i))
                    .unwrap_or(timestamp);

                let side_raw = side_col.value(i);
                let side = if side_raw.eq_ignore_ascii_case("BUY") {
                    TradeSide::Buy
                } else if side_raw.eq_ignore_ascii_case("SELL") {
                    TradeSide::Sell
                } else {
                    debug!("Unknown update side: {}, skipping", side_raw);
                    continue;
                };

                let source_raw = source_col.value(i);
                let source = if source_raw.eq_ignore_ascii_case("polymarket") {
                    OrderbookSource::Polymarket
                } else {
                    OrderbookSource::Polymarket
                };

                let end_date = timestamp_ns_to_rfc3339(end_date_col.value(i));

                let market_metadata = Some(PolymarketMarketMetadata {
                    event_id: event_id_col.value(i).to_string(),
                    ticker: ticker.to_string(),
                    title: title_col.value(i).to_string(),
                    end_date,
                    outcome: Some(outcome.to_string()),
                });

                updates.push(OrderbookUpdateEvent {
                    asset_id: asset_id_col.value(i).to_string(),
                    market: market_col.value(i).to_string(),
                    price: price_col.value(i),
                    size: size_col.value(i),
                    side,
                    hash: hash_col.value(i).to_string(),
                    best_bid: best_bid_col.value(i),
                    best_ask: best_ask_col.value(i),
                    timestamp,
                    observed_at: Utc.timestamp_millis_opt(observed_at).unwrap(),
                    source,
                    market_metadata,
                });
            }
        }

        if updates.is_empty() {
            warn!("No orderbook updates found in file");
            return Ok(updates);
        }

        updates.sort_by_key(|u| u.timestamp);

        info!(
            "Loaded {} updates, date range: {} to {}",
            updates.len(),
            updates.first().map(|u| u.timestamp).unwrap_or(0),
            updates.last().map(|u| u.timestamp).unwrap_or(0)
        );

        Ok(updates)
    }

    /// Load trade events from a Parquet file.
    ///
    /// Expected schema:
    /// - `ts` (TIMESTAMP_MS): Trade timestamp in milliseconds
    /// - `ticker` (STRING): Market ticker identifier
    /// - `outcome` (STRING): Outcome name
    /// - `side` (STRING): "BUY" or "SELL" (aggressor side)
    /// - `price` (FLOAT64): Trade price
    /// - `size` (FLOAT64): Trade size
    ///
    /// # Arguments
    /// * `path` - Path to the Parquet file
    ///
    /// # Returns
    /// Vector of `PolymarketTradeEvent` sorted by timestamp
    pub fn load_trades(&self, path: &Path) -> Result<Vec<PolymarketTradeEvent>, BacktestError> {
        info!("Loading trade events from: {}", path.display());

        let file = File::open(path)?;
        let mut builder = ParquetRecordBatchReaderBuilder::try_new_with_options(
            file,
            ArrowReaderOptions::new().with_page_index(true),
        )?
        .with_batch_size(PARQUET_BATCH_SIZE);

        // Use column projection to only read the columns we need.
        //
        // Required columns: ts, ticker, outcome, side, price, size, asset_id
        // Optional columns:
        // - observed_at (falls back to ts if not present)
        let needed_columns = [
            "ts",
            "ticker",
            "outcome",
            "side",
            "price",
            "size",
            "asset_id",
            "observed_at",
        ];
        let arrow_schema = builder.schema();
        let indices: Vec<usize> = needed_columns
            .iter()
            .filter_map(|name| arrow_schema.index_of(name).ok())
            .collect();

        let projection = ProjectionMask::roots(builder.parquet_schema(), indices);
        let ticker_matcher = TickerMatcher::new(&self.ticker_patterns);
        if let Some(filter) = build_row_filter(
            builder.parquet_schema(),
            arrow_schema,
            ticker_matcher.clone(),
            self.outcome_filter.clone(),
            self.time_range_ms,
        ) {
            builder = builder.with_row_filter(filter);
        }

        let reader = builder.with_projection(projection).build()?;

        let mut trades = Vec::new();

        for batch_result in reader {
            let batch = batch_result?;

            // Get columns
            let ts_col = get_timestamp_column_by_name(&batch, "ts")?;
            let ticker_col = get_string_column(&batch, "ticker")?;
            let outcome_col = get_string_column(&batch, "outcome")?;
            let side_col = get_string_column(&batch, "side")?;
            let price_col = get_float_column(&batch, "price")?;
            let size_col = get_float_column(&batch, "size")?;
            let asset_id_col = get_string_column(&batch, "asset_id")?;
            // observed_at is optional - falls back to ts if not present
            let observed_at_col = get_timestamp_column_by_name(&batch, "observed_at").ok();

            for i in 0..batch.num_rows() {
                let ticker = ticker_col.value(i);

                // Apply ticker filter first
                if !ticker_matcher.matches(ticker) {
                    continue;
                }

                let outcome = outcome_col.value(i);

                // Apply outcome filter if specified
                if let Some(ref filter) = self.outcome_filter {
                    if outcome != filter {
                        continue;
                    }
                }

                let timestamp = ts_col.value(i);
                if let Some(range) = self.time_range_ms {
                    if !range.contains(timestamp) {
                        continue;
                    }
                }
                let observed_at = observed_at_col
                    .as_ref()
                    .map(|col| col.value(i))
                    .unwrap_or(timestamp);
                let side_str = side_col.value(i);
                let price = price_col.value(i);
                let size = size_col.value(i);

                let side = if side_str.eq_ignore_ascii_case("BUY") {
                    TradeSide::Buy
                } else if side_str.eq_ignore_ascii_case("SELL") {
                    TradeSide::Sell
                } else {
                    debug!("Unknown trade side: {}, skipping", side_str);
                    continue;
                };

                let asset_id = asset_id_col.value(i).to_string();

                let market_metadata = Some(PolymarketMarketMetadata {
                    event_id: String::new(),
                    ticker: ticker.to_string(),
                    title: String::new(),
                    end_date: String::new(),
                    outcome: Some(outcome.to_string()),
                });

                let trade = PolymarketTradeEvent {
                    asset_id,
                    market: ticker.to_string(),
                    price,
                    size,
                    side,
                    timestamp,
                    observed_at: Utc.timestamp_millis_opt(observed_at).unwrap(),
                    fee_rate_bps: 0,
                    market_metadata,
                };

                trades.push(trade);
            }
        }

        if trades.is_empty() {
            return Err(BacktestError::EmptyDataError(
                "No trades found in file".to_string(),
            ));
        }

        // Sort by timestamp
        trades.sort_by_key(|t| t.timestamp);

        info!(
            "Loaded {} trades, date range: {} to {}",
            trades.len(),
            trades.first().map(|t| t.timestamp).unwrap_or(0),
            trades.last().map(|t| t.timestamp).unwrap_or(0)
        );

        Ok(trades)
    }

    /// Load spot price events from a Parquet file.
    ///
    /// Expected schema:
    /// - `ts` (TIMESTAMP_MS): Spot price timestamp in milliseconds
    /// - `symbol` (STRING): Trading pair symbol (e.g., "BTC/USDC")
    /// - `price` (FLOAT64): Spot price
    /// - `source` (STRING, optional): Data source (e.g., "binance", "chainlink")
    ///
    /// # Arguments
    /// * `path` - Path to the Parquet file
    ///
    /// # Returns
    /// Vector of `SpotPriceUpdate` sorted by timestamp
    pub fn load_spot_events(&self, path: &Path) -> Result<Vec<SpotPriceUpdate>, BacktestError> {
        info!("Loading spot price events from: {}", path.display());

        let file = File::open(path)?;
        let mut builder = ParquetRecordBatchReaderBuilder::try_new_with_options(
            file,
            ArrowReaderOptions::new().with_page_index(true),
        )?
        .with_batch_size(PARQUET_BATCH_SIZE);

        // Use column projection to only read the columns we need
        // Required columns: ts, symbol, price
        // Optional columns: source
        let needed_columns = ["ts", "symbol", "price", "source"];
        let arrow_schema = builder.schema();
        let indices: Vec<usize> = needed_columns
            .iter()
            .filter_map(|name| arrow_schema.index_of(name).ok())
            .collect();

        let projection = ProjectionMask::roots(builder.parquet_schema(), indices);
        if let Some(filter) = build_row_filter(
            builder.parquet_schema(),
            arrow_schema,
            TickerMatcher::All,
            None,
            self.time_range_ms,
        ) {
            builder = builder.with_row_filter(filter);
        }
        let reader = builder.with_projection(projection).build()?;

        let mut events = Vec::new();

        for batch_result in reader {
            let batch = batch_result?;

            // Get columns
            let ts_col = get_timestamp_column_by_name(&batch, "ts")?;
            let symbol_col = get_string_column(&batch, "symbol")?;
            let price_col = get_float_column(&batch, "price")?;

            // Source column is optional
            let source_col = get_string_column(&batch, "source").ok();

            for i in 0..batch.num_rows() {
                let symbol = symbol_col.value(i).to_string();
                let price = price_col.value(i);
                let timestamp_millis = ts_col.value(i);
                if let Some(range) = self.time_range_ms {
                    if !range.contains(timestamp_millis) {
                        continue;
                    }
                }

                if i < 5 {
                    debug!(
                        "Spot event {}: symbol={}, price={}, timestamp_ms={}",
                        i, symbol, price, timestamp_millis
                    );
                }

                let timestamp =
                    DateTime::from_timestamp_millis(timestamp_millis).unwrap_or_else(|| {
                        warn!("Invalid timestamp {}, using epoch", timestamp_millis);
                        DateTime::from_timestamp_millis(0).unwrap()
                    });

                let source = source_col
                    .as_ref()
                    .map(|col| col.value(i).to_string())
                    .unwrap_or_else(|| "backtest".to_string());

                events.push(SpotPriceUpdate {
                    symbol,
                    price,
                    timestamp,
                    source,
                });
            }
        }

        if events.is_empty() {
            return Err(BacktestError::EmptyDataError(
                "No spot price events found in file".to_string(),
            ));
        }

        // Sort by timestamp
        events.sort_by_key(|e| e.timestamp);

        info!(
            "Loaded {} spot price events, date range: {} to {}",
            events.len(),
            events
                .first()
                .map(|e| e.timestamp.timestamp_millis())
                .unwrap_or(0),
            events
                .last()
                .map(|e| e.timestamp.timestamp_millis())
                .unwrap_or(0)
        );

        Ok(events)
    }
}

/// Parse JSON array of order levels into OrderSummary vector for bids.
/// Bids are sorted by price descending (highest price = best bid first).
fn parse_bid_levels(json_str: &str) -> Result<Vec<OrderSummary>, BacktestError> {
    #[derive(serde::Deserialize)]
    struct Level {
        price: f64,
        size: f64,
    }

    let levels: Vec<Level> = serde_json::from_str(json_str)?;
    let mut orders: Vec<OrderSummary> = levels
        .into_iter()
        .map(|l| OrderSummary {
            price: l.price,
            size: l.size,
        })
        .collect();

    // Parquet data is typically already ordered by best-price-first. Avoid an O(n log n)
    // sort for every snapshot unless we detect it's actually unsorted.
    let is_sorted_desc = orders.windows(2).all(|w| w[0].price >= w[1].price);
    if !is_sorted_desc {
        // Sort bids descending by price (best bid = highest price first)
        orders.sort_by(|a, b| {
            b.price
                .partial_cmp(&a.price)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    Ok(orders)
}

/// Parse JSON array of order levels into OrderSummary vector for asks.
/// Asks are sorted by price ascending (lowest price = best ask first).
fn parse_ask_levels(json_str: &str) -> Result<Vec<OrderSummary>, BacktestError> {
    #[derive(serde::Deserialize)]
    struct Level {
        price: f64,
        size: f64,
    }

    let levels: Vec<Level> = serde_json::from_str(json_str)?;
    let mut orders: Vec<OrderSummary> = levels
        .into_iter()
        .map(|l| OrderSummary {
            price: l.price,
            size: l.size,
        })
        .collect();

    // Parquet data is typically already ordered by best-price-first. Avoid an O(n log n)
    // sort for every snapshot unless we detect it's actually unsorted.
    let is_sorted_asc = orders.windows(2).all(|w| w[0].price <= w[1].price);
    if !is_sorted_asc {
        // Sort asks ascending by price (best ask = lowest price first)
        orders.sort_by(|a, b| {
            a.price
                .partial_cmp(&b.price)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    Ok(orders)
}

/// Get a timestamp column from the batch by name.
/// Expects a column of type TIMESTAMP (Arrow TimestampMillisecond).
fn get_timestamp_column_by_name(
    batch: &arrow::record_batch::RecordBatch,
    name: &str,
) -> Result<TimestampMillisecondArray, BacktestError> {
    let col_idx = batch
        .schema()
        .index_of(name)
        .map_err(|_| BacktestError::InvalidSchemaError(format!("Column '{}' not found", name)))?;

    let col = batch.column(col_idx);

    if let Some(arr) = col.as_any().downcast_ref::<TimestampMillisecondArray>() {
        return Ok(arr.clone());
    }

    Err(BacktestError::InvalidSchemaError(format!(
        "Column '{}' is not a timestamp type",
        name
    )))
}

/// Get a string column from the batch.
fn get_string_column(
    batch: &arrow::record_batch::RecordBatch,
    name: &str,
) -> Result<AnyStringArray, BacktestError> {
    let col_idx = batch
        .schema()
        .index_of(name)
        .map_err(|_| BacktestError::InvalidSchemaError(format!("Column '{}' not found", name)))?;

    let col = batch.column(col_idx);
    if let Some(arr) = col.as_any().downcast_ref::<StringArray>() {
        return Ok(AnyStringArray::Utf8(arr.clone()));
    }
    if let Some(arr) = col
        .as_any()
        .downcast_ref::<arrow::array::LargeStringArray>()
    {
        return Ok(AnyStringArray::LargeUtf8(arr.clone()));
    }

    Err(BacktestError::InvalidSchemaError(format!(
        "Column '{}' is not a string type",
        name
    )))
}

/// Get a float column from the batch.
fn get_float_column(
    batch: &arrow::record_batch::RecordBatch,
    name: &str,
) -> Result<Float64Array, BacktestError> {
    let col_idx = batch
        .schema()
        .index_of(name)
        .map_err(|_| BacktestError::InvalidSchemaError(format!("Column '{}' not found", name)))?;

    let col = batch.column(col_idx);
    col.as_any()
        .downcast_ref::<Float64Array>()
        .cloned()
        .ok_or_else(|| {
            BacktestError::InvalidSchemaError(format!("Column '{}' is not a float type", name))
        })
}

/// Get end_date column from the batch as TimestampNanosecond.
fn get_end_date_column(
    batch: &arrow::record_batch::RecordBatch,
) -> Result<TimestampNanosecondArray, BacktestError> {
    let col_idx = batch.schema().index_of("end_date").map_err(|_| {
        BacktestError::InvalidSchemaError("Column 'end_date' not found".to_string())
    })?;

    let col = batch.column(col_idx);

    col.as_any()
        .downcast_ref::<TimestampNanosecondArray>()
        .cloned()
        .ok_or_else(|| {
            BacktestError::InvalidSchemaError(
                "Column 'end_date' is not a timestamp_ns type".to_string(),
            )
        })
}

/// Convert nanosecond timestamp to RFC3339 string.
fn timestamp_ns_to_rfc3339(nanos: i64) -> String {
    let secs = nanos / 1_000_000_000;
    let nsecs = (nanos % 1_000_000_000) as u32;
    Utc.timestamp_opt(secs, nsecs)
        .single()
        .map(|dt: DateTime<Utc>| dt.to_rfc3339())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ticker_time_range_ms() {
        let ticker = "btc-updown-15m-1768780800";
        let range = parse_ticker_time_range_ms(ticker).expect("parse range");
        assert_eq!(range.start_ms, 1768780800_i64 * 1000);
        assert_eq!(range.end_ms, (1768780800_i64 + 15 * 60) * 1000);
    }

    #[test]
    fn test_derive_time_range_from_patterns_with_buffer() {
        let patterns = Some(vec![
            "btc-updown-15m-1000".to_string(),
            "btc-updown-15m-2000".to_string(),
        ]);
        let buffer = Duration::from_secs(60);
        let range = derive_time_range_from_patterns(&patterns, buffer).expect("range");
        assert_eq!(range.start_ms, (1000_i64 * 1000).saturating_sub(60_000));
        assert_eq!(
            range.end_ms,
            ((2000_i64 + 15 * 60) * 1000).saturating_add(60_000)
        );
    }

    #[test]
    fn test_parse_bid_levels_sorts_descending() {
        // Input: prices in ascending order (as stored in parquet)
        let json = r#"[{"price": 0.45, "size": 200.0}, {"price": 0.50, "size": 100.0}]"#;
        let levels = parse_bid_levels(json).unwrap();

        assert_eq!(levels.len(), 2);
        // Best bid (highest price) should be first
        assert_eq!(levels[0].price, 0.50);
        assert_eq!(levels[0].size, 100.0);
        assert_eq!(levels[1].price, 0.45);
        assert_eq!(levels[1].size, 200.0);
    }

    #[test]
    fn test_parse_ask_levels_sorts_ascending() {
        // Input: prices in descending order (as stored in parquet)
        let json = r#"[{"price": 0.55, "size": 200.0}, {"price": 0.50, "size": 100.0}]"#;
        let levels = parse_ask_levels(json).unwrap();

        assert_eq!(levels.len(), 2);
        // Best ask (lowest price) should be first
        assert_eq!(levels[0].price, 0.50);
        assert_eq!(levels[0].size, 100.0);
        assert_eq!(levels[1].price, 0.55);
        assert_eq!(levels[1].size, 200.0);
    }

    #[test]
    fn test_parse_bid_levels_empty() {
        let json = "[]";
        let levels = parse_bid_levels(json).unwrap();
        assert!(levels.is_empty());
    }

    #[test]
    fn test_parse_ask_levels_empty() {
        let json = "[]";
        let levels = parse_ask_levels(json).unwrap();
        assert!(levels.is_empty());
    }

    #[test]
    fn test_parse_bid_levels_invalid_json() {
        let json = "not json";
        let result = parse_bid_levels(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_ask_levels_invalid_json() {
        let json = "not json";
        let result = parse_ask_levels(json);
        assert!(result.is_err());
    }
}
