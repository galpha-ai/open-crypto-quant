use std::any::Any;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use popeyes_trading_types::MarketDataEvent;
use trade_server::{
    domain::SystemEvent,
    notifier::Notifiable,
    signal::{OrderIntent, QuoteLevel, SignalGenerator, SignalMetadata, TradableSignal},
};

#[derive(Debug, Clone)]
pub struct BuyIntentSignal {
    metadata: SignalMetadata,
    mint: String,
    market: String,
    bid_price: f64,
    trade_amount: f64,
}

impl BuyIntentSignal {
    pub fn new(
        mint: String,
        market: String,
        bid_price: f64,
        trade_amount: f64,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            metadata: SignalMetadata::with_timestamp(timestamp),
            mint,
            market,
            bid_price,
            trade_amount,
        }
    }
}

impl TradableSignal for BuyIntentSignal {
    fn signal_id(&self) -> &str {
        self.metadata.id()
    }

    fn signal_type(&self) -> &str {
        "cheap_outcome_buy"
    }

    fn get_mint(&self) -> Option<&str> {
        Some(&self.mint)
    }

    fn get_market(&self) -> Option<&str> {
        Some(&self.market)
    }

    fn get_price(&self) -> Option<f64> {
        Some(self.bid_price)
    }

    fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        Some(self.metadata.timestamp())
    }

    fn get_slot(&self) -> Option<u64> {
        None
    }

    fn get_creator(&self) -> Option<solana_sdk::pubkey::Pubkey> {
        None
    }

    fn passes_filter(&self) -> bool {
        true
    }

    fn as_notifiable(&self) -> Option<Box<dyn Notifiable + Send + Sync>> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "signal_id": self.signal_id(),
            "signal_type": self.signal_type(),
            "mint": self.mint,
            "market": self.market,
            "bid_price": self.bid_price,
            "trade_amount": self.trade_amount,
            "timestamp": self.metadata.timestamp(),
            "is_intent_signal": true,
        }))
    }

    fn is_intent_signal(&self) -> bool {
        true
    }

    fn get_order_intent(&self) -> Option<OrderIntent> {
        Some(OrderIntent::new(
            self.mint.clone(),
            Some(self.market.clone()),
            Some(vec![QuoteLevel::gtc(self.bid_price, self.trade_amount)]),
            None,
            self.signal_id().to_string(),
            self.metadata.timestamp(),
            self.get_context(),
        ))
    }
}

pub struct CheapBuyerGenerator {
    threshold: f64,
    trade_amount: f64,
    logical_time: Option<DateTime<Utc>>,
}

impl CheapBuyerGenerator {
    pub fn new(threshold: f64, trade_amount: f64) -> Self {
        Self {
            threshold,
            trade_amount,
            logical_time: None,
        }
    }
}

#[async_trait]
impl SignalGenerator for CheapBuyerGenerator {
    async fn generate_signal(
        &mut self,
        event: &SystemEvent,
    ) -> Result<Vec<Box<dyn TradableSignal>>> {
        if let Some(ts) = event.timestamp() {
            self.logical_time = Some(ts);
        }

        let snapshot = match event {
            SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(snapshot)) => snapshot,
            _ => return Ok(vec![]),
        };

        let best_bid = match snapshot.bids.first() {
            Some(level) => level.price,
            None => return Ok(vec![]),
        };
        let best_ask = match snapshot.asks.first() {
            Some(level) => level.price,
            None => return Ok(vec![]),
        };
        if best_ask <= 0.0 {
            return Ok(vec![]);
        }

        let mid_price = (best_bid + best_ask) / 2.0;
        if mid_price >= self.threshold {
            return Ok(vec![]);
        }

        let signal_time = self
            .logical_time
            .or_else(|| DateTime::from_timestamp_millis(snapshot.timestamp))
            .unwrap_or_else(Utc::now);

        let signal = BuyIntentSignal::new(
            snapshot.asset_id.clone(),
            snapshot.market.clone(),
            best_ask,
            self.trade_amount,
            signal_time,
        );

        Ok(vec![Box::new(signal)])
    }
}
