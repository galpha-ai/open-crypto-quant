//! Application orchestration for Polymarket subscriber
//!
//! This module coordinates the WebSocket client, event parser, Redis publishers,
//! market discovery, and subscription management.

use anyhow::Result;
use popeyes_trading_types::PolymarketTradeEvent;
use prometheus::Registry;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::api_client::PolymarketApiClient;
use crate::config::Config;
use crate::health_monitor::HealthMonitor;
use crate::market_discovery::MarketDiscoveryService;
use crate::market_metadata_cache::MarketMetadataCache;
use crate::market_metadata_loader::MarketMetadataLoader;
use crate::metrics::Metrics;
use crate::parser::EventParser;
use crate::redis_orderbook_publisher::RedisOrderbookPublisher;
use crate::redis_publisher::RedisTradePublisher;
use crate::subscription_manager::SubscriptionManager;
use crate::types::{ParsedBookEvent, ParsedPriceChangeEvent, SubscriptionState};
use crate::ws_client::{ObservedMessage, PolymarketWebSocketClient};

/// Main application struct that orchestrates all components
pub struct App {
    config: Config,
    metrics: Arc<Metrics>,
}

impl App {
    /// Create a new App instance from configuration
    pub fn new(config: Config) -> Result<Self> {
        // Initialize metrics registry
        let registry = Registry::new();
        let metrics = Arc::new(Metrics::new(&registry)?);

        // Start metrics server
        let port = config.metrics.port;
        info!(?port, "Starting metrics server");
        common::metrics::start_metrics_server(registry, port);

        Ok(Self { config, metrics })
    }

    /// Run the application
    pub async fn run(self) -> Result<()> {
        info!("Polymarket subscriber starting");

        // Create main cancellation token for graceful shutdown
        let cancellation_token = CancellationToken::new();

        // Set up Ctrl+C handler
        let cancellation_token_clone = cancellation_token.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            info!("Received shutdown signal, initiating graceful shutdown");
            cancellation_token_clone.cancel();
        });

        // Check if market discovery is enabled
        if let Some(ref discovery_config) = self.config.polymarket.market_discovery {
            if discovery_config.enabled {
                info!("Starting with automatic market discovery");
                self.run_with_discovery(cancellation_token).await
            } else {
                info!("Market discovery disabled, using manual asset list");
                self.run_manual_mode(cancellation_token).await
            }
        } else {
            info!("No market discovery config, using manual asset list");
            self.run_manual_mode(cancellation_token).await
        }
    }

    /// Run in manual mode with static asset list
    async fn run_manual_mode(self, cancellation_token: CancellationToken) -> Result<()> {
        // Create market metadata cache
        let market_metadata_cache = Arc::new(MarketMetadataCache::new());

        // Load metadata for configured assets if discovery config exists
        if let Some(ref discovery_config) = self.config.polymarket.market_discovery {
            info!("Loading market metadata for configured assets on startup");

            // Create API client
            let api_client = Arc::new(PolymarketApiClient::new(
                discovery_config.api_base_url.clone(),
                discovery_config.api_timeout_secs,
                discovery_config.api_retry_attempts,
                discovery_config.api_retry_backoff_ms,
            )?);

            // Create metadata loader
            let loader = MarketMetadataLoader::new(
                Arc::clone(&api_client),
                Arc::clone(&market_metadata_cache),
            );

            // Load metadata for configured assets
            match loader
                .load_metadata_for_assets(&self.config.polymarket.assets, discovery_config.tag_id)
                .await
            {
                Ok(count) => {
                    info!(
                        markets_cached = count,
                        "Successfully loaded market metadata for manual mode"
                    );
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        "Failed to load market metadata, trades will be published without metadata"
                    );
                }
            }
        } else {
            info!("No market discovery config found, skipping metadata loading");
        }

        // Create broadcast channels
        let (message_tx, _) = broadcast::channel::<ObservedMessage>(1000);
        let (trade_tx, _) = broadcast::channel::<PolymarketTradeEvent>(1000);
        let (price_change_tx, _) = broadcast::channel::<ParsedPriceChangeEvent>(1000);
        let (book_tx, _) = broadcast::channel::<ParsedBookEvent>(1000);

        // Create WebSocket client
        let ws_client = PolymarketWebSocketClient::new(
            self.config.polymarket.wss_endpoint.clone(),
            self.config.polymarket.assets.clone(),
            message_tx.clone(),
            Arc::clone(&self.metrics),
        );

        // Create subscription state for manual mode
        let subscription_state = Arc::new(Mutex::new(SubscriptionState::new()));
        {
            let mut state = subscription_state.lock().await;
            let mut assets = std::collections::HashSet::new();
            for asset_id in &self.config.polymarket.assets {
                assets.insert(asset_id.clone());
            }
            state.update(assets, self.config.polymarket.assets.len());
        }

        // Update metrics with initial subscription count
        self.metrics
            .assets_subscribed
            .set(self.config.polymarket.assets.len() as f64);

        // Create parser with metadata cache
        let parser = EventParser::new(
            message_tx.subscribe(),
            trade_tx.clone(),
            price_change_tx.clone(),
            book_tx.clone(),
            Arc::clone(&self.metrics),
            Arc::clone(&market_metadata_cache),
        );

        // Create Redis trade publisher
        let redis_trade_publisher = RedisTradePublisher::new(
            &self.config,
            trade_tx.subscribe(),
            Arc::clone(&self.metrics),
        )
        .await?;

        // Create Redis orderbook publisher (if configured)
        let redis_orderbook_publisher = if self.config.redis.orderbook_queues.is_some()
            || self.config.redis.orderbook_streams.is_some()
            || self.config.redis.orderbook_pubsub.is_some()
        {
            Some(
                RedisOrderbookPublisher::new(
                    &self.config,
                    price_change_tx.subscribe(),
                    book_tx.subscribe(),
                    Arc::clone(&self.metrics),
                    Arc::clone(&market_metadata_cache),
                )
                .await?,
            )
        } else {
            None
        };

        // Create health monitor if enabled
        let health_monitor_handle = if let Some(ref health_config) = self.config.polymarket.health_monitoring {
            if health_config.enabled {
                info!("Health monitoring enabled");
                let health_monitor = HealthMonitor::new(
                    health_config.clone(),
                    Arc::clone(&subscription_state),
                    Arc::clone(&self.metrics),
                );
                let health_token = cancellation_token.child_token();
                let trade_rx = trade_tx.subscribe();
                let price_change_rx = price_change_tx.subscribe();
                Some(tokio::spawn(async move {
                    health_monitor.run(trade_rx, price_change_rx, health_token).await
                }))
            } else {
                info!("Health monitoring disabled");
                None
            }
        } else {
            info!("Health monitoring not configured");
            None
        };

        // Run all components concurrently
        info!("Starting WebSocket client, parser, and Redis publishers");

        let ws_token = cancellation_token.child_token();
        let parser_token = cancellation_token.child_token();
        let trade_publisher_token = cancellation_token.child_token();
        let orderbook_publisher_token = cancellation_token.child_token();

        // Spawn orderbook publisher if configured
        let orderbook_publisher_handle: Option<JoinHandle<Result<()>>> = if let Some(orderbook_publisher) = redis_orderbook_publisher {
            let token = orderbook_publisher_token.clone();
            Some(tokio::spawn(async move {
                orderbook_publisher.run(token).await
            }))
        } else {
            None
        };

        let result = tokio::select! {
            result = ws_client.run(ws_token) => {
                error!(?result, "WebSocket client exited");
                cancellation_token.cancel();
                result
            }
            result = parser.run(parser_token) => {
                error!(?result, "Parser exited");
                cancellation_token.cancel();
                result
            }
            result = redis_trade_publisher.run(trade_publisher_token) => {
                error!(?result, "Redis trade publisher exited");
                cancellation_token.cancel();
                result
            }
            _ = cancellation_token.cancelled() => {
                info!("Manual mode shutdown complete");
                Ok(())
            }
        };

        // Cleanup orderbook publisher if it was started
        if let Some(handle) = orderbook_publisher_handle {
            handle.abort();
        }

        // Cleanup health monitor if it was started
        if let Some(handle) = health_monitor_handle {
            handle.abort();
        }

        result
    }

    /// Run with automatic market discovery and subscription management
    async fn run_with_discovery(self, cancellation_token: CancellationToken) -> Result<()> {
        let discovery_config = self
            .config
            .polymarket
            .market_discovery
            .as_ref()
            .expect("Market discovery config should be present");

        // Create market metadata cache
        let market_metadata_cache = Arc::new(MarketMetadataCache::new());

        // Create market discovery service
        let (discovery_service, discovery_rx) =
            MarketDiscoveryService::new(
                discovery_config.clone(),
                Arc::clone(&self.metrics),
                Arc::clone(&market_metadata_cache),
            )?;

        // Create subscription manager
        let (subscription_manager, mut subscription_rx) =
            SubscriptionManager::new(discovery_config.clone(), Arc::clone(&self.metrics));

        // Get reference to subscription state for health monitoring (before manager is moved)
        let subscription_state = subscription_manager.state_ref();

        // Start market discovery service
        let discovery_token = cancellation_token.child_token();
        let discovery_handle = tokio::spawn(async move {
            discovery_service.run(discovery_token).await
        });

        // Start subscription manager
        let manager_token = cancellation_token.child_token();
        let manager_handle = tokio::spawn(async move {
            subscription_manager.run(discovery_rx, manager_token).await
        });

        info!("Market discovery and subscription manager started");

        // Wait for initial subscription update
        info!("Waiting for initial market discovery...");
        let initial_subscription = tokio::select! {
            _ = cancellation_token.cancelled() => {
                info!("Shutdown before initial discovery completed");
                return Ok(());
            }
            result = subscription_rx.recv() => {
                match result {
                    Ok(update) => {
                        info!(
                            assets = update.asset_ids.len(),
                            markets = update.markets_count,
                            "Received initial subscription list"
                        );
                        update
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to receive initial subscription");
                        return Err(anyhow::anyhow!("Initial subscription failed"));
                    }
                }
            }
        };

        // Create broadcast channels for WebSocket -> Parser -> Redis pipeline
        let (message_tx, _) = broadcast::channel::<ObservedMessage>(1000);
        let (trade_tx, _) = broadcast::channel::<PolymarketTradeEvent>(1000);
        let (price_change_tx, _) = broadcast::channel::<ParsedPriceChangeEvent>(1000);
        let (book_tx, _) = broadcast::channel::<ParsedBookEvent>(1000);

        // Create parser with metadata cache (runs continuously)
        let parser = EventParser::new(
            message_tx.subscribe(),
            trade_tx.clone(),
            price_change_tx.clone(),
            book_tx.clone(),
            Arc::clone(&self.metrics),
            Arc::clone(&market_metadata_cache),
        );
        let parser_token = cancellation_token.child_token();
        let parser_handle = tokio::spawn(async move { parser.run(parser_token).await });

        // Create Redis trade publisher (runs continuously)
        let redis_trade_publisher = RedisTradePublisher::new(
            &self.config,
            trade_tx.subscribe(),
            Arc::clone(&self.metrics),
        )
        .await?;
        let trade_publisher_token = cancellation_token.child_token();
        let trade_publisher_handle = tokio::spawn(async move {
            redis_trade_publisher.run(trade_publisher_token).await
        });

        // Create Redis orderbook publisher if configured (runs continuously)
        let orderbook_publisher_handle: Option<JoinHandle<Result<()>>> = if self.config.redis.orderbook_queues.is_some()
            || self.config.redis.orderbook_streams.is_some()
            || self.config.redis.orderbook_pubsub.is_some()
        {
            let redis_orderbook_publisher = RedisOrderbookPublisher::new(
                &self.config,
                price_change_tx.subscribe(),
                book_tx.subscribe(),
                Arc::clone(&self.metrics),
                Arc::clone(&market_metadata_cache),
            )
            .await?;
            let orderbook_publisher_token = cancellation_token.child_token();
            Some(tokio::spawn(async move {
                redis_orderbook_publisher.run(orderbook_publisher_token).await
            }))
        } else {
            None
        };

        // Create health monitor if enabled (shares subscription state with manager)
        let health_monitor_handle = if let Some(ref health_config) = self.config.polymarket.health_monitoring {
            if health_config.enabled {
                info!("Health monitoring enabled for discovery mode");
                let health_monitor = HealthMonitor::new(
                    health_config.clone(),
                    subscription_state,
                    Arc::clone(&self.metrics),
                );
                let health_token = cancellation_token.child_token();
                let trade_rx = trade_tx.subscribe();
                let price_change_rx = price_change_tx.subscribe();
                Some(tokio::spawn(async move {
                    health_monitor.run(trade_rx, price_change_rx, health_token).await
                }))
            } else {
                info!("Health monitoring disabled");
                None
            }
        } else {
            info!("Health monitoring not configured");
            None
        };

        // Start WebSocket with initial subscription
        let mut ws_handle = self.spawn_websocket_client(
            initial_subscription.asset_ids,
            message_tx.clone(),
            cancellation_token.child_token(),
        );

        // Main coordination loop
        loop {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    info!("Shutdown signal received, stopping all components");
                    break;
                }
                // Watch for WebSocket client exit
                result = &mut ws_handle => {
                    match result {
                        Ok(Ok(())) => {
                            info!("WebSocket client exited successfully");
                        }
                        Ok(Err(e)) => {
                            error!(error = %e, "WebSocket client exited with error");
                        }
                        Err(e) => {
                            error!(error = %e, "WebSocket task panicked");
                        }
                    }
                    // WebSocket exited unexpectedly, trigger shutdown
                    cancellation_token.cancel();
                    break;
                }
                // Watch for subscription updates
                result = subscription_rx.recv() => {
                    match result {
                        Ok(update) => {
                            info!(
                                assets = update.asset_ids.len(),
                                markets = update.markets_count,
                                "Subscription update received, reconnecting WebSocket"
                            );

                            // Cancel current WebSocket
                            ws_handle.abort();

                            // Wait a bit for cleanup
                            sleep(Duration::from_millis(500)).await;

                            // Spawn new WebSocket with updated subscription
                            ws_handle = self.spawn_websocket_client(
                                update.asset_ids,
                                message_tx.clone(),
                                cancellation_token.child_token(),
                            );

                            info!("WebSocket reconnection initiated");
                        }
                        Err(e) => {
                            warn!(error = %e, "Subscription channel closed");
                            break;
                        }
                    }
                }
            }
        }

        // Cleanup
        info!("Shutting down all components");
        ws_handle.abort();
        parser_handle.abort();
        trade_publisher_handle.abort();
        if let Some(handle) = orderbook_publisher_handle {
            handle.abort();
        }
        discovery_handle.abort();
        manager_handle.abort();
        if let Some(handle) = health_monitor_handle {
            handle.abort();
        }

        // Wait a bit for graceful cleanup
        sleep(Duration::from_secs(1)).await;

        info!("Shutdown complete");
        Ok(())
    }

    /// Spawn a WebSocket client task with the given asset IDs
    fn spawn_websocket_client(
        &self,
        asset_ids: Vec<String>,
        message_tx: broadcast::Sender<ObservedMessage>,
        cancellation_token: CancellationToken,
    ) -> JoinHandle<Result<()>> {
        let ws_client = PolymarketWebSocketClient::new(
            self.config.polymarket.wss_endpoint.clone(),
            asset_ids,
            message_tx,
            Arc::clone(&self.metrics),
        );

        tokio::spawn(async move { ws_client.run(cancellation_token).await })
    }
}
