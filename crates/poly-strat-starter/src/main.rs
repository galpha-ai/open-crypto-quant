mod strategy;

use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::EnvFilter;
use trade_server::backtest::{BacktestConfig, BacktestRunner, CaptureConfig, PositionConfig};

use crate::strategy::CheapBuyerGenerator;

#[derive(Parser, Debug)]
#[command(name = "poly-strat")]
#[command(about = "Starter strategy backtester for Polymarket data")]
struct Args {
    #[arg(long)]
    snapshot_path: PathBuf,

    #[arg(long)]
    update_path: PathBuf,

    #[arg(long)]
    trade_path: PathBuf,

    #[arg(long, default_value = "output/backtest_events.jsonl")]
    output_path: PathBuf,

    #[arg(long)]
    outcome_filter: Option<String>,

    #[arg(long = "ticker-pattern")]
    ticker_patterns: Vec<String>,

    #[arg(long, default_value_t = 0.30)]
    threshold: f64,

    #[arg(long, default_value_t = 100.0)]
    trade_amount: f64,

    #[arg(long, default_value_t = 10)]
    max_positions: u32,

    #[arg(long, default_value_t = 0.15)]
    take_profit: f64,

    #[arg(long, default_value_t = 0.10)]
    stop_loss: f64,

    #[arg(long, default_value_t = 3600)]
    max_hold_secs: u64,

    #[arg(long, default_value_t = 10_000.0)]
    initial_balance: f64,

    #[arg(long)]
    capture_snapshots: bool,

    #[arg(long)]
    capture_updates: bool,

    #[arg(long)]
    capture_trades: bool,

    #[arg(long)]
    capture_spot_prices: bool,
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let args = Args::parse();

    if let Some(parent) = args.output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let position_config = PositionConfig::new(
        args.initial_balance,
        args.max_positions,
        Duration::from_secs(args.max_hold_secs),
        args.trade_amount,
    )
    .with_take_profit(args.take_profit)
    .with_stop_loss(args.stop_loss);

    let capture_config = CaptureConfig {
        market_data_orderbook_snapshots: args.capture_snapshots,
        market_data_orderbook_updates: args.capture_updates,
        market_data_polymarket_trades: args.capture_trades,
        market_data_spot_prices: args.capture_spot_prices,
        ..CaptureConfig::default()
    };

    let mut config = BacktestConfig::new(
        args.snapshot_path,
        args.update_path,
        args.trade_path,
        args.output_path.clone(),
    )
    .with_position_config(position_config)
    .with_capture(capture_config)
    .with_timer_interval(Duration::from_secs(1));

    if let Some(outcome) = args.outcome_filter {
        config = config.with_outcome_filter(outcome);
    }

    if !args.ticker_patterns.is_empty() {
        config = config.with_ticker_patterns(args.ticker_patterns);
    }

    config
        .validate()
        .context("backtest config validation failed")?;

    let generator = CheapBuyerGenerator::new(args.threshold, args.trade_amount);
    let result = BacktestRunner::run(config, Box::new(generator)).await?;

    println!("Backtest completed:");
    println!("  output: {}", result.output_path.display());
    println!("  snapshots: {}", result.metrics.total_snapshots);
    println!("  trades: {}", result.metrics.total_trades);
    println!("  signals: {}", result.metrics.total_signals);
    println!("  orders: {}", result.metrics.total_orders_placed);
    println!("  fills: {}", result.metrics.total_fills);
    println!("  final_pnl: {:.4}", result.metrics.final_pnl);
    println!("  duration_ms: {}", result.metrics.duration_ms);

    Ok(())
}
