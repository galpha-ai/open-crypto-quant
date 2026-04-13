//! Synthetic snapshot generation utilities for research/backtests.
//!
//! Produces an updates-applied snapshot stream using the same `OrderbookTracker`
//! semantics as live/backtest execution, and can persist a compact multi-level
//! Parquet for research workflows.

use std::{fs::File, path::Path, sync::Arc};

use arrow::{
    array::{
        ArrayRef, Float64Array, RecordBatch, StringArray, TimestampMillisecondArray,
        TimestampNanosecondArray,
    },
    datatypes::{DataType, Field, Schema, TimeUnit},
};
use parquet::arrow::ArrowWriter;
use popeyes_trading_types::{OrderbookSnapshotEvent, OrderbookUpdateEvent};
use tracing::info;

use crate::orderbook_tracker::OrderbookTracker;

use super::{BacktestConfig, BacktestError, ParquetLoader};

#[derive(Debug, Clone)]
pub struct SyntheticBboSnapshotWriteOptions {
    /// If true, apply Polymarket binary CTF mirroring when processing updates.
    ///
    /// For each update on one outcome, synthesize and apply a mirrored update on the opposite
    /// outcome, with:
    /// - `outcome' = other(outcome)` (Up<->Down, Yes<->No)
    /// - `side' = opposite(side)` (BUY<->SELL)
    /// - `price' = 1.0 - price` (rounded to 0.001 ticks)
    ///
    /// The mirrored update is used only to keep tracked book state consistent; it is not emitted
    /// as a row in the synthetic BBO output.
    pub mirror_polymarket_binary_updates: bool,

    /// Number of orderbook levels to persist per side (0-indexed columns).
    ///
    /// Level 0 corresponds to the best bid/ask.
    pub book_levels: usize,
}

impl Default for SyntheticBboSnapshotWriteOptions {
    fn default() -> Self {
        Self {
            mirror_polymarket_binary_updates: false,
            book_levels: 5,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SyntheticSnapshotWriteStats {
    pub rows_written: u64,
    pub raw_snapshots_written: u64,
    pub synthetic_snapshots_written: u64,
    pub updates_ignored_before_seed: u64,
    pub mirrored_updates_applied: u64,
    pub mirrored_updates_ignored_before_seed: u64,
    pub mirrored_updates_missing_pair: u64,
}

fn parse_end_date_ns(snapshot: &OrderbookSnapshotEvent) -> Option<i64> {
    let end_date = snapshot
        .market_metadata
        .as_ref()
        .map(|m| m.end_date.as_str())?;
    if end_date.is_empty() {
        return None;
    }

    chrono::DateTime::parse_from_rfc3339(end_date)
        .ok()?
        .timestamp_nanos_opt()
}

fn snapshot_ticker(snapshot: &OrderbookSnapshotEvent) -> String {
    snapshot
        .market_metadata
        .as_ref()
        .map(|m| m.ticker.clone())
        .unwrap_or_else(|| snapshot.market.clone())
}

fn snapshot_outcome(snapshot: &OrderbookSnapshotEvent) -> String {
    snapshot
        .market_metadata
        .as_ref()
        .and_then(|m| m.outcome.clone())
        .unwrap_or_default()
}

fn snapshot_hash(snapshot: &OrderbookSnapshotEvent) -> String {
    snapshot.hash.clone()
}

fn opposite_polymarket_binary_outcome(outcome: &str) -> Option<&'static str> {
    match outcome {
        "Up" => Some("Down"),
        "Down" => Some("Up"),
        "Yes" => Some("No"),
        "No" => Some("Yes"),
        _ => None,
    }
}

fn mirror_price_1milli(price: f64) -> f64 {
    let ticks = (price * 1000.0).round();
    let mirrored_ticks = 1000.0 - ticks;
    mirrored_ticks / 1000.0
}

fn synthesize_polymarket_binary_mirrored_update(
    update: &OrderbookUpdateEvent,
    ticker_outcome_to_asset_id: &std::collections::HashMap<(String, String), String>,
) -> Option<OrderbookUpdateEvent> {
    let meta = update.market_metadata.as_ref()?;
    let ticker = meta.ticker.clone();
    let outcome = meta.outcome.as_deref()?;
    let other_outcome = opposite_polymarket_binary_outcome(outcome)?;
    let other_asset_id = ticker_outcome_to_asset_id
        .get(&(ticker.clone(), other_outcome.to_string()))
        .cloned()?;

    let side = match update.side {
        popeyes_trading_types::TradeSide::Buy => popeyes_trading_types::TradeSide::Sell,
        popeyes_trading_types::TradeSide::Sell => popeyes_trading_types::TradeSide::Buy,
    };

    Some(OrderbookUpdateEvent {
        asset_id: other_asset_id,
        market: update.market.clone(),
        price: mirror_price_1milli(update.price),
        size: update.size,
        side,
        hash: update.hash.clone(),
        best_bid: mirror_price_1milli(update.best_ask),
        best_ask: mirror_price_1milli(update.best_bid),
        timestamp: update.timestamp,
        observed_at: update.observed_at,
        source: update.source.clone(),
        market_metadata: update.market_metadata.clone(),
    })
}

fn push_snapshot_row(
    ts_ms: &mut Vec<i64>,
    observed_at_ms: &mut Vec<i64>,
    asset_id: &mut Vec<String>,
    ticker: &mut Vec<String>,
    outcome: &mut Vec<String>,
    bid_prices: &mut [Vec<Option<f64>>],
    bid_sizes: &mut [Vec<Option<f64>>],
    ask_prices: &mut [Vec<Option<f64>>],
    ask_sizes: &mut [Vec<Option<f64>>],
    end_date_ns: &mut Vec<Option<i64>>,
    hash: &mut Vec<String>,
    snapshot: &OrderbookSnapshotEvent,
    book_levels: usize,
) {
    ts_ms.push(snapshot.timestamp);
    observed_at_ms.push(snapshot.observed_at.timestamp_millis());
    asset_id.push(snapshot.asset_id.clone());
    ticker.push(snapshot_ticker(snapshot));
    outcome.push(snapshot_outcome(snapshot));
    push_book_levels(
        snapshot,
        book_levels,
        bid_prices,
        bid_sizes,
        ask_prices,
        ask_sizes,
    );
    end_date_ns.push(parse_end_date_ns(snapshot));
    hash.push(snapshot_hash(snapshot));
}

fn push_update_row(
    ts_ms: &mut Vec<i64>,
    observed_at_ms: &mut Vec<i64>,
    asset_id: &mut Vec<String>,
    ticker: &mut Vec<String>,
    outcome: &mut Vec<String>,
    bid_prices: &mut [Vec<Option<f64>>],
    bid_sizes: &mut [Vec<Option<f64>>],
    ask_prices: &mut [Vec<Option<f64>>],
    ask_sizes: &mut [Vec<Option<f64>>],
    end_date_ns: &mut Vec<Option<i64>>,
    hash: &mut Vec<String>,
    snapshot: &OrderbookSnapshotEvent,
    update: &OrderbookUpdateEvent,
    book_levels: usize,
) {
    ts_ms.push(update.timestamp);
    observed_at_ms.push(update.observed_at.timestamp_millis());
    asset_id.push(update.asset_id.clone());
    ticker.push(snapshot_ticker(snapshot));
    outcome.push(snapshot_outcome(snapshot));
    push_book_levels(
        snapshot,
        book_levels,
        bid_prices,
        bid_sizes,
        ask_prices,
        ask_sizes,
    );
    end_date_ns.push(parse_end_date_ns(snapshot));
    hash.push(update.hash.clone());
}

fn push_book_levels(
    snapshot: &OrderbookSnapshotEvent,
    book_levels: usize,
    bid_prices: &mut [Vec<Option<f64>>],
    bid_sizes: &mut [Vec<Option<f64>>],
    ask_prices: &mut [Vec<Option<f64>>],
    ask_sizes: &mut [Vec<Option<f64>>],
) {
    for level in 0..book_levels {
        let bid = snapshot.bids.get(level);
        let ask = snapshot.asks.get(level);

        bid_prices[level].push(bid.map(|l| l.price));
        bid_sizes[level].push(bid.map(|l| l.size));
        ask_prices[level].push(ask.map(|l| l.price));
        ask_sizes[level].push(ask.map(|l| l.size));
    }
}

fn build_multi_level_schema(book_levels: usize) -> Schema {
    let mut fields = vec![
        Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Millisecond, None),
            false,
        ),
        Field::new(
            "observed_at",
            DataType::Timestamp(TimeUnit::Millisecond, None),
            false,
        ),
        Field::new("asset_id", DataType::Utf8, false),
        Field::new("ticker", DataType::Utf8, false),
        Field::new("outcome", DataType::Utf8, false),
    ];

    for level in 0..book_levels {
        fields.push(Field::new(
            format!("bid_price_{level}"),
            DataType::Float64,
            true,
        ));
        fields.push(Field::new(
            format!("bid_size_{level}"),
            DataType::Float64,
            true,
        ));
    }

    for level in 0..book_levels {
        fields.push(Field::new(
            format!("ask_price_{level}"),
            DataType::Float64,
            true,
        ));
        fields.push(Field::new(
            format!("ask_size_{level}"),
            DataType::Float64,
            true,
        ));
    }

    fields.push(Field::new(
        "end_date",
        DataType::Timestamp(TimeUnit::Nanosecond, None),
        true,
    ));
    fields.push(Field::new("hash", DataType::Utf8, true));

    Schema::new(fields)
}

fn flush_multi_level_batch(
    writer: &mut ArrowWriter<File>,
    schema: Arc<Schema>,
    ts_ms: &mut Vec<i64>,
    observed_at_ms: &mut Vec<i64>,
    asset_id: &mut Vec<String>,
    ticker: &mut Vec<String>,
    outcome: &mut Vec<String>,
    bid_prices: &mut [Vec<Option<f64>>],
    bid_sizes: &mut [Vec<Option<f64>>],
    ask_prices: &mut [Vec<Option<f64>>],
    ask_sizes: &mut [Vec<Option<f64>>],
    end_date_ns: &mut Vec<Option<i64>>,
    hash: &mut Vec<String>,
    book_levels: usize,
) -> Result<usize, BacktestError> {
    if ts_ms.is_empty() {
        return Ok(0);
    }

    let mut columns: Vec<ArrayRef> = Vec::with_capacity(7 + (4 * book_levels));
    columns.push(Arc::new(TimestampMillisecondArray::from(std::mem::take(
        ts_ms,
    ))));
    columns.push(Arc::new(TimestampMillisecondArray::from(std::mem::take(
        observed_at_ms,
    ))));
    columns.push(Arc::new(StringArray::from(std::mem::take(asset_id))));
    columns.push(Arc::new(StringArray::from(std::mem::take(ticker))));
    columns.push(Arc::new(StringArray::from(std::mem::take(outcome))));

    for level in 0..book_levels {
        columns.push(Arc::new(Float64Array::from(std::mem::take(
            &mut bid_prices[level],
        ))));
        columns.push(Arc::new(Float64Array::from(std::mem::take(
            &mut bid_sizes[level],
        ))));
    }

    for level in 0..book_levels {
        columns.push(Arc::new(Float64Array::from(std::mem::take(
            &mut ask_prices[level],
        ))));
        columns.push(Arc::new(Float64Array::from(std::mem::take(
            &mut ask_sizes[level],
        ))));
    }

    columns.push(Arc::new(TimestampNanosecondArray::from(std::mem::take(
        end_date_ns,
    ))));
    columns.push(Arc::new(StringArray::from(std::mem::take(hash))));

    let batch = RecordBatch::try_new(schema, columns)?;

    let row_count = batch.num_rows();
    writer.write(&batch)?;
    Ok(row_count)
}

/// Write a compact multi-level synthetic snapshot Parquet suitable for research.
///
/// The output is an updates-applied snapshot stream generated using `OrderbookTracker`:
/// - raw snapshots are applied and emitted
/// - updates are applied and emitted as synthetic snapshots only after an initial snapshot seed
/// - when snapshot and update timestamps are equal, the snapshot is applied first (snapshot-first)
///
/// Output schema:
/// - `ts` (TIMESTAMP_MS): source timestamp supplied by the data feed (not controlled by this server)
/// - `observed_at` (TIMESTAMP_MS): time the server observed the event (from snapshot or update);
///   use this for ordering from the server's perspective
/// - `asset_id` (STRING)
/// - `ticker` (STRING)
/// - `outcome` (STRING)
/// - `bid_price_{i}` / `bid_size_{i}` (FLOAT64, nullable) for i in 0..book_levels
/// - `ask_price_{i}` / `ask_size_{i}` (FLOAT64, nullable) for i in 0..book_levels
/// - `end_date` (TIMESTAMP_NS, nullable)
/// - `hash` (STRING, nullable)
pub fn write_synthetic_bbo_snapshots_parquet(
    config: &BacktestConfig,
    output_path: &Path,
    batch_size: usize,
) -> Result<SyntheticSnapshotWriteStats, BacktestError> {
    write_synthetic_bbo_snapshots_parquet_with_options(
        config,
        output_path,
        batch_size,
        SyntheticBboSnapshotWriteOptions::default(),
    )
}

pub fn write_synthetic_bbo_snapshots_parquet_with_options(
    config: &BacktestConfig,
    output_path: &Path,
    batch_size: usize,
    options: SyntheticBboSnapshotWriteOptions,
) -> Result<SyntheticSnapshotWriteStats, BacktestError> {
    let loader = ParquetLoader::with_filters_and_time_range(
        config.outcome_filter.clone(),
        config.ticker_patterns.clone(),
        config.ticker_time_range_filter,
        config.ticker_time_range_buffer,
    );

    let mut snapshots = loader.load_snapshots(&config.snapshot_path)?;
    let mut updates = loader.load_updates(&config.update_path)?;
    snapshots.sort_by_key(|s| s.timestamp);
    updates.sort_by_key(|u| u.timestamp);

    let mut ticker_outcome_to_asset_id: std::collections::HashMap<(String, String), String> =
        std::collections::HashMap::new();
    if options.mirror_polymarket_binary_updates {
        for snapshot in &snapshots {
            let meta = match snapshot.market_metadata.as_ref() {
                Some(m) => m,
                None => continue,
            };
            let outcome = match meta.outcome.as_deref() {
                Some(o) => o,
                None => continue,
            };
            ticker_outcome_to_asset_id.insert(
                (meta.ticker.clone(), outcome.to_string()),
                snapshot.asset_id.clone(),
            );
        }
    }

    let book_levels = options.book_levels.max(1);
    let schema = Arc::new(build_multi_level_schema(book_levels));

    let file = File::create(output_path)?;
    let mut writer = ArrowWriter::try_new(file, Arc::clone(&schema), None)?;

    let mut tracker = OrderbookTracker::new();
    let mut snapshot_iter = snapshots.into_iter().peekable();
    let mut update_iter = updates.into_iter().peekable();

    let mut stats = SyntheticSnapshotWriteStats::default();

    let mut ts_ms: Vec<i64> = Vec::with_capacity(batch_size);
    let mut observed_at_ms: Vec<i64> = Vec::with_capacity(batch_size);
    let mut asset_id: Vec<String> = Vec::with_capacity(batch_size);
    let mut ticker: Vec<String> = Vec::with_capacity(batch_size);
    let mut outcome: Vec<String> = Vec::with_capacity(batch_size);
    let mut bid_prices: Vec<Vec<Option<f64>>> = (0..book_levels)
        .map(|_| Vec::with_capacity(batch_size))
        .collect();
    let mut bid_sizes: Vec<Vec<Option<f64>>> = (0..book_levels)
        .map(|_| Vec::with_capacity(batch_size))
        .collect();
    let mut ask_prices: Vec<Vec<Option<f64>>> = (0..book_levels)
        .map(|_| Vec::with_capacity(batch_size))
        .collect();
    let mut ask_sizes: Vec<Vec<Option<f64>>> = (0..book_levels)
        .map(|_| Vec::with_capacity(batch_size))
        .collect();
    let mut end_date_ns: Vec<Option<i64>> = Vec::with_capacity(batch_size);
    let mut hash: Vec<String> = Vec::with_capacity(batch_size);

    loop {
        let next_snapshot_ts = snapshot_iter.peek().map(|s| s.timestamp);
        let next_update_ts = update_iter.peek().map(|u| u.timestamp);

        match (next_snapshot_ts, next_update_ts) {
            (Some(_), Some(_)) if next_snapshot_ts <= next_update_ts => {
                let snapshot = snapshot_iter.next().expect("peeked snapshot exists");
                let tracked = tracker.apply_snapshot(&snapshot);
                push_snapshot_row(
                    &mut ts_ms,
                    &mut observed_at_ms,
                    &mut asset_id,
                    &mut ticker,
                    &mut outcome,
                    &mut bid_prices,
                    &mut bid_sizes,
                    &mut ask_prices,
                    &mut ask_sizes,
                    &mut end_date_ns,
                    &mut hash,
                    &tracked,
                    book_levels,
                );
                stats.raw_snapshots_written += 1;
            }
            (Some(_), Some(_)) | (None, Some(_)) => {
                let update = update_iter.next().expect("peeked update exists");
                let applied = tracker.apply_update(&update);
                if let Some(ref synthetic) = applied {
                    push_update_row(
                        &mut ts_ms,
                        &mut observed_at_ms,
                        &mut asset_id,
                        &mut ticker,
                        &mut outcome,
                        &mut bid_prices,
                        &mut bid_sizes,
                        &mut ask_prices,
                        &mut ask_sizes,
                        &mut end_date_ns,
                        &mut hash,
                        synthetic,
                        &update,
                        book_levels,
                    );
                    stats.synthetic_snapshots_written += 1;
                } else {
                    stats.updates_ignored_before_seed += 1;
                }

                if options.mirror_polymarket_binary_updates {
                    match synthesize_polymarket_binary_mirrored_update(
                        &update,
                        &ticker_outcome_to_asset_id,
                    ) {
                        Some(mirrored) => match tracker.apply_update(&mirrored) {
                            Some(_) => stats.mirrored_updates_applied += 1,
                            None => stats.mirrored_updates_ignored_before_seed += 1,
                        },
                        None => stats.mirrored_updates_missing_pair += 1,
                    }
                }
            }
            (Some(_), None) => {
                let snapshot = snapshot_iter.next().expect("peeked snapshot exists");
                let tracked = tracker.apply_snapshot(&snapshot);
                push_snapshot_row(
                    &mut ts_ms,
                    &mut observed_at_ms,
                    &mut asset_id,
                    &mut ticker,
                    &mut outcome,
                    &mut bid_prices,
                    &mut bid_sizes,
                    &mut ask_prices,
                    &mut ask_sizes,
                    &mut end_date_ns,
                    &mut hash,
                    &tracked,
                    book_levels,
                );
                stats.raw_snapshots_written += 1;
            }
            (None, None) => break,
        }

        if ts_ms.len() >= batch_size {
            let wrote = flush_multi_level_batch(
                &mut writer,
                Arc::clone(&schema),
                &mut ts_ms,
                &mut observed_at_ms,
                &mut asset_id,
                &mut ticker,
                &mut outcome,
                &mut bid_prices,
                &mut bid_sizes,
                &mut ask_prices,
                &mut ask_sizes,
                &mut end_date_ns,
                &mut hash,
                book_levels,
            )?;
            stats.rows_written += wrote as u64;
        }
    }

    let wrote = flush_multi_level_batch(
        &mut writer,
        Arc::clone(&schema),
        &mut ts_ms,
        &mut observed_at_ms,
        &mut asset_id,
        &mut ticker,
        &mut outcome,
        &mut bid_prices,
        &mut bid_sizes,
        &mut ask_prices,
        &mut ask_sizes,
        &mut end_date_ns,
        &mut hash,
        book_levels,
    )?;
    stats.rows_written += wrote as u64;

    writer.close()?;

    info!(
        output_path = %output_path.display(),
        rows_written = stats.rows_written,
        raw_snapshots_written = stats.raw_snapshots_written,
        synthetic_snapshots_written = stats.synthetic_snapshots_written,
        updates_ignored_before_seed = stats.updates_ignored_before_seed,
        book_levels,
        "Wrote synthetic multi-level snapshot parquet"
    );

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use std::{fs::File, sync::Arc};

    use arrow::{
        array::{
            Float64Array, RecordBatch, StringArray, TimestampMillisecondArray,
            TimestampNanosecondArray,
        },
        datatypes::{DataType, Field, Schema, TimeUnit},
    };
    use parquet::arrow::{ArrowWriter, arrow_reader::ParquetRecordBatchReaderBuilder};
    use tempfile::TempDir;

    use super::*;

    fn write_snapshot_parquet(path: &std::path::Path) {
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                "ts",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new(
                "observed_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("asset_id", DataType::Utf8, false),
            Field::new("ticker", DataType::Utf8, false),
            Field::new("outcome", DataType::Utf8, false),
            Field::new("bids", DataType::Utf8, false),
            Field::new("asks", DataType::Utf8, false),
            Field::new(
                "end_date",
                DataType::Timestamp(TimeUnit::Nanosecond, None),
                false,
            ),
        ]));

        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(TimestampMillisecondArray::from(vec![1000_i64])),
                Arc::new(TimestampMillisecondArray::from(vec![1000_i64])),
                Arc::new(StringArray::from(vec!["asset-1"])),
                Arc::new(StringArray::from(vec!["TICKER"])),
                Arc::new(StringArray::from(vec!["Up"])),
                Arc::new(StringArray::from(vec![
                    r#"[{"price": 0.49, "size": 1.0}, {"price": 0.48, "size": 2.0}]"#,
                ])),
                Arc::new(StringArray::from(vec![
                    r#"[{"price": 0.51, "size": 1.5}, {"price": 0.52, "size": 3.0}]"#,
                ])),
                Arc::new(TimestampNanosecondArray::from(vec![2_000_000_000_i64])),
            ],
        )
        .expect("snapshot batch");

        let file = File::create(path).expect("create snapshot parquet");
        let mut writer = ArrowWriter::try_new(file, schema, None).expect("snapshot writer");
        writer.write(&batch).expect("write snapshot batch");
        writer.close().expect("close snapshot writer");
    }

    fn write_update_parquet(path: &std::path::Path) {
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                "ts",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new(
                "observed_at",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("asset_id", DataType::Utf8, false),
            Field::new("market", DataType::Utf8, false),
            Field::new("price", DataType::Float64, false),
            Field::new("size", DataType::Float64, false),
            Field::new("side", DataType::Utf8, false),
            Field::new("hash", DataType::Utf8, false),
            Field::new("best_bid", DataType::Float64, false),
            Field::new("best_ask", DataType::Float64, false),
            Field::new("source", DataType::Utf8, false),
            Field::new("event_id", DataType::Utf8, false),
            Field::new("ticker", DataType::Utf8, false),
            Field::new("title", DataType::Utf8, false),
            Field::new(
                "end_date",
                DataType::Timestamp(TimeUnit::Nanosecond, None),
                false,
            ),
            Field::new("outcome", DataType::Utf8, false),
        ]));

        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(TimestampMillisecondArray::from(vec![1100_i64])),
                Arc::new(TimestampMillisecondArray::from(vec![1100_i64])),
                Arc::new(StringArray::from(vec!["asset-1"])),
                Arc::new(StringArray::from(vec!["TICKER"])),
                Arc::new(Float64Array::from(vec![0.49_f64])),
                Arc::new(Float64Array::from(vec![4.0_f64])),
                Arc::new(StringArray::from(vec!["BUY"])),
                Arc::new(StringArray::from(vec!["update-hash"])),
                Arc::new(Float64Array::from(vec![0.50_f64])),
                Arc::new(Float64Array::from(vec![0.51_f64])),
                Arc::new(StringArray::from(vec!["polymarket"])),
                Arc::new(StringArray::from(vec!["event-1"])),
                Arc::new(StringArray::from(vec!["TICKER"])),
                Arc::new(StringArray::from(vec!["Title"])),
                Arc::new(TimestampNanosecondArray::from(vec![2_000_000_000_i64])),
                Arc::new(StringArray::from(vec!["Up"])),
            ],
        )
        .expect("update batch");

        let file = File::create(path).expect("create update parquet");
        let mut writer = ArrowWriter::try_new(file, schema, None).expect("update writer");
        writer.write(&batch).expect("write update batch");
        writer.close().expect("close update writer");
    }

    #[test]
    fn writes_multi_level_book_columns() {
        let temp_dir = TempDir::new().expect("temp dir");
        let snapshot_path = temp_dir.path().join("snapshots.parquet");
        let update_path = temp_dir.path().join("updates.parquet");
        let trade_path = temp_dir.path().join("trades.parquet");
        let output_path = temp_dir.path().join("synthetic.parquet");

        write_snapshot_parquet(&snapshot_path);
        write_update_parquet(&update_path);
        File::create(&trade_path).expect("create trade placeholder");

        let config =
            BacktestConfig::new(snapshot_path, update_path, trade_path, output_path.clone())
                .with_timer_interval(std::time::Duration::from_secs(1));

        let options = SyntheticBboSnapshotWriteOptions {
            mirror_polymarket_binary_updates: false,
            book_levels: 2,
        };

        let stats =
            write_synthetic_bbo_snapshots_parquet_with_options(&config, &output_path, 8, options)
                .expect("write synthetic parquet");
        assert_eq!(stats.rows_written, 2);

        let file = File::open(&output_path).expect("open output parquet");
        let reader = ParquetRecordBatchReaderBuilder::try_new(file)
            .expect("reader builder")
            .build()
            .expect("build reader");
        let batch = reader
            .into_iter()
            .next()
            .expect("batch exists")
            .expect("read batch");

        let schema = batch.schema();
        for name in [
            "bid_price_0",
            "bid_size_0",
            "bid_price_1",
            "bid_size_1",
            "ask_price_0",
            "ask_size_0",
            "ask_price_1",
            "ask_size_1",
        ] {
            assert!(schema.index_of(name).is_ok(), "missing column: {name}");
        }

        let bid_price_0 = batch
            .column(schema.index_of("bid_price_0").expect("bid_price_0 idx"))
            .as_any()
            .downcast_ref::<Float64Array>()
            .expect("bid_price_0 type");
        let bid_size_0 = batch
            .column(schema.index_of("bid_size_0").expect("bid_size_0 idx"))
            .as_any()
            .downcast_ref::<Float64Array>()
            .expect("bid_size_0 type");
        let bid_price_1 = batch
            .column(schema.index_of("bid_price_1").expect("bid_price_1 idx"))
            .as_any()
            .downcast_ref::<Float64Array>()
            .expect("bid_price_1 type");
        let bid_size_1 = batch
            .column(schema.index_of("bid_size_1").expect("bid_size_1 idx"))
            .as_any()
            .downcast_ref::<Float64Array>()
            .expect("bid_size_1 type");

        let ask_price_0 = batch
            .column(schema.index_of("ask_price_0").expect("ask_price_0 idx"))
            .as_any()
            .downcast_ref::<Float64Array>()
            .expect("ask_price_0 type");
        let ask_size_0 = batch
            .column(schema.index_of("ask_size_0").expect("ask_size_0 idx"))
            .as_any()
            .downcast_ref::<Float64Array>()
            .expect("ask_size_0 type");
        let ask_price_1 = batch
            .column(schema.index_of("ask_price_1").expect("ask_price_1 idx"))
            .as_any()
            .downcast_ref::<Float64Array>()
            .expect("ask_price_1 type");
        let ask_size_1 = batch
            .column(schema.index_of("ask_size_1").expect("ask_size_1 idx"))
            .as_any()
            .downcast_ref::<Float64Array>()
            .expect("ask_size_1 type");

        assert_eq!(bid_price_0.value(0), 0.49);
        assert_eq!(bid_size_0.value(0), 1.0);
        assert_eq!(bid_price_1.value(0), 0.48);
        assert_eq!(bid_size_1.value(0), 2.0);
        assert_eq!(ask_price_0.value(0), 0.51);
        assert_eq!(ask_size_0.value(0), 1.5);
        assert_eq!(ask_price_1.value(0), 0.52);
        assert_eq!(ask_size_1.value(0), 3.0);

        assert_eq!(bid_price_0.value(1), 0.50);
        assert_eq!(bid_size_0.value(1), 4.0);
        assert_eq!(bid_price_1.value(1), 0.48);
        assert_eq!(bid_size_1.value(1), 2.0);
        assert_eq!(ask_price_0.value(1), 0.51);
        assert_eq!(ask_size_0.value(1), 1.5);
        assert_eq!(ask_price_1.value(1), 0.52);
        assert_eq!(ask_size_1.value(1), 3.0);

        let observed_at = batch
            .column(schema.index_of("observed_at").expect("observed_at idx"))
            .as_any()
            .downcast_ref::<TimestampMillisecondArray>()
            .expect("observed_at type");
        assert_eq!(observed_at.value(1), 1100);
    }
}
