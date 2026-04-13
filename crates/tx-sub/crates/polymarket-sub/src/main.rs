//! Polymarket Subscriber
//!
//! Real-time subscription service for Polymarket prediction market events.
//! Connects to Polymarket's WebSocket API, processes trade events, and publishes
//! structured data to Redis.

mod api_client;
mod app;
mod config;
mod health_monitor;
mod market_discovery;
mod market_metadata_cache;
mod market_metadata_loader;
mod metrics;
mod parser;
mod redis_orderbook_publisher;
mod redis_publisher;
mod subscription_manager;
mod types;
mod ws_client;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "polymarket-sub")]
#[command(about = "Polymarket WebSocket subscriber for trade events", long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE")]
    config_file: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging with JSON output
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer().json())
        .init();

    let args = Args::parse();

    tracing::info!(
        config_file = %args.config_file.display(),
        "Polymarket subscriber starting"
    );

    // Load configuration
    let config = config::Config::from_file(&args.config_file)?;

    // Create and run app
    let app = app::App::new(config)?;
    app.run().await?;

    Ok(())
}
