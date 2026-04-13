//! Integration test for spot-price-sub WebSocket subscription
//!
//! This test connects to the Polymarket RTDS WebSocket API, subscribes to
//! crypto_prices updates, and prints received messages for 30 seconds.
//!
//! Usage:
//!   cargo run -p spot-price-sub --bin spot-price-integration-test

use anyhow::Result;
use popeyes_trading_types::SpotPriceUpdate;
use prometheus::Registry;
use spot_price_sub::{Metrics, RtdsWebSocketClient, SpotPriceParser};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Test configuration - hardcoded for integration test
const WSS_ENDPOINT: &str = "wss://ws-live-data.polymarket.com";
const SYMBOLS: &[&str] = &["BTCUSDT", "ETHUSDT", "SOLUSDT", "XRPUSDT"];
const TEST_DURATION_SECS: u64 = 30;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging with environment filter
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,spot_price_sub=debug"));

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

    info!("Starting spot-price-sub integration test");
    info!(
        endpoint = WSS_ENDPOINT,
        symbols = ?SYMBOLS,
        duration_secs = TEST_DURATION_SECS,
        "Test configuration"
    );
    info!("Test will run for {} seconds and then exit", TEST_DURATION_SECS);

    // Create metrics registry (required by components but not used in test)
    let registry = Registry::new();
    let metrics = Arc::new(Metrics::new(&registry)?);

    // Create broadcast channels
    let (ws_message_tx, ws_message_rx) = broadcast::channel(1000);
    let (spot_price_tx, mut spot_price_rx) = broadcast::channel(1000);

    // Create cancellation token
    let cancellation_token = CancellationToken::new();

    // Create WebSocket client
    let ws_client = RtdsWebSocketClient::new(
        WSS_ENDPOINT.to_string(),
        SYMBOLS.iter().map(|s| s.to_string()).collect(),
        ws_message_tx,
        Arc::clone(&metrics),
    );

    // Create parser
    let parser = SpotPriceParser::new(ws_message_rx, spot_price_tx, Arc::clone(&metrics));

    // Counter for received updates
    let update_count = Arc::new(AtomicU64::new(0));
    let update_count_clone = Arc::clone(&update_count);

    // Spawn task to print received spot price updates
    let print_task = tokio::spawn(async move {
        info!("Starting spot price update printer");
        loop {
            match spot_price_rx.recv().await {
                Ok(update) => {
                    let count = update_count_clone.fetch_add(1, Ordering::Relaxed) + 1;
                    print_spot_price_update(&update, count);
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    info!(lagged = n, "Printer lagged behind, some messages skipped");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("Spot price channel closed, printer shutting down");
                    break;
                }
            }
        }
    });

    // Run the test with timeout
    let cancellation_token_clone = cancellation_token.clone();
    let test_result = timeout(Duration::from_secs(TEST_DURATION_SECS), async {
        tokio::select! {
            result = ws_client.run(cancellation_token.clone()) => {
                if let Err(e) = result {
                    error!(error = %e, "WebSocket client error");
                }
            }
            result = parser.start() => {
                if let Err(e) = result {
                    error!(error = %e, "Parser error");
                }
            }
        }
    })
    .await;

    // Cancel all tasks
    cancellation_token_clone.cancel();
    print_task.abort();

    // Report results
    let total_updates = update_count.load(Ordering::Relaxed);
    match test_result {
        Ok(_) => {
            info!(
                total_updates = total_updates,
                "Integration test completed (component exited before timeout)"
            );
        }
        Err(_) => {
            info!(
                total_updates = total_updates,
                duration_secs = TEST_DURATION_SECS,
                "Integration test completed successfully ({} second timeout reached)",
                TEST_DURATION_SECS
            );
        }
    }

    if total_updates == 0 {
        error!("No spot price updates received during test - this may indicate a problem");
    } else {
        info!(
            updates_per_second = total_updates as f64 / TEST_DURATION_SECS as f64,
            "Test statistics"
        );
    }

    info!("Integration test finished");
    Ok(())
}

/// Print a spot price update in a human-readable format
fn print_spot_price_update(update: &SpotPriceUpdate, count: u64) {
    info!(
        count = count,
        symbol = %update.symbol,
        price = update.price,
        timestamp = %update.timestamp,
        source = %update.source,
        "Received spot price update"
    );
}
