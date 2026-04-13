//! Spot price subscriber - subscribes to real-time cryptocurrency spot prices
//! from Polymarket RTDS API and publishes to Redis

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

mod app;
mod binance_ws_client;
mod config;
mod health_monitor;
mod metrics;
mod parser;
mod redis_publisher;
mod ws_client;

use app::App;
use config::Config;

#[derive(Parser)]
#[command(name = "spot-price-sub")]
#[command(about = "Spot price subscriber for Polymarket RTDS API")]
struct Args {
    /// Path to configuration file
    #[arg(long, default_value = "configs/spot-price-sub/config.yaml")]
    config_file: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging with JSON format
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    // Parse command line arguments
    let args = Args::parse();

    tracing::info!(config_file = %args.config_file, "Loading configuration");

    // Load configuration
    let config = Config::from_file(&args.config_file)?;

    tracing::info!(
        wss_endpoint = %config.spot_price.wss_endpoint,
        symbols = ?config.spot_price.symbols,
        redis_url = %config.redis.url,
        "Configuration loaded successfully"
    );

    // Create and run application
    let app = App::new(config)?;
    app.run().await?;

    Ok(())
}
