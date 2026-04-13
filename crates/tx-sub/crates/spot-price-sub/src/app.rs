//! Application orchestration for spot price subscriber

use anyhow::Result;
use prometheus::Registry;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::binance_ws_client::BinanceWebSocketClient;
use crate::config::Config;
use crate::health_monitor::HealthMonitor;
use crate::metrics::Metrics;
use crate::parser::SpotPriceParser;
use crate::redis_publisher::RedisSpotPricePublisher;
use crate::ws_client::RtdsWebSocketClient;

/// Main application
pub struct App {
    config: Config,
    metrics: Arc<Metrics>,
}

impl App {
    /// Create a new application
    pub fn new(config: Config) -> Result<Self> {
        // Create metrics registry
        let registry = Registry::new();
        let metrics = Arc::new(Metrics::new(&registry)?);

        // Start metrics server if configured
        if let Some(port) = config.metrics.port {
            common::metrics::start_metrics_server(registry, Some(port));
            tracing::info!(port = port, "Metrics server started");
        }

        Ok(Self { config, metrics })
    }

    /// Run the application
    pub async fn run(self) -> Result<()> {
        tracing::info!("Spot price subscriber starting");

        // Create cancellation token for graceful shutdown
        let cancellation_token = CancellationToken::new();

        // Create broadcast channels
        let (ws_message_tx, ws_message_rx) = broadcast::channel(1000);
        let (spot_price_tx, spot_price_rx) = broadcast::channel(1000);

        // Select WebSocket client based on data source
        let data_source = self.config.spot_price.data_source.as_str();
        tracing::info!(data_source = data_source, "Initializing WebSocket client");

        // Create parser
        let parser = SpotPriceParser::new(
            ws_message_rx,
            spot_price_tx.clone(),
            Arc::clone(&self.metrics),
        );

        // Create Redis publisher
        let redis_publisher = RedisSpotPricePublisher::new(
            spot_price_rx.resubscribe(),
            &self.config.redis,
            Arc::clone(&self.metrics),
        )
        .await?;

        // Create health monitor if enabled
        let health_monitor = if let Some(ref health_config) = self.config.spot_price.health_monitoring
        {
            if health_config.enabled {
                Some(HealthMonitor::new(
                    spot_price_rx.resubscribe(),
                    health_config.clone(),
                    Arc::clone(&self.metrics),
                ))
            } else {
                None
            }
        } else {
            None
        };

        tracing::info!(
            symbols = ?self.config.spot_price.symbols,
            health_monitoring = health_monitor.is_some(),
            data_source = data_source,
            "All components initialized, starting"
        );

        // Run all components concurrently
        let cancellation_token_clone = cancellation_token.clone();

        // Run appropriate WebSocket client based on data source
        match data_source {
            "binance" => {
                let binance_client = BinanceWebSocketClient::new(
                    self.config.spot_price.wss_endpoint.clone(),
                    self.config.spot_price.symbols.clone(),
                    ws_message_tx,
                    Arc::clone(&self.metrics),
                );

                tokio::select! {
                    result = binance_client.run(cancellation_token.clone()) => {
                        tracing::info!("Binance WebSocket client exited");
                        if let Err(e) = result {
                            tracing::error!(error = %e, "Binance WebSocket client error");
                        }
                    }
                    result = parser.start() => {
                        tracing::info!("Parser exited");
                        if let Err(e) = result {
                            tracing::error!(error = %e, "Parser error");
                        }
                    }
                    result = redis_publisher.start() => {
                        tracing::info!("Redis publisher exited");
                        if let Err(e) = result {
                            tracing::error!(error = %e, "Redis publisher error");
                        }
                    }
                    result = async {
                        if let Some(monitor) = health_monitor {
                            monitor.start().await
                        } else {
                            std::future::pending::<Result<()>>().await
                        }
                    } => {
                        tracing::info!("Health monitor exited");
                        if let Err(e) = result {
                            tracing::error!(error = %e, "Health monitor error");
                        }
                    }
                    _ = shutdown_signal() => {
                        tracing::info!("Shutdown signal received");
                        cancellation_token_clone.cancel();
                    }
                }
            }
            "rtds" | _ => {
                let rtds_client = RtdsWebSocketClient::new(
                    self.config.spot_price.wss_endpoint.clone(),
                    self.config.spot_price.symbols.clone(),
                    ws_message_tx,
                    Arc::clone(&self.metrics),
                );

                tokio::select! {
                    result = rtds_client.run(cancellation_token.clone()) => {
                        tracing::info!("RTDS WebSocket client exited");
                        if let Err(e) = result {
                            tracing::error!(error = %e, "RTDS WebSocket client error");
                        }
                    }
                    result = parser.start() => {
                        tracing::info!("Parser exited");
                        if let Err(e) = result {
                            tracing::error!(error = %e, "Parser error");
                        }
                    }
                    result = redis_publisher.start() => {
                        tracing::info!("Redis publisher exited");
                        if let Err(e) = result {
                            tracing::error!(error = %e, "Redis publisher error");
                        }
                    }
                    result = async {
                        if let Some(monitor) = health_monitor {
                            monitor.start().await
                        } else {
                            std::future::pending::<Result<()>>().await
                        }
                    } => {
                        tracing::info!("Health monitor exited");
                        if let Err(e) = result {
                            tracing::error!(error = %e, "Health monitor error");
                        }
                    }
                    _ = shutdown_signal() => {
                        tracing::info!("Shutdown signal received");
                        cancellation_token_clone.cancel();
                    }
                }
            }
        }

        tracing::info!("Spot price subscriber shut down");
        Ok(())
    }
}

/// Wait for shutdown signal (Ctrl+C)
async fn shutdown_signal() {
    signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
}
