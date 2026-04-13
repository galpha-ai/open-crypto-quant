//! Integration tests for ParquetLoader using real data files.
//!
//! These tests are disabled by default and require environment variables to run:
//! - `BACKTEST_SNAPSHOT_PATH`: Path to a Parquet file with orderbook snapshots
//! - `BACKTEST_TRADE_PATH`: Path to a Parquet file with trade events
//!
//! # Example
//!
//! ```bash
//! BACKTEST_SNAPSHOT_PATH=/path/to/snapshots.parquet \
//! BACKTEST_TRADE_PATH=/path/to/trades.parquet \
//! cargo test --package trade_server loader_integration -- --ignored
//! ```

use std::env;
use std::fs::File;
use std::path::Path;
use std::time::Instant;

use super::ParquetLoader;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

fn get_snapshot_path() -> Option<String> {
    env::var("BACKTEST_SNAPSHOT_PATH").ok()
}

fn get_trade_path() -> Option<String> {
    env::var("BACKTEST_TRADE_PATH").ok()
}

#[test]
#[ignore = "requires BACKTEST_SNAPSHOT_PATH environment variable"]
fn test_load_real_snapshots() {
    let path_str = get_snapshot_path().expect("BACKTEST_SNAPSHOT_PATH must be set");
    let path = Path::new(&path_str);

    assert!(path.exists(), "Snapshot file does not exist: {}", path_str);

    let loader = ParquetLoader::new(None);
    let snapshots = loader
        .load_snapshots(path)
        .expect("Failed to load snapshots");

    // Verify we loaded some data
    assert!(!snapshots.is_empty(), "Should load at least one snapshot");

    // Verify data integrity
    let first = snapshots.first().unwrap();
    assert!(!first.asset_id.is_empty(), "asset_id should not be empty");
    assert!(!first.market.is_empty(), "market should not be empty");
    assert!(first.timestamp > 0, "timestamp should be positive");

    // Verify bids and asks are present
    assert!(
        !first.bids.is_empty() || !first.asks.is_empty(),
        "Should have at least one side of orderbook"
    );

    // Verify chronological order
    for window in snapshots.windows(2) {
        assert!(
            window[1].timestamp >= window[0].timestamp,
            "Snapshots should be sorted by timestamp"
        );
    }

    // Verify market metadata if present
    if let Some(ref metadata) = first.market_metadata {
        assert!(
            !metadata.ticker.is_empty(),
            "ticker should not be empty in metadata"
        );
        assert!(
            metadata.outcome.is_some(),
            "outcome should be present in metadata"
        );
    }

    println!("Loaded {} snapshots successfully", snapshots.len());
    println!(
        "Time range: {} to {}",
        snapshots.first().unwrap().timestamp,
        snapshots.last().unwrap().timestamp
    );
}

#[test]
#[ignore = "requires BACKTEST_SNAPSHOT_PATH environment variable"]
fn test_load_snapshots_with_outcome_filter() {
    let path_str = get_snapshot_path().expect("BACKTEST_SNAPSHOT_PATH must be set");
    let path = Path::new(&path_str);

    // First load all snapshots to find available outcomes
    let loader_all = ParquetLoader::new(None);
    let all_snapshots = loader_all
        .load_snapshots(path)
        .expect("Failed to load all snapshots");

    // Get a unique outcome from the data
    let sample_outcome = all_snapshots
        .first()
        .and_then(|s| s.market_metadata.as_ref())
        .and_then(|m| m.outcome.clone())
        .expect("Should have outcome in market metadata");

    println!("Filtering by outcome: {}", sample_outcome);

    // Load with filter
    let loader_filtered = ParquetLoader::new(Some(sample_outcome.clone()));
    let filtered_snapshots = loader_filtered
        .load_snapshots(path)
        .expect("Failed to load filtered snapshots");

    // Verify all filtered snapshots have the correct outcome
    for snapshot in &filtered_snapshots {
        if let Some(ref metadata) = snapshot.market_metadata {
            assert_eq!(
                metadata.outcome.as_deref(),
                Some(sample_outcome.as_str()),
                "All snapshots should have filtered outcome"
            );
        }
    }

    // Filtered count should be <= total count
    assert!(
        filtered_snapshots.len() <= all_snapshots.len(),
        "Filtered count ({}) should be <= total count ({})",
        filtered_snapshots.len(),
        all_snapshots.len()
    );

    println!(
        "Filtered {} snapshots from {} total",
        filtered_snapshots.len(),
        all_snapshots.len()
    );
}

#[test]
#[ignore = "requires BACKTEST_TRADE_PATH environment variable"]
fn test_load_real_trades() {
    let path_str = get_trade_path().expect("BACKTEST_TRADE_PATH must be set");
    let path = Path::new(&path_str);

    assert!(path.exists(), "Trade file does not exist: {}", path_str);

    let loader = ParquetLoader::new(None);
    let trades = loader.load_trades(path).expect("Failed to load trades");

    // Verify we loaded some data
    assert!(!trades.is_empty(), "Should load at least one trade");

    // Verify data integrity
    let first = trades.first().unwrap();
    assert!(!first.asset_id.is_empty(), "asset_id should not be empty");
    assert!(!first.market.is_empty(), "market should not be empty");
    assert!(first.timestamp > 0, "timestamp should be positive");
    assert!(first.price > 0.0, "price should be positive");
    assert!(first.size > 0.0, "size should be positive");

    // Verify chronological order
    for window in trades.windows(2) {
        assert!(
            window[1].timestamp >= window[0].timestamp,
            "Trades should be sorted by timestamp"
        );
    }

    println!("Loaded {} trades successfully", trades.len());
    println!(
        "Time range: {} to {}",
        trades.first().unwrap().timestamp,
        trades.last().unwrap().timestamp
    );
}

#[test]
#[ignore = "requires BACKTEST_TRADE_PATH environment variable"]
fn test_load_trades_with_outcome_filter() {
    let path_str = get_trade_path().expect("BACKTEST_TRADE_PATH must be set");
    let path = Path::new(&path_str);

    // First load all trades to find available outcomes
    let loader_all = ParquetLoader::new(None);
    let all_trades = loader_all
        .load_trades(path)
        .expect("Failed to load all trades");

    // Get a unique outcome from the data
    let sample_outcome = all_trades
        .first()
        .and_then(|t| t.market_metadata.as_ref())
        .and_then(|m| m.outcome.clone())
        .expect("Should have outcome in market metadata");

    println!("Filtering by outcome: {}", sample_outcome);

    // Load with filter
    let loader_filtered = ParquetLoader::new(Some(sample_outcome.clone()));
    let filtered_trades = loader_filtered
        .load_trades(path)
        .expect("Failed to load filtered trades");

    // Verify all filtered trades have the correct outcome
    for trade in &filtered_trades {
        if let Some(ref metadata) = trade.market_metadata {
            assert_eq!(
                metadata.outcome.as_deref(),
                Some(sample_outcome.as_str()),
                "All trades should have filtered outcome"
            );
        }
    }

    // Filtered count should be <= total count
    assert!(
        filtered_trades.len() <= all_trades.len(),
        "Filtered count ({}) should be <= total count ({})",
        filtered_trades.len(),
        all_trades.len()
    );

    println!(
        "Filtered {} trades from {} total",
        filtered_trades.len(),
        all_trades.len()
    );
}

#[test]
#[ignore = "requires BACKTEST_SNAPSHOT_PATH and BACKTEST_TRADE_PATH environment variables"]
fn test_load_snapshots_and_trades_consistency() {
    let snapshot_path_str = get_snapshot_path().expect("BACKTEST_SNAPSHOT_PATH must be set");
    let trade_path_str = get_trade_path().expect("BACKTEST_TRADE_PATH must be set");

    let snapshot_path = Path::new(&snapshot_path_str);
    let trade_path = Path::new(&trade_path_str);

    let loader = ParquetLoader::new(None);
    let snapshots = loader
        .load_snapshots(snapshot_path)
        .expect("Failed to load snapshots");
    let trades = loader
        .load_trades(trade_path)
        .expect("Failed to load trades");

    // Get time ranges
    let snapshot_start = snapshots.first().map(|s| s.timestamp).unwrap_or(0);
    let snapshot_end = snapshots.last().map(|s| s.timestamp).unwrap_or(0);
    let trade_start = trades.first().map(|t| t.timestamp).unwrap_or(0);
    let trade_end = trades.last().map(|t| t.timestamp).unwrap_or(0);

    println!("Snapshot time range: {} - {}", snapshot_start, snapshot_end);
    println!("Trade time range: {} - {}", trade_start, trade_end);

    // Collect unique asset IDs from both
    let snapshot_assets: std::collections::HashSet<_> =
        snapshots.iter().map(|s| s.asset_id.clone()).collect();
    let trade_assets: std::collections::HashSet<_> =
        trades.iter().map(|t| t.asset_id.clone()).collect();

    println!("Unique assets in snapshots: {}", snapshot_assets.len());
    println!("Unique assets in trades: {}", trade_assets.len());

    // Check for overlap
    let common_assets: std::collections::HashSet<_> = snapshot_assets
        .intersection(&trade_assets)
        .cloned()
        .collect();
    println!("Common assets: {}", common_assets.len());

    // Verify timestamps are in valid range (milliseconds since epoch, after 2020)
    let min_valid_ts = 1577836800000_i64; // 2020-01-01 00:00:00 UTC
    let max_valid_ts = 2000000000000_i64; // ~2033

    for snapshot in &snapshots {
        assert!(
            snapshot.timestamp >= min_valid_ts && snapshot.timestamp <= max_valid_ts,
            "Snapshot timestamp {} out of valid range",
            snapshot.timestamp
        );
    }

    for trade in &trades {
        assert!(
            trade.timestamp >= min_valid_ts && trade.timestamp <= max_valid_ts,
            "Trade timestamp {} out of valid range",
            trade.timestamp
        );
    }
}

#[test]
#[ignore = "requires BACKTEST_SNAPSHOT_PATH environment variable"]
fn test_snapshot_orderbook_validity() {
    let path_str = get_snapshot_path().expect("BACKTEST_SNAPSHOT_PATH must be set");
    let path = Path::new(&path_str);

    let loader = ParquetLoader::new(None);
    let snapshots = loader
        .load_snapshots(path)
        .expect("Failed to load snapshots");

    let mut bid_ask_count = 0;
    let mut empty_orderbook_count = 0;
    let mut bids_sorted_count = 0;
    let mut asks_sorted_count = 0;
    let mut total_with_bids = 0;
    let mut total_with_asks = 0;

    for snapshot in &snapshots {
        if snapshot.bids.is_empty() && snapshot.asks.is_empty() {
            empty_orderbook_count += 1;
            continue;
        }

        // Check if bid prices are in descending order (best bid first)
        if !snapshot.bids.is_empty() {
            total_with_bids += 1;
            let bids_sorted = snapshot.bids.windows(2).all(|w| w[0].price >= w[1].price);
            if bids_sorted {
                bids_sorted_count += 1;
            }
        }

        // Check if ask prices are in ascending order (best ask first)
        if !snapshot.asks.is_empty() {
            total_with_asks += 1;
            let asks_sorted = snapshot.asks.windows(2).all(|w| w[0].price <= w[1].price);
            if asks_sorted {
                asks_sorted_count += 1;
            }
        }

        // Verify bid/ask spread makes sense (best bid < best ask)
        if let (Some(best_bid), Some(best_ask)) = (snapshot.bids.first(), snapshot.asks.first()) {
            if best_bid.price < best_ask.price {
                bid_ask_count += 1;
            }
        }

        // Verify prices and sizes are non-negative
        for bid in &snapshot.bids {
            assert!(bid.price >= 0.0, "Bid price should be non-negative");
            assert!(bid.size >= 0.0, "Bid size should be non-negative");
        }

        for ask in &snapshot.asks {
            assert!(ask.price >= 0.0, "Ask price should be non-negative");
            assert!(ask.size >= 0.0, "Ask size should be non-negative");
        }
    }

    let non_empty_count = snapshots.len() - empty_orderbook_count;

    println!(
        "Valid bid-ask spreads: {} / {}",
        bid_ask_count, non_empty_count
    );
    println!(
        "Empty orderbooks: {} / {}",
        empty_orderbook_count,
        snapshots.len()
    );
    println!(
        "Bids sorted (descending): {} / {}",
        bids_sorted_count, total_with_bids
    );
    println!(
        "Asks sorted (ascending): {} / {}",
        asks_sorted_count, total_with_asks
    );

    // Verify we have meaningful data
    assert!(
        non_empty_count > 0,
        "Should have at least one non-empty orderbook"
    );
}

/// Performance test for snapshot loading.
///
/// This test measures the time taken to load snapshots with and without ticker filtering,
/// and reports the file metadata to understand the data volume.
///
/// Run with:
/// ```bash
/// BACKTEST_SNAPSHOT_PATH=/path/to/snapshots.parquet \
/// cargo test --package trade_server test_snapshot_loading_performance -- --ignored --nocapture
/// ```
#[test]
#[ignore = "requires BACKTEST_SNAPSHOT_PATH environment variable"]
fn test_snapshot_loading_performance() {
    let path_str = get_snapshot_path().expect("BACKTEST_SNAPSHOT_PATH must be set");
    let path = Path::new(&path_str);

    // Get file metadata
    let file = File::open(path).expect("Failed to open file");
    let file_size = file.metadata().expect("Failed to get metadata").len();
    let builder =
        ParquetRecordBatchReaderBuilder::try_new(file).expect("Failed to create reader builder");
    let parquet_metadata = builder.metadata();

    println!("\n=== Parquet File Metadata ===");
    println!("File path: {}", path_str);
    println!("File size: {:.2} MB", file_size as f64 / 1_000_000.0);
    println!("Row groups: {}", parquet_metadata.num_row_groups());
    println!(
        "Total rows: {}",
        parquet_metadata
            .row_groups()
            .iter()
            .map(|rg| rg.num_rows())
            .sum::<i64>()
    );

    // Print column information
    let schema = builder.schema();
    println!("\nSchema ({} columns):", schema.fields().len());
    for (i, field) in schema.fields().iter().enumerate() {
        println!("  {}: {} ({})", i, field.name(), field.data_type());
    }

    // Get unique tickers to use for filter test
    println!("\n=== Loading all data (no filter) ===");
    let start = Instant::now();
    let loader = ParquetLoader::new(None);
    let all_snapshots = loader
        .load_snapshots(path)
        .expect("Failed to load snapshots");
    let load_all_time = start.elapsed();

    println!(
        "Loaded {} snapshots in {:.2?}",
        all_snapshots.len(),
        load_all_time
    );
    println!(
        "Throughput: {:.2} MB/s",
        file_size as f64 / 1_000_000.0 / load_all_time.as_secs_f64()
    );

    // Get unique tickers
    let unique_tickers: std::collections::HashSet<_> =
        all_snapshots.iter().map(|s| s.market.clone()).collect();
    println!("\nUnique tickers: {}", unique_tickers.len());

    // Test with ticker filter if we have tickers
    if let Some(sample_ticker) = unique_tickers.iter().next() {
        println!("\n=== Loading with ticker filter: {} ===", sample_ticker);

        // Create a glob pattern from the ticker
        let ticker_pattern = format!(
            "{}*",
            sample_ticker
                .split('-')
                .take(3)
                .collect::<Vec<_>>()
                .join("-")
        );
        println!("Using pattern: {}", ticker_pattern);

        let start = Instant::now();
        let filtered_loader = ParquetLoader::with_filters(None, Some(vec![ticker_pattern.clone()]));
        let filtered_snapshots = filtered_loader
            .load_snapshots(path)
            .expect("Failed to load filtered snapshots");
        let load_filtered_time = start.elapsed();

        println!(
            "Loaded {} snapshots in {:.2?}",
            filtered_snapshots.len(),
            load_filtered_time
        );

        // Calculate filtering ratio
        let filter_ratio = filtered_snapshots.len() as f64 / all_snapshots.len() as f64;
        println!(
            "Filter ratio: {:.2}% of data matches filter",
            filter_ratio * 100.0
        );

        // Performance expectation: with column projection, filtered loading should be faster
        // proportional to the data reduction, not just slightly faster due to less parsing
        println!("\n=== Performance Analysis ===");
        println!("Load all time: {:.2?}", load_all_time);
        println!("Load filtered time: {:.2?}", load_filtered_time);
        let speedup = load_all_time.as_secs_f64() / load_filtered_time.as_secs_f64();
        println!("Speedup: {:.2}x", speedup);

        // The key insight: with column projection, even loading all data should be faster
        // because we only read 6/16 columns (37.5% of column data)
        println!("\nExpected optimizations:");
        println!(
            "  - Column projection: read 6 columns instead of {}",
            schema.fields().len()
        );
        println!(
            "  - Expected read reduction: ~{:.0}%",
            (1.0 - 6.0 / schema.fields().len() as f64) * 100.0
        );
    }
}
