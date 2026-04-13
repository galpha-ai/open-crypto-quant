use std::sync::Arc;

use anyhow::Result;
use futures_util::future;
use prometheus::Registry;
use tokio::sync::broadcast;
use tracing::info;

use crate::{
    config::{AppConfig, RedisConfig},
    grpc::{GrpcDataSubscriptionManager, GrpcSubscriptionConfig, TransactionData},
    metrics::Metrics,
    parser::{MarketTrade, ParserConfig, TransactionParser},
    redis_trade_consumer::RedisTradeConsumer,
    redis_tx_consumer::RedisTxConsumer,
    stats_monitor::StatsMonitor,
};

pub struct App {
    grpc_subscriber: GrpcDataSubscriptionManager,
    parser: TransactionParser,
    trade_rx: Option<broadcast::Receiver<MarketTrade>>,
    redis_config: RedisConfig,
    redis_persister: Option<RedisTxConsumer>,
    registry: Registry,
    metrics_port: Option<u16>,
    metrics: Arc<Metrics>,
    test_mode: bool,
    test_config: Option<crate::config::TestConfig>,
    shutdown_tx: Option<broadcast::Sender<()>>,
    stats_monitor: Option<Arc<StatsMonitor>>,
    stats_monitor_config: Option<crate::config::StatsMonitorConfig>,
}

impl App {
    pub fn new(
        app_config: &AppConfig,
        grpc_config: GrpcSubscriptionConfig,
        parser_config: ParserConfig,
    ) -> Self {
        let redis_config = app_config.redis.clone();
        let metrics_port = app_config.metrics.as_ref().and_then(|m| m.port);
        let registry = Registry::new();
        let metrics = Arc::new(Metrics::new(&registry));

        // Create broadcast channel for TransactionData
        let (grpc_tx_sender, _) = broadcast::channel::<TransactionData>(1000);
        // Create channels for communication between components
        let (trade_tx, _) = broadcast::channel::<MarketTrade>(1000); // Sender for parser
        let trade_rx = trade_tx.subscribe(); // Receiver for consumer(s)

        // Create GRPC subscriber with shared metrics
        let grpc_subscriber = GrpcDataSubscriptionManager::new(
            grpc_config,
            grpc_tx_sender.clone(),
            Arc::clone(&metrics),
        );

        // Create stats monitor if enabled
        let stats_monitor = if let Some(stats_config) = &app_config.stats_monitor {
            if stats_config.enabled {
                Some(StatsMonitor::new_with_metrics(
                    std::time::Duration::from_secs(stats_config.window_seconds.unwrap_or(10)),
                    std::time::Duration::from_secs(stats_config.log_interval_seconds.unwrap_or(5)),
                    true,
                    Arc::clone(&metrics),
                ))
            } else {
                None
            }
        } else {
            None
        };

        // Create parser with stats monitor
        let parser_grpc_rx = grpc_tx_sender.subscribe();
        let parser = TransactionParser::new(
            parser_grpc_rx,
            trade_tx,
            parser_config,
            stats_monitor.clone(),
        );

        // Create Redis persister if enabled in config
        let redis_persister = if let Some(persist_config) = &app_config.persist_tx_redis {
            if persist_config.enabled {
                let persister_grpc_rx = grpc_tx_sender.subscribe();
                let persister = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        RedisTxConsumer::new(
                            &redis_config.url,
                            persist_config.clone(),
                            persister_grpc_rx,
                        )
                        .await
                    })
                })
                .expect("Failed to create Redis Persister");
                Some(persister)
            } else {
                None
            }
        } else {
            None
        };

        let test_mode = app_config.test.is_some();
        let test_config = app_config.test.clone();
        let stats_monitor_config = app_config.stats_monitor.clone();

        Self {
            grpc_subscriber,
            parser,
            registry,
            trade_rx: Some(trade_rx),
            redis_config,
            metrics_port,
            redis_persister,
            metrics,
            test_mode,
            test_config,
            shutdown_tx: None,
            stats_monitor,
            stats_monitor_config,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Starting application");

        // Spawn stats monitor logging task if enabled
        if let Some(monitor) = &self.stats_monitor {
            Arc::clone(monitor).spawn_logging_task();
        }

        // Spawn inactivity watchdog if enabled and not in test mode
        if !self.test_mode {
            if let Some(monitor) = &self.stats_monitor {
                if let Some(config) = &self.stats_monitor_config {
                    if let Some(timeout_secs) = config.inactivity_timeout_seconds {
                        info!(
                            timeout_seconds = timeout_secs,
                            "Starting inactivity watchdog"
                        );
                        Arc::clone(monitor).spawn_inactivity_watchdog(
                            std::time::Duration::from_secs(timeout_secs),
                        );
                    }
                }
            }
        }

        let trade_rx = self.trade_rx.take().expect("trade_rx should be available");

        // Create shutdown channel for test mode
        let (shutdown_tx, mut shutdown_rx) = if self.test_mode {
            let (tx, rx) = broadcast::channel(1);
            self.shutdown_tx = Some(tx.clone());
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        // In test mode, use test trade printer instead of Redis consumer
        if self.test_mode {
            let test_config = self
                .test_config
                .as_ref()
                .expect("test_config should be set in test mode");
            let shutdown_tx = shutdown_tx.expect("shutdown_tx should be set in test mode");

            info!("Running in test mode - using test trade printer");

            // Start test trade printer
            let mut test_printer = crate::test_trade_printer::TestTradePrinter::new(
                trade_rx,
                test_config.event_count,
                test_config.print_format.clone(),
                shutdown_tx,
            );

            // Run without metrics server
            tokio::select! {
                result = self.grpc_subscriber.start() => result?,
                result = self.parser.start() => result?,
                result = test_printer.run() => result?,
                _ = async {
                    if let Some(rx) = &mut shutdown_rx {
                        let _ = rx.recv().await;
                        info!("Received shutdown signal");
                    } else {
                        future::pending::<()>().await;
                    }
                } => {
                    info!("Shutting down test mode");
                    return Ok(());
                }
            }
        } else {
            // Normal mode with Redis consumer
            let mut redis_consumer = RedisTradeConsumer::new(
                &self.redis_config.url,
                self.redis_config.queues.clone(),
                self.redis_config.streams.clone(),
                self.redis_config.pubsub.clone(),
                trade_rx,
                Arc::clone(&self.metrics),
            )
            .await?;

            // Start metrics server with configured port
            common::metrics::start_metrics_server(self.registry.clone(), self.metrics_port);

            // Start Redis persister task if enabled
            let persister_handle = if let Some(mut persister) = self.redis_persister.take() {
                Some(tokio::spawn(async move { persister.start().await }))
            } else {
                None
            };

            // Start all components concurrently
            tokio::select! {
                result = self.grpc_subscriber.start() => result?,
                result = self.parser.start() => result?,
                result = redis_consumer.start() => result?,
                res = async {
                    match persister_handle {
                        Some(handle) => handle.await.map_err(anyhow::Error::from)?,
                        None => future::pending().await,
                    }
                } => match res {
                    Ok(_) => tracing::info!("Redis persister finished cleanly"),
                    Err(e) => tracing::error!(error = ?e, "Redis persister task failed"),
                }
            }
        }
        Ok(())
    }
}
