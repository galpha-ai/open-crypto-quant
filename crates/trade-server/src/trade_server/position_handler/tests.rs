//! Tests for position handler.

use std::any::Any;

use chrono::Utc;

use crate::notifier::Notifiable;
use crate::signal::{OrderIntent, QuoteLevel, SignalAction, TradableSignal};

/// A test signal that implements `is_intent_signal() = true`
#[derive(Debug, Clone)]
pub struct TestIntentSignal {
    signal_id: String,
    mint: String,
    market: Option<String>,
    intent: OrderIntent,
}

impl TestIntentSignal {
    pub fn new(mint: &str, bids: Option<Vec<QuoteLevel>>, asks: Option<Vec<QuoteLevel>>) -> Self {
        let signal_id = uuid::Uuid::new_v4().to_string();
        Self {
            signal_id: signal_id.clone(),
            mint: mint.to_string(),
            market: Some("test_market".to_string()),
            intent: OrderIntent::new(
                mint.to_string(),
                Some("test_market".to_string()),
                bids,
                asks,
                signal_id,
                Utc::now(),
                None,
            ),
        }
    }
}

impl TradableSignal for TestIntentSignal {
    fn signal_id(&self) -> &str {
        &self.signal_id
    }

    fn signal_type(&self) -> &str {
        "test_intent"
    }

    fn get_mint(&self) -> Option<&str> {
        Some(&self.mint)
    }

    fn get_price(&self) -> Option<f64> {
        Some(0.5)
    }

    fn get_timestamp(&self) -> Option<chrono::DateTime<Utc>> {
        Some(Utc::now())
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

    fn to_json(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "signal_id": self.signal_id,
            "mint": self.mint,
        }))
    }

    fn get_market(&self) -> Option<&str> {
        self.market.as_deref()
    }

    fn is_intent_signal(&self) -> bool {
        true
    }

    fn get_order_intent(&self) -> Option<OrderIntent> {
        Some(self.intent.clone())
    }
}

/// A regular (non-intent) test signal
#[derive(Debug, Clone)]
pub struct TestActionSignal {
    signal_id: String,
    mint: String,
    action: SignalAction,
}

impl TestActionSignal {
    pub fn entry(mint: &str) -> Self {
        Self {
            signal_id: uuid::Uuid::new_v4().to_string(),
            mint: mint.to_string(),
            action: SignalAction::Entry,
        }
    }
}

impl TradableSignal for TestActionSignal {
    fn signal_id(&self) -> &str {
        &self.signal_id
    }

    fn signal_type(&self) -> &str {
        "test_action"
    }

    fn get_mint(&self) -> Option<&str> {
        Some(&self.mint)
    }

    fn get_price(&self) -> Option<f64> {
        Some(0.5)
    }

    fn get_timestamp(&self) -> Option<chrono::DateTime<Utc>> {
        Some(Utc::now())
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

    fn to_json(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "signal_id": self.signal_id,
            "mint": self.mint,
        }))
    }

    fn signal_action(&self) -> SignalAction {
        self.action
    }

    // Explicitly NOT intent-based
    fn is_intent_signal(&self) -> bool {
        false
    }
}

#[test]
fn test_intent_signal_returns_true() {
    let signal = TestIntentSignal::new(
        "token123",
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        Some(vec![QuoteLevel::gtc(0.55, 100.0)]),
    );

    assert!(signal.is_intent_signal());
    assert!(signal.get_order_intent().is_some());
}

#[test]
fn test_action_signal_returns_false() {
    let signal = TestActionSignal::entry("token123");

    assert!(!signal.is_intent_signal());
}

#[test]
fn test_intent_signal_contains_correct_levels() {
    let bids = vec![QuoteLevel::gtc(0.45, 100.0)];
    let asks = vec![QuoteLevel::gtc(0.55, 100.0)];

    let signal = TestIntentSignal::new("token123", Some(bids.clone()), Some(asks.clone()));

    let intent = signal.get_order_intent().unwrap();
    assert_eq!(intent.mint, "token123");
    assert_eq!(intent.bid_count(), 1);
    assert_eq!(intent.ask_count(), 1);
}
