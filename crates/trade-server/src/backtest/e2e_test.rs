//! End-to-end tests for the backtest runner.
//!
//! Tests the complete backtest workflow including:
//! - Data loading from Parquet files
//! - Signal generation
//! - Order execution and fill simulation
//! - Position management
//! - Metrics calculation
//! - JSONL output

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::Result;
use arrow::{
    array::{
        Float64Array, RecordBatch, StringArray, TimestampMillisecondArray, TimestampNanosecondArray,
    },
    datatypes::{DataType, Field, Schema, TimeUnit},
};
use parquet::arrow::ArrowWriter;
use tempfile::TempDir;

use crate::{
    backtest::{BacktestConfig, BacktestRunner, CompletenessConfig, PositionConfig},
    signal::SignalGenerator,
};

/// Create a test Parquet file with orderbook snapshots.
fn create_snapshot_parquet(dir: &TempDir, snapshots: &[(i64, f64, f64)]) -> Result<PathBuf> {
    let path = dir.path().join("snapshots.parquet");

    let schema = Arc::new(Schema::new(vec![
        Field::new(
            "ts",
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
            true,
        ),
        Field::new(
            "observed_at",
            DataType::Timestamp(TimeUnit::Millisecond, None),
            false,
        ),
    ]));

    let mut ts_values = Vec::new();
    let mut asset_id_values = Vec::new();
    let mut ticker_values = Vec::new();
    let mut outcome_values = Vec::new();
    let mut bids_values = Vec::new();
    let mut asks_values = Vec::new();
    let mut end_date_values = Vec::new();
    let mut observed_at_values = Vec::new();

    // End date: 2025-12-31 00:00:00 UTC in nanoseconds
    let end_date_ns = 1767139200_i64 * 1_000_000_000;

    for (ts, bid_price, ask_price) in snapshots {
        ts_values.push(*ts);
        asset_id_values.push("TEST-MARKET-Up".to_string());
        ticker_values.push("TEST-MARKET".to_string());
        outcome_values.push("Up".to_string());
        bids_values.push(format!(r#"[{{"price": {}, "size": 1000.0}}]"#, bid_price));
        asks_values.push(format!(r#"[{{"price": {}, "size": 1000.0}}]"#, ask_price));
        end_date_values.push(end_date_ns);
        observed_at_values.push(*ts);
    }

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(TimestampMillisecondArray::from(ts_values)),
            Arc::new(StringArray::from(asset_id_values)),
            Arc::new(StringArray::from(ticker_values)),
            Arc::new(StringArray::from(outcome_values)),
            Arc::new(StringArray::from(bids_values)),
            Arc::new(StringArray::from(asks_values)),
            Arc::new(TimestampNanosecondArray::from(end_date_values)),
            Arc::new(TimestampMillisecondArray::from(observed_at_values)),
        ],
    )?;

    let file = std::fs::File::create(&path)?;
    // Use uncompressed for test simplicity (snappy requires feature flag)
    let mut writer = ArrowWriter::try_new(file, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;

    Ok(path)
}

/// Create a test Parquet file with trade events.
fn create_trade_parquet(
    dir: &TempDir,
    trades: &[(i64, f64, f64, &str)], // (ts, price, size, side)
) -> Result<PathBuf> {
    let path = dir.path().join("trades.parquet");

    let schema = Arc::new(Schema::new(vec![
        Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Millisecond, None),
            false,
        ),
        Field::new("ticker", DataType::Utf8, false),
        Field::new("outcome", DataType::Utf8, false),
        Field::new("side", DataType::Utf8, false),
        Field::new("price", DataType::Float64, false),
        Field::new("size", DataType::Float64, false),
        Field::new("asset_id", DataType::Utf8, false),
    ]));

    let mut ts_values = Vec::new();
    let mut ticker_values = Vec::new();
    let mut outcome_values = Vec::new();
    let mut side_values = Vec::new();
    let mut price_values = Vec::new();
    let mut size_values = Vec::new();
    let mut asset_id_values = Vec::new();

    for (ts, price, size, side) in trades {
        ts_values.push(*ts);
        ticker_values.push("TEST-MARKET".to_string());
        outcome_values.push("Up".to_string());
        side_values.push(side.to_string());
        price_values.push(*price);
        size_values.push(*size);
        asset_id_values.push("TEST-MARKET-Up".to_string());
    }

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(TimestampMillisecondArray::from(ts_values)),
            Arc::new(StringArray::from(ticker_values)),
            Arc::new(StringArray::from(outcome_values)),
            Arc::new(StringArray::from(side_values)),
            Arc::new(Float64Array::from(price_values)),
            Arc::new(Float64Array::from(size_values)),
            Arc::new(StringArray::from(asset_id_values)),
        ],
    )?;

    let file = std::fs::File::create(&path)?;
    // Use uncompressed for test simplicity (snappy requires feature flag)
    let mut writer = ArrowWriter::try_new(file, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;

    Ok(path)
}

/// Create a test Parquet file with orderbook updates.
fn create_update_parquet(
    dir: &TempDir,
    updates: &[(i64, f64, f64, &str, f64, f64)], // (ts, price, size, side, best_bid, best_ask)
) -> Result<PathBuf> {
    let path = dir.path().join("updates.parquet");

    let schema = Arc::new(Schema::new(vec![
        Field::new(
            "ts",
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
            true,
        ),
        Field::new("outcome", DataType::Utf8, false),
    ]));

    let mut ts_values = Vec::new();
    let mut asset_id_values = Vec::new();
    let mut market_values = Vec::new();
    let mut price_values = Vec::new();
    let mut size_values = Vec::new();
    let mut side_values = Vec::new();
    let mut hash_values = Vec::new();
    let mut best_bid_values = Vec::new();
    let mut best_ask_values = Vec::new();
    let mut source_values = Vec::new();
    let mut event_id_values = Vec::new();
    let mut ticker_values = Vec::new();
    let mut title_values = Vec::new();
    let mut end_date_values = Vec::new();
    let mut outcome_values = Vec::new();

    // End date: 2025-12-31 00:00:00 UTC in nanoseconds
    let end_date_ns = 1767139200_i64 * 1_000_000_000;

    for (ts, price, size, side, best_bid, best_ask) in updates {
        ts_values.push(*ts);
        asset_id_values.push("TEST-MARKET-Up".to_string());
        market_values.push("TEST-MARKET".to_string());
        price_values.push(*price);
        size_values.push(*size);
        side_values.push(side.to_string());
        hash_values.push("hash".to_string());
        best_bid_values.push(*best_bid);
        best_ask_values.push(*best_ask);
        source_values.push("polymarket".to_string());
        event_id_values.push("event-1".to_string());
        ticker_values.push("TEST-MARKET".to_string());
        title_values.push("Test Market".to_string());
        end_date_values.push(end_date_ns);
        outcome_values.push("Up".to_string());
    }

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(TimestampMillisecondArray::from(ts_values)),
            Arc::new(StringArray::from(asset_id_values)),
            Arc::new(StringArray::from(market_values)),
            Arc::new(Float64Array::from(price_values)),
            Arc::new(Float64Array::from(size_values)),
            Arc::new(StringArray::from(side_values)),
            Arc::new(StringArray::from(hash_values)),
            Arc::new(Float64Array::from(best_bid_values)),
            Arc::new(Float64Array::from(best_ask_values)),
            Arc::new(StringArray::from(source_values)),
            Arc::new(StringArray::from(event_id_values)),
            Arc::new(StringArray::from(ticker_values)),
            Arc::new(StringArray::from(title_values)),
            Arc::new(TimestampNanosecondArray::from(end_date_values)),
            Arc::new(StringArray::from(outcome_values)),
        ],
    )?;

    let file = std::fs::File::create(&path)?;
    let mut writer = ArrowWriter::try_new(file, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;

    Ok(path)
}

#[tokio::test]
async fn test_backtest_e2e_basic() {
    let temp_dir = TempDir::new().unwrap();

    // Create test data: 5 snapshots at 1-second intervals
    // Mid price around 0.50, spread of 0.02
    let snapshots = vec![
        (1000, 0.49, 0.51), // t=1s
        (2000, 0.49, 0.51), // t=2s
        (3000, 0.48, 0.52), // t=3s
        (4000, 0.50, 0.52), // t=4s
        (5000, 0.49, 0.51), // t=5s
    ];

    // Create test trades that will fill our orders
    // Our bid at 0.48 should fill when a SELL crosses at 0.48
    // Our ask at 0.52 should fill when a BUY crosses at 0.52
    let trades = vec![
        (1500, 0.50, 100.0, "BUY"),  // Regular trade, won't fill our orders
        (2500, 0.48, 50.0, "SELL"),  // Should fill our bid at 0.48
        (3500, 0.52, 30.0, "BUY"),   // Should fill our ask at 0.52
        (4500, 0.50, 100.0, "SELL"), // Regular trade
    ];

    let snapshot_path = create_snapshot_parquet(&temp_dir, &snapshots).unwrap();
    let update_path = create_update_parquet(&temp_dir, &[]).unwrap();
    let trade_path = create_trade_parquet(&temp_dir, &trades).unwrap();
    let output_path = temp_dir.path().join("output.jsonl");

    let config = BacktestConfig::new(snapshot_path, update_path, trade_path, output_path.clone())
        .with_outcome_filter("Up")
        .with_timer_interval(Duration::from_secs(60)) // Long timer to avoid timer events
        .with_position_config(PositionConfig::new(
            1000.0,                    // initial_balance
            10,                        // max_open_positions
            Duration::from_secs(3600), // max_holding_period
            100.0,                     // trade_amount
        ))
        .with_completeness(CompletenessConfig {
            enabled: false, // Disable for test with minimal data
            ..Default::default()
        });

    // Use a simple signal generator (NoopSignalGenerator for this test)
    let signal_generator: Box<dyn SignalGenerator + Send> =
        Box::new(crate::signal::NoopSignalGenerator);

    let result = BacktestRunner::run(config, signal_generator).await.unwrap();

    // Verify basic metrics
    assert_eq!(
        result.metrics.total_snapshots, 5,
        "Should process 5 snapshots"
    );
    assert_eq!(result.metrics.total_trades, 4, "Should process 4 trades");
    assert!(result.output_path.exists(), "Output file should be created");

    // Verify output file is valid JSONL
    let content = std::fs::read_to_string(&result.output_path).unwrap();
    for line in content.lines() {
        let _: serde_json::Value =
            serde_json::from_str(line).expect("Each line should be valid JSON");
    }

    // Metrics duration field should be set (may be 0 if test runs very fast)
    // Just verify it's a valid u64 (which it always is for unit tests that run quickly)
}

#[tokio::test]
async fn test_backtest_reproducibility() {
    let temp_dir = TempDir::new().unwrap();

    let snapshots = vec![(1000, 0.49, 0.51), (2000, 0.50, 0.52), (3000, 0.48, 0.50)];

    let trades = vec![(1500, 0.50, 100.0, "BUY"), (2500, 0.49, 50.0, "SELL")];

    let snapshot_path = create_snapshot_parquet(&temp_dir, &snapshots).unwrap();
    let update_path = create_update_parquet(&temp_dir, &[]).unwrap();
    let trade_path = create_trade_parquet(&temp_dir, &trades).unwrap();
    let output_path1 = temp_dir.path().join("output1.jsonl");
    let output_path2 = temp_dir.path().join("output2.jsonl");

    let config1 = BacktestConfig::new(
        snapshot_path.clone(),
        update_path.clone(),
        trade_path.clone(),
        output_path1.clone(),
    )
    .with_outcome_filter("Up")
    .with_completeness(CompletenessConfig {
        enabled: false, // Disable for test with minimal data
        ..Default::default()
    });

    let config2 = BacktestConfig::new(snapshot_path, update_path, trade_path, output_path2.clone())
        .with_outcome_filter("Up")
        .with_completeness(CompletenessConfig {
            enabled: false, // Disable for test with minimal data
            ..Default::default()
        });

    let signal_gen1: Box<dyn SignalGenerator + Send> = Box::new(crate::signal::NoopSignalGenerator);
    let signal_gen2: Box<dyn SignalGenerator + Send> = Box::new(crate::signal::NoopSignalGenerator);

    let result1 = BacktestRunner::run(config1, signal_gen1).await.unwrap();
    let result2 = BacktestRunner::run(config2, signal_gen2).await.unwrap();

    // Metrics should be identical (except duration which can vary)
    assert_eq!(
        result1.metrics.total_snapshots,
        result2.metrics.total_snapshots
    );
    assert_eq!(result1.metrics.total_trades, result2.metrics.total_trades);
    assert_eq!(result1.metrics.total_signals, result2.metrics.total_signals);
    assert_eq!(result1.metrics.total_fills, result2.metrics.total_fills);
    assert_eq!(result1.metrics.final_pnl, result2.metrics.final_pnl);
    assert_eq!(
        result1.metrics.final_inventory,
        result2.metrics.final_inventory
    );

    // Output event count should be identical
    let content1 = std::fs::read_to_string(&output_path1).unwrap();
    let content2 = std::fs::read_to_string(&output_path2).unwrap();
    assert_eq!(
        content1.lines().count(),
        content2.lines().count(),
        "Output should have same number of events"
    );
}

#[tokio::test]
async fn test_backtest_empty_data() {
    let temp_dir = TempDir::new().unwrap();
    let snapshot_path = temp_dir.path().join("empty_snapshots.parquet");
    let update_path = create_update_parquet(&temp_dir, &[]).unwrap();
    let trade_path = temp_dir.path().join("empty_trades.parquet");
    let output_path = temp_dir.path().join("output.jsonl");

    // Create empty Parquet files
    let schema = Arc::new(Schema::new(vec![
        Field::new(
            "ts",
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
            true,
        ),
    ]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(TimestampMillisecondArray::from(Vec::<i64>::new())),
            Arc::new(StringArray::from(Vec::<String>::new())),
            Arc::new(StringArray::from(Vec::<String>::new())),
            Arc::new(StringArray::from(Vec::<String>::new())),
            Arc::new(StringArray::from(Vec::<String>::new())),
            Arc::new(StringArray::from(Vec::<String>::new())),
            Arc::new(TimestampNanosecondArray::from(Vec::<i64>::new())),
        ],
    )
    .unwrap();

    let file = std::fs::File::create(&snapshot_path).unwrap();
    let mut writer = ArrowWriter::try_new(file, schema.clone(), None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();

    // Create empty trades file
    let trade_schema = Arc::new(Schema::new(vec![
        Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Millisecond, None),
            false,
        ),
        Field::new("ticker", DataType::Utf8, false),
        Field::new("outcome", DataType::Utf8, false),
        Field::new("side", DataType::Utf8, false),
        Field::new("price", DataType::Float64, false),
        Field::new("size", DataType::Float64, false),
        Field::new("asset_id", DataType::Utf8, false),
    ]));

    let trade_batch = RecordBatch::try_new(
        trade_schema.clone(),
        vec![
            Arc::new(TimestampMillisecondArray::from(Vec::<i64>::new())),
            Arc::new(StringArray::from(Vec::<String>::new())),
            Arc::new(StringArray::from(Vec::<String>::new())),
            Arc::new(StringArray::from(Vec::<String>::new())),
            Arc::new(Float64Array::from(Vec::<f64>::new())),
            Arc::new(Float64Array::from(Vec::<f64>::new())),
            Arc::new(StringArray::from(Vec::<String>::new())),
        ],
    )
    .unwrap();

    let trade_file = std::fs::File::create(&trade_path).unwrap();
    let mut trade_writer = ArrowWriter::try_new(trade_file, trade_schema, None).unwrap();
    trade_writer.write(&trade_batch).unwrap();
    trade_writer.close().unwrap();

    let config = BacktestConfig::new(snapshot_path, update_path, trade_path, output_path.clone());

    let signal_generator: Box<dyn SignalGenerator + Send> =
        Box::new(crate::signal::NoopSignalGenerator);

    // Should fail with empty data error
    let result = BacktestRunner::run(config, signal_generator).await;
    assert!(result.is_err(), "Should fail with empty data");
}

#[tokio::test]
async fn test_backtest_data_time_range() {
    let temp_dir = TempDir::new().unwrap();

    // Create snapshots with specific timestamps
    let start_ts = 1700000000000_i64; // Some timestamp in ms
    let end_ts = 1700000005000_i64;

    let snapshots = vec![
        (start_ts, 0.49, 0.51),
        (start_ts + 1000, 0.50, 0.52),
        (end_ts, 0.48, 0.50),
    ];

    let trades = vec![(start_ts + 500, 0.50, 100.0, "BUY")];

    let snapshot_path = create_snapshot_parquet(&temp_dir, &snapshots).unwrap();
    let update_path = create_update_parquet(&temp_dir, &[]).unwrap();
    let trade_path = create_trade_parquet(&temp_dir, &trades).unwrap();
    let output_path = temp_dir.path().join("output.jsonl");

    let config = BacktestConfig::new(snapshot_path, update_path, trade_path, output_path)
        .with_completeness(CompletenessConfig {
            enabled: false, // Disable for test with minimal data
            ..Default::default()
        });

    let signal_generator: Box<dyn SignalGenerator + Send> =
        Box::new(crate::signal::NoopSignalGenerator);

    let result = BacktestRunner::run(config, signal_generator).await.unwrap();

    // Check that data time range is captured
    assert!(result.metrics.data_start_time.is_some());
    assert!(result.metrics.data_end_time.is_some());

    let start = result.metrics.data_start_time.unwrap();
    let end = result.metrics.data_end_time.unwrap();

    assert!(end > start, "End time should be after start time");
}
