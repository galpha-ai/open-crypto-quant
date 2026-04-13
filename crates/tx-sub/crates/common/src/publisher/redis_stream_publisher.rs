use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use popeyes_trading_types::{Event, MarketDataEvent, TokenEvent};
use redis::{Client, aio::ConnectionManager};
use tracing::debug;

use super::traits::RedisPublisher;
use crate::config::StreamConfig;

pub struct RedisStreamPublisher {
    conn_manager: ConnectionManager,
    streams: Vec<StreamConfig>,
}

impl RedisStreamPublisher {
    pub async fn new(redis_url: &str, streams: Vec<StreamConfig>) -> Result<Self> {
        let redis_client = Client::open(redis_url)?;
        let conn_manager = ConnectionManager::new(redis_client).await?;

        Ok(Self {
            conn_manager,
            streams,
        })
    }
}

#[async_trait]
impl RedisPublisher for RedisStreamPublisher {
    async fn publish(&self, event: Event) -> Result<()> {
        let json_str = self.convert_event_format(&event)?;
        let event_type = get_event_type(&event);
        self.publish_raw_with_event_type(json_str, event_type).await
    }

    async fn publish_raw(&self, json: String) -> Result<()> {
        self.publish_raw_with_event_type(json, "raw").await
    }

    fn name(&self) -> &str {
        "RedisStreamPublisher"
    }
}

impl RedisStreamPublisher {
    fn convert_event_format(&self, event: &Event) -> Result<String> {
        serde_json::to_string(event)
            .with_context(|| format!("Failed to serialize event with payload: {:?}", event))
            .map_err(|e| anyhow!("{:#?}", e))
    }

    async fn publish_raw_with_event_type(&self, json: String, event_type: &str) -> Result<()> {
        let mut conn = self.conn_manager.clone();
        let timestamp = chrono::Utc::now().to_rfc3339();

        for stream in &self.streams {
            let mut cmd = redis::cmd("XADD");
            cmd.arg(&stream.name);

            // Add max length configuration if specified
            if let Some(max_length) = stream.max_length {
                cmd.arg("MAXLEN");
                cmd.arg("~"); // Use approximate trimming for better performance
                cmd.arg(max_length);
            }

            // Use "*" to auto-generate ID
            cmd.arg("*");

            // Add the event data as a field
            cmd.arg("data");
            cmd.arg(&json);

            // Add metadata fields
            cmd.arg("event_type");
            cmd.arg(event_type);

            cmd.arg("timestamp");
            cmd.arg(&timestamp);

            let _: String = cmd
                .query_async(&mut conn)
                .await
                .with_context(|| format!("Failed to XADD to stream {}", stream.name))?;

            debug!(
                stream_name = stream.name,
                event_type = event_type,
                "Successfully added event to Redis stream"
            );
        }

        Ok(())
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
