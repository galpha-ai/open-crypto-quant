use std::sync::Arc;

use anyhow::Result;
use popeyes_trading_types::Event;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use common::publisher::{RedisListPublisher, RedisPublisher, RedisPubsubPublisher, RedisStreamPublisher};
use crate::{
    config::{PubsubConfig, QueueConfig, StreamConfig},
    metrics::Metrics,
    parser::{
        MarketTrade,
        TradeLog::{Bonk, MeteoraDLMM, Pumpfun, PumpfunTokenCreate, RayAMMv4},
    },
};

pub struct RedisTradeConsumer {
    publishers: Vec<Box<dyn RedisPublisher>>,
    trade_rx: broadcast::Receiver<MarketTrade>,
    metrics: Arc<Metrics>,
}

impl RedisTradeConsumer {
    pub async fn new(
        redis_url: &str,
        queues: Vec<QueueConfig>,
        streams: Option<Vec<StreamConfig>>,
        pubsub: Option<PubsubConfig>,
        trade_rx: broadcast::Receiver<MarketTrade>,
        metrics: Arc<Metrics>,
    ) -> Result<Self> {
        let mut publishers: Vec<Box<dyn RedisPublisher>> = Vec::new();

        // Add list publisher if queues are configured
        if !queues.is_empty() {
            let list_publisher = RedisListPublisher::new(redis_url, queues).await?;
            publishers.push(Box::new(list_publisher));
            info!("Initialized RedisListPublisher");
        }

        // Add stream publisher if streams are configured
        if let Some(stream_configs) = streams {
            if !stream_configs.is_empty() {
                let stream_publisher = RedisStreamPublisher::new(redis_url, stream_configs).await?;
                publishers.push(Box::new(stream_publisher));
                info!("Initialized RedisStreamPublisher");
            }
        }

        // Add pubsub publisher if pubsub is configured
        if let Some(pubsub_config) = pubsub {
            if !pubsub_config.channels.is_empty() {
                let pubsub_publisher = RedisPubsubPublisher::new(redis_url, pubsub_config).await?;
                publishers.push(Box::new(pubsub_publisher));
                info!("Initialized RedisPubsubPublisher");
            }
        }

        if publishers.is_empty() {
            return Err(anyhow::anyhow!(
                "No Redis publishers configured. Please configure at least one queue, stream, or pubsub channel."
            ));
        }

        Ok(Self {
            publishers,
            trade_rx,
            metrics,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        tracing::debug!("Starting Redis trade consumer");

        loop {
            match self.trade_rx.recv().await {
                Ok(trade) => {
                    let dex_str = match trade.log {
                        Pumpfun(_) => "Pumpfun",
                        PumpfunTokenCreate(_) => "Pumpfun",
                        RayAMMv4(_) => "RayAMMv4",
                        MeteoraDLMM(_) => "MeteoraDLMM",
                        Bonk(_) => "Bonk",
                    };
                    self.metrics
                        .parsed_trades_total
                        .with_label_values(&[&dex_str])
                        .inc();
                    debug!(dex = %dex_str, "Incremented parsed trade counter");

                    let token_event = trade.to_token_event();
                    // Wrap in Event::Token for the publisher
                    let wrapped_event = Event::Token(token_event);
                    for publisher in &self.publishers {
                        let publisher_type = match publisher.name() {
                            "RedisListPublisher" => "list",
                            "RedisStreamPublisher" => "stream",
                            "RedisPubsubPublisher" => "pubsub",
                            _ => "unknown",
                        };

                        match publisher.publish(wrapped_event.clone()).await {
                            Ok(_) => {
                                self.metrics
                                    .redis_publish_total
                                    .with_label_values(&[publisher_type, "success"])
                                    .inc();
                            }
                            Err(e) => {
                                self.metrics
                                    .redis_publish_total
                                    .with_label_values(&[publisher_type, "failure"])
                                    .inc();
                                tracing::error!(
                                    err = ?e,
                                    publisher = publisher.name(),
                                    "Error publishing trade event"
                                );
                            }
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(count = n, "Redis trade consumer lagged behind trade stream");
                    // Continue processing
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("Trade broadcast channel closed, stopping Redis consumer.");
                    break; // Exit loop
                }
            }
        }

        Ok(())
    }
}
