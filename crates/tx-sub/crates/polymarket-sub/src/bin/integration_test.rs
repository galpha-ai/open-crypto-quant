//! Integration test for Polymarket subscriber
//!
//! This test subscribes to active Solana binary prediction markets,
//! prints received trade events, and exits after 10 seconds.

use anyhow::Result;
use polymarket_sub::config::Config;
use polymarket_sub::App;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{error, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize rustls crypto provider (required for TLS connections)
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("integration_test: failed to install rustls crypto provider");

    // Initialize logging with environment filter
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,polymarket_sub=debug"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();

    info!("Starting Polymarket integration test");
    info!("Test will run for 30 seconds and then exit");

    // Load configuration
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "configs/polymarket-sub/config.integration_test.yaml".to_string());

    info!("Loading configuration from: {}", config_path);

    let config = Config::from_file(&config_path).map_err(|e| {
        error!("Failed to load config file: {}", e);
        e
    })?;

    info!("Configuration loaded successfully");

    // Create the app
    let app = App::new(config)?;

    info!("Starting Polymarket subscriber - will run for 30 seconds");

    // Run the app with a 60 second timeout
    match timeout(Duration::from_secs(60), app.run()).await {
        Ok(result) => {
            match result {
                Ok(_) => {
                    info!("Integration test completed successfully (app exited before timeout)");
                }
                Err(e) => {
                    error!("Integration test failed: {}", e);
                    return Err(e);
                }
            }
        }
        Err(_) => {
            info!("Integration test completed successfully (30 second timeout reached)");
        }
    }

    info!("Integration test finished");
    Ok(())
}
