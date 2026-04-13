use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result};
use tempfile::TempDir;
use trade_server::{
    backtest::{BacktestConfig, BacktestRunner, CaptureConfig, ParquetLoader, PositionConfig},
    signal::NoopSignalGenerator,
};

struct PerfMetrics {
    data_load_duration_ms: u128,
    event_loop_duration_ms: u128,
    total_duration_ms: u128,
    snapshot_count: usize,
    update_count: usize,
    trade_count: usize,
    events_processed: usize,
    peak_memory_mb: Option<f64>,
    unique_tickers: usize,
}

fn expand_path(raw: &str) -> PathBuf {
    PathBuf::from(shellexpand::tilde(raw).as_ref())
}

fn env_var(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn parse_ticker_patterns(raw: Option<String>) -> Option<Vec<String>> {
    raw.map(|value| {
        value
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    })
    .filter(|patterns| !patterns.is_empty())
}

fn derive_trade_path(snapshot_path: &Path) -> PathBuf {
    let filename = snapshot_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("snapshots.parquet");
    let derived_name = filename.replace("snapshots", "trades");
    snapshot_path
        .parent()
        .map(|parent| parent.join(derived_name))
        .unwrap_or_else(|| PathBuf::from("trades.parquet"))
}

fn get_memory_usage_mb() -> Option<f64> {
    let statm = fs::read_to_string("/proc/self/statm").ok()?;
    let mut parts = statm.split_whitespace();
    let _size = parts.next()?;
    let resident_pages: u64 = parts.next()?.parse().ok()?;
    let page_size_bytes = 4096u64;
    Some(resident_pages as f64 * page_size_bytes as f64 / (1024.0 * 1024.0))
}

fn load_config_from_env() -> Result<(BacktestConfig, TempDir)> {
    let snapshot_path = env_var("BACKTEST_PERF_SNAPSHOT_PATH")
        .map(|v| expand_path(&v))
        .context("BACKTEST_PERF_SNAPSHOT_PATH is required")?;
    let update_path = env_var("BACKTEST_PERF_UPDATE_PATH")
        .map(|v| expand_path(&v))
        .context("BACKTEST_PERF_UPDATE_PATH is required")?;

    let trade_path = env_var("BACKTEST_PERF_TRADE_PATH")
        .map(|v| expand_path(&v))
        .unwrap_or_else(|| derive_trade_path(&snapshot_path));

    let spot_path = env_var("BACKTEST_PERF_SPOT_PATH").map(|v| expand_path(&v));

    let ticker_patterns = parse_ticker_patterns(env_var("BACKTEST_PERF_TICKER_PATTERNS"));
    let outcome_filter = env_var("BACKTEST_PERF_OUTCOME_FILTER");

    let output_dir = TempDir::new().context("create temp output dir")?;
    let output_path = output_dir.path().join("backtest_perf_events.jsonl");

    let mut config = BacktestConfig::new(snapshot_path, update_path, trade_path, output_path)
        .with_position_config(PositionConfig::default())
        .with_capture(CaptureConfig {
            token_events: false,
            market_data_orderbook_snapshots: false,
            market_data_orderbook_updates: false,
            market_data_polymarket_trades: false,
            market_data_spot_prices: false,
            timer_events: false,
            signal_events: false,
            position_events: false,
            execution_events: false,
            limit_order_events: false,
            redemption_events: false,
        });

    if let Some(patterns) = ticker_patterns {
        config = config.with_ticker_patterns(patterns);
    }

    if let Some(filter) = outcome_filter {
        config = config.with_outcome_filter(filter);
    }

    if let Some(path) = &spot_path {
        config = config.with_spot_event_path(path.clone());
    }

    config.validate()?;

    Ok((config, output_dir))
}

fn print_metrics(metrics: &PerfMetrics) {
    println!("Backtest performance metrics:");
    println!("  data_load_duration_ms: {}", metrics.data_load_duration_ms);
    println!(
        "  event_loop_duration_ms: {}",
        metrics.event_loop_duration_ms
    );
    println!("  total_duration_ms: {}", metrics.total_duration_ms);
    println!("  snapshot_count: {}", metrics.snapshot_count);
    println!("  update_count: {}", metrics.update_count);
    println!("  trade_count: {}", metrics.trade_count);
    println!("  events_processed: {}", metrics.events_processed);
    println!("  unique_tickers: {}", metrics.unique_tickers);
    match metrics.peak_memory_mb {
        Some(value) => println!("  peak_memory_mb: {:.2}", value),
        None => println!("  peak_memory_mb: unavailable"),
    }
}

#[tokio::test]
#[ignore = "Performance test requires large parquet files and env vars"]
async fn test_backtest_performance() -> Result<()> {
    let (config, _temp_dir) = load_config_from_env()?;

    let mem_before = get_memory_usage_mb();

    let loader = ParquetLoader::with_filters_and_time_range(
        config.outcome_filter.clone(),
        config.ticker_patterns.clone(),
        config.ticker_time_range_filter,
        config.ticker_time_range_buffer,
    );

    let data_load_start = Instant::now();
    let snapshots = loader
        .load_snapshots(&config.snapshot_path)
        .context("load snapshots")?;
    let updates = loader
        .load_updates(&config.update_path)
        .context("load updates")?;
    let trades = loader
        .load_trades(&config.trade_path)
        .context("load trades")?;
    let _spot_events = if let Some(ref spot_path) = config.spot_event_path {
        Some(
            loader
                .load_spot_events(spot_path)
                .context("load spot events")?,
        )
    } else {
        None
    };
    let data_load_duration_ms = data_load_start.elapsed().as_millis();
    let mem_after_load = get_memory_usage_mb();

    let mut tickers = BTreeSet::new();
    tickers.extend(snapshots.iter().map(|s| s.market.as_str().to_string()));
    tickers.extend(updates.iter().map(|u| u.market.as_str().to_string()));
    tickers.extend(trades.iter().map(|t| t.market.as_str().to_string()));

    let snapshot_count = snapshots.len();
    let update_count = updates.len();
    let trade_count = trades.len();
    let unique_tickers = tickers.len();

    drop(snapshots);
    drop(updates);
    drop(trades);

    let event_loop_start = Instant::now();
    let result = BacktestRunner::run(config, Box::new(NoopSignalGenerator)).await?;
    let event_loop_duration_ms = event_loop_start.elapsed().as_millis();
    let mem_after_run = get_memory_usage_mb();

    let peak_memory_mb = [mem_before, mem_after_load, mem_after_run]
        .into_iter()
        .flatten()
        .fold(None::<f64>, |acc, value| match acc {
            Some(current) => Some(current.max(value)),
            None => Some(value),
        });

    let events_processed = snapshot_count + update_count + trade_count;
    let total_duration_ms = data_load_duration_ms + event_loop_duration_ms;

    let metrics = PerfMetrics {
        data_load_duration_ms,
        event_loop_duration_ms,
        total_duration_ms,
        snapshot_count,
        update_count,
        trade_count,
        events_processed,
        peak_memory_mb,
        unique_tickers,
    };

    print_metrics(&metrics);
    println!(
        "Backtest result metrics: snapshots={}, trades={}, signals={}, fills={}, cancels={}, duration_ms={}",
        result.metrics.total_snapshots,
        result.metrics.total_trades,
        result.metrics.total_signals,
        result.metrics.total_fills,
        result.metrics.total_cancels,
        result.metrics.duration_ms
    );

    Ok(())
}
