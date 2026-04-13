use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use popeyes_trading_types::{Event, MarketDataEvent, TokenEvent};
use redis::{Client, aio::ConnectionManager};
use tracing::debug;

use super::traits::RedisPublisher;
use crate::config::PubsubConfig;

pub struct RedisPubsubPublisher {
    conn_manager: ConnectionManager,
    pubsub_config: PubsubConfig,
}

impl RedisPubsubPublisher {
    pub async fn new(redis_url: &str, pubsub_config: PubsubConfig) -> Result<Self> {
        let redis_client = Client::open(redis_url)?;
        let conn_manager = ConnectionManager::new(redis_client).await?;

        Ok(Self {
            conn_manager,
            pubsub_config,
        })
    }
}

#[async_trait]
impl RedisPublisher for RedisPubsubPublisher {
    async fn publish(&self, event: Event) -> Result<()> {
        let json_str = self.convert_event_format(&event)?;
        let event_type = get_event_type(&event);

        let mut conn = self.conn_manager.clone();

        for channel in &self.pubsub_config.channels {
            let _: i32 = redis::cmd("PUBLISH")
                .arg(channel)
                .arg(&json_str)
                .query_async(&mut conn)
                .await
                .with_context(|| format!("Failed to publish to channel {}", channel))?;

            debug!(
                channel_name = channel,
                event_type = event_type,
                "Successfully published event to Redis channel"
            );
        }

        Ok(())
    }

    async fn publish_raw(&self, json: String) -> Result<()> {
        let mut conn = self.conn_manager.clone();

        for channel in &self.pubsub_config.channels {
            let _: i32 = redis::cmd("PUBLISH")
                .arg(channel)
                .arg(&json)
                .query_async(&mut conn)
                .await
                .with_context(|| format!("Failed to publish to channel {}", channel))?;

            debug!(
                channel_name = channel,
                "Successfully published raw event to Redis channel"
            );
        }

        Ok(())
    }

    fn name(&self) -> &str {
        "RedisPubsubPublisher"
    }
}

impl RedisPubsubPublisher {
    fn convert_event_format(&self, event: &Event) -> Result<String> {
        serde_json::to_string(event)
            .with_context(|| format!("Failed to serialize event with payload: {:?}", event))
            .map_err(|e| anyhow!("{:#?}", e))
    }
}

fn get_event_type(event: &Event) -> &'static str {
    match event {
        Event::Token(token_event) => match token_event {
            TokenEvent::Create(_) => "create",
            TokenEvent::Buy(_) => "buy",
            TokenEvent::Sell(_) => "sell",
            TokenEvent::Swap(_) => "swap",
        },
        Event::MarketData(market_data_event) => match market_data_event {
            MarketDataEvent::OrderbookUpdate(_) => "orderbook_update",
            MarketDataEvent::OrderbookSnapshot(_) => "orderbook_snapshot",
            MarketDataEvent::SpotPrice(_) => "spot_price",
            MarketDataEvent::PolymarketTrade(_) => "polymarket_trade",
        },
    }
}
