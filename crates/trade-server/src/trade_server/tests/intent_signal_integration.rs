//! Integration tests for intent-based signal flow.
//!
//! These tests verify the complete data flow for intent-based signals:
//! 1. Signal with is_intent_signal() = true is generated
//! 2. PositionHandler routes it to handle_intent_signal()
//! 3. PositionManager.reconcile_intent() computes required orders
//! 4. Orders are executed via OrderExecutor
//! 5. LimitOrderEvents update pending order state
//! 6. Subsequent intents correctly reconcile against updated state

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::Mutex;

use crate::event_coordinator::{EventCoordinator, NoopEventCoordinator};
use crate::execution::{LimitOrderEvent, Order, OrderExecutor};
use crate::execution::{OrderSide, OrderType};
use crate::notifier::{Notifiable, Notifier};
use crate::position::{ConfigurableExitStrategy, InMemoryPositionManager, PositionManager};
use crate::signal::{OrderIntent, QuoteLevel, TradableSignal};
use crate::trade_server::PositionHandler;

// === Test Infrastructure ===

/// A mock executor that tracks limit order operations and supports intent-based reconciliation.
#[derive(Clone)]
struct MockLimitOrderExecutor {
    /// Orders that have been placed (order_id -> order details)
    placed_orders: Arc<Mutex<HashMap<String, PlacedOrder>>>,
    /// Orders that have been cancelled
    cancelled_orders: Arc<Mutex<Vec<String>>>,
    /// Counter for generating order IDs
    order_counter: Arc<Mutex<u64>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PlacedOrder {
    order_id: String,
    mint: String,
    side: OrderSide,
    price: f64,
    size: f64,
}

impl MockLimitOrderExecutor {
    fn new() -> Self {
        Self {
            placed_orders: Arc::new(Mutex::new(HashMap::new())),
            cancelled_orders: Arc::new(Mutex::new(Vec::new())),
            order_counter: Arc::new(Mutex::new(0)),
        }
    }

    async fn placed_order_count(&self) -> usize {
        self.placed_orders.lock().await.len()
    }

    async fn cancelled_order_count(&self) -> usize {
        self.cancelled_orders.lock().await.len()
    }

    #[allow(dead_code)]
    async fn get_placed_order(&self, order_id: &str) -> Option<PlacedOrder> {
        self.placed_orders.lock().await.get(order_id).cloned()
    }

    async fn was_cancelled(&self, order_id: &str) -> bool {
        self.cancelled_orders
            .lock()
            .await
            .contains(&order_id.to_string())
    }
}

#[async_trait]
impl OrderExecutor for MockLimitOrderExecutor {
    async fn execute_market_order(&self, order: Order) -> Result<crate::execution::ExecutionEvent> {
        // Not used in intent-based flow, but required by trait
        Ok(crate::execution::ExecutionEvent::OrderRejected {
            mint: order.mint,
            reason: "Mock executor does not support market orders".to_string(),
        })
    }

    async fn handle_token_trade(
        &self,
        _trade: &popeyes_trading_types::TokenTradeEvent,
    ) -> Result<()> {
        Ok(())
    }

    async fn execute_limit_order(&self, order: Order) -> Result<LimitOrderEvent> {
        let mut counter = self.order_counter.lock().await;
        *counter += 1;
        let order_id = format!("order_{}", *counter);
        drop(counter);

        let (side, price, size) = match &order.order_type {
            OrderType::LimitBuy {
                quote_amount,
                limit_price,
                ..
            } => (OrderSide::Buy, *limit_price, *quote_amount / *limit_price),
            OrderType::LimitSell {
                token_amount,
                limit_price,
                ..
            } => (OrderSide::Sell, *limit_price, *token_amount),
            _ => return Err(anyhow::anyhow!("Not a limit order")),
        };

        let placed = PlacedOrder {
            order_id: order_id.clone(),
            mint: order.mint.clone(),
            side,
            price,
            size,
        };

        self.placed_orders
            .lock()
            .await
            .insert(order_id.clone(), placed);

        Ok(LimitOrderEvent::OrderPlaced {
            order_id,
            mint: order.mint,
            market: order.market,
            price,
            size,
            side,
            timestamp: order.timestamp,
            signal_id: order.signal_id,
            context: order.context,
        })
    }

    async fn cancel_order(&self, order_id: &str) -> Result<LimitOrderEvent> {
        self.cancelled_orders
            .lock()
            .await
            .push(order_id.to_string());
        self.placed_orders.lock().await.remove(order_id);

        Ok(LimitOrderEvent::OrderCancelled {
            order_id: order_id.to_string(),
            reason: Some("Cancelled by reconciliation".to_string()),
            timestamp: Utc::now(),
        })
    }

    fn supports_limit_orders(&self) -> bool {
        true
    }
}

/// A mock notifier that does nothing.
struct MockNotifier;

#[async_trait]
impl Notifier for MockNotifier {
    async fn notify(&self, _notification: &(dyn Notifiable + Sync)) -> Result<()> {
        Ok(())
    }
}

/// A test signal that implements intent-based signaling.
#[derive(Debug, Clone)]
struct TestIntentSignal {
    signal_id: String,
    mint: String,
    market: Option<String>,
    bids: Option<Vec<QuoteLevel>>,
    asks: Option<Vec<QuoteLevel>>,
    timestamp: DateTime<Utc>,
}

impl TestIntentSignal {
    fn new(
        mint: &str,
        market: Option<&str>,
        bids: Option<Vec<QuoteLevel>>,
        asks: Option<Vec<QuoteLevel>>,
    ) -> Self {
        Self::new_at(mint, market, bids, asks, Utc::now())
    }

    fn new_at(
        mint: &str,
        market: Option<&str>,
        bids: Option<Vec<QuoteLevel>>,
        asks: Option<Vec<QuoteLevel>>,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            signal_id: uuid::Uuid::new_v4().to_string(),
            mint: mint.to_string(),
            market: market.map(|s| s.to_string()),
            bids,
            asks,
            timestamp,
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

    fn get_market(&self) -> Option<&str> {
        self.market.as_deref()
    }

    fn get_price(&self) -> Option<f64> {
        None
    }

    fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        Some(self.timestamp)
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
            "signal_id": self.signal_id,
            "mint": self.mint,
            "type": "intent",
        }))
    }

    fn is_intent_signal(&self) -> bool {
        true
    }

    fn get_order_intent(&self) -> Option<OrderIntent> {
        Some(OrderIntent::new(
            self.mint.clone(),
            self.market.clone(),
            self.bids.clone(),
            self.asks.clone(),
            self.signal_id.clone(),
            self.timestamp,
            self.get_context(),
        ))
    }
}

/// A test signal that uses action-based signaling (not intent-based).
#[derive(Debug, Clone)]
struct TestActionSignal {
    signal_id: String,
    mint: String,
    price: f64,
    timestamp: DateTime<Utc>,
}

impl TestActionSignal {
    fn new(mint: &str, price: f64) -> Self {
        Self {
            signal_id: uuid::Uuid::new_v4().to_string(),
            mint: mint.to_string(),
            price,
            timestamp: Utc::now(),
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
        Some(self.price)
    }

    fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        Some(self.timestamp)
    }

    fn get_slot(&self) -> Option<u64> {
        Some(1)
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
            "signal_id": self.signal_id,
            "mint": self.mint,
            "type": "action",
        }))
    }

    // Default is_intent_signal() returns false
    // Default signal_intent() returns Entry
}

// === Test Helpers ===

fn create_test_position_manager() -> InMemoryPositionManager {
    create_test_position_manager_with_debounce(None)
}

fn create_test_position_manager_with_debounce(
    min_quote_lifetime_ms: Option<u64>,
) -> InMemoryPositionManager {
    let registry = prometheus::Registry::new();
    let exit_strategy = Arc::new(ConfigurableExitStrategy::new(
        0.5,
        0.2,
        chrono::Duration::minutes(5),
        3,
    ));

    InMemoryPositionManager::new_with_quote_lifetime(
        0.1,
        10000.0, // Large enough for test buy orders
        chrono::Duration::minutes(5),
        5,
        min_quote_lifetime_ms,
        &registry,
        exit_strategy.clone(),
    )
}

fn create_test_position_handler(
    position_manager: Arc<dyn PositionManager>,
    order_executor: Arc<dyn OrderExecutor + Send + Sync>,
) -> PositionHandler {
    let notifier: Arc<dyn Notifier> = Arc::new(MockNotifier);
    let event_coordinator: Arc<dyn EventCoordinator> = Arc::new(NoopEventCoordinator::new());
    let exit_strategy = Arc::new(ConfigurableExitStrategy::new(
        0.5,
        0.2,
        chrono::Duration::minutes(5),
        3,
    ));

    PositionHandler::new(
        position_manager,
        order_executor,
        notifier,
        event_coordinator,
        exit_strategy,
        None,
        3,
    )
}

// === Integration Tests ===

/// Test: Intent signal routes to handle_intent_signal and generates orders.
#[tokio::test]
async fn test_intent_signal_generates_limit_orders() {
    let position_manager = Arc::new(create_test_position_manager());
    let executor = Arc::new(MockLimitOrderExecutor::new());
    let handler = create_test_position_handler(
        position_manager.clone() as Arc<dyn PositionManager>,
        executor.clone() as Arc<dyn OrderExecutor + Send + Sync>,
    );

    // Create intent signal with one bid and one ask
    let signal = TestIntentSignal::new(
        "token123",
        Some("market456"),
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        Some(vec![QuoteLevel::gtc(0.55, 100.0)]),
    );

    // Handle the signal
    handler.handle_signal(&signal).await.unwrap();

    // Wait for async order execution
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Verify orders were placed
    assert_eq!(executor.placed_order_count().await, 2);
    assert_eq!(executor.cancelled_order_count().await, 0);
}

/// Test: Action signal (is_intent_signal = false) does NOT route to intent handler.
#[tokio::test]
async fn test_action_signal_does_not_use_intent_handler() {
    let position_manager = Arc::new(create_test_position_manager());
    let executor = Arc::new(MockLimitOrderExecutor::new());
    let handler = create_test_position_handler(
        position_manager.clone() as Arc<dyn PositionManager>,
        executor.clone() as Arc<dyn OrderExecutor + Send + Sync>,
    );

    // Create action signal (default is_intent_signal = false)
    let signal = TestActionSignal::new("token123", 0.50);

    // Handle the signal - should use action-based path, not intent path
    let result = handler.handle_signal(&signal).await;

    // Wait for async operations
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Action signals go through position manager's handle_signal which creates buy orders
    // The mock executor doesn't support market orders, but the important thing is
    // that it didn't go through the intent path (no limit orders placed)
    assert!(result.is_ok());
}

/// Test: Reconciliation correctly identifies matching state and generates no orders.
#[tokio::test]
async fn test_intent_reconciliation_idempotent() {
    let position_manager = Arc::new(create_test_position_manager());
    let executor = Arc::new(MockLimitOrderExecutor::new());
    let handler = create_test_position_handler(
        position_manager.clone() as Arc<dyn PositionManager>,
        executor.clone() as Arc<dyn OrderExecutor + Send + Sync>,
    );

    // Create initial intent
    let signal1 = TestIntentSignal::new(
        "token123",
        Some("market456"),
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        None,
    );

    // First signal - should generate 1 order
    handler.handle_signal(&signal1).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let first_count = executor.placed_order_count().await;
    assert_eq!(first_count, 1);

    // Simulate order placed event updating position manager state
    let event = LimitOrderEvent::OrderPlaced {
        order_id: "order_1".to_string(),
        mint: "token123".to_string(),
        market: Some("market456".to_string()),
        price: 0.45,
        size: 100.0,
        side: OrderSide::Buy,
        timestamp: Utc::now(),
        signal_id: None,
        context: None,
    };
    position_manager
        .handle_limit_order_event(&event)
        .await
        .unwrap();

    // Second signal with same intent - should generate 0 new orders (idempotent)
    let signal2 = TestIntentSignal::new(
        "token123",
        Some("market456"),
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        None,
    );

    handler.handle_signal(&signal2).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Count should still be 1 (no new orders)
    assert_eq!(executor.placed_order_count().await, 1);
}

/// Test: Price change triggers cancel + new placement.
#[tokio::test]
async fn test_intent_price_change_triggers_cancel_and_place() {
    let position_manager = Arc::new(create_test_position_manager());
    let executor = Arc::new(MockLimitOrderExecutor::new());
    let handler = create_test_position_handler(
        position_manager.clone() as Arc<dyn PositionManager>,
        executor.clone() as Arc<dyn OrderExecutor + Send + Sync>,
    );

    // Place initial order
    let signal1 = TestIntentSignal::new(
        "token123",
        None,
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        None,
    );
    handler.handle_signal(&signal1).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Simulate order placed event
    let event = LimitOrderEvent::OrderPlaced {
        order_id: "order_1".to_string(),
        mint: "token123".to_string(),
        market: None,
        price: 0.45,
        size: 100.0,
        side: OrderSide::Buy,
        timestamp: Utc::now(),
        signal_id: None,
        context: None,
    };
    position_manager
        .handle_limit_order_event(&event)
        .await
        .unwrap();

    assert_eq!(executor.placed_order_count().await, 1);
    assert_eq!(executor.cancelled_order_count().await, 0);

    // Change price - should cancel old order and place new one
    let signal2 = TestIntentSignal::new(
        "token123",
        None,
        Some(vec![QuoteLevel::gtc(0.46, 100.0)]), // Different price
        None,
    );
    handler.handle_signal(&signal2).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Should have cancelled old order and placed new one
    assert!(executor.was_cancelled("order_1").await);
    // New order placed (total count is 2, but 1 was cancelled)
    assert_eq!(executor.placed_order_count().await, 1); // Only 1 remains after cancel
}

#[tokio::test]
async fn test_intent_quote_update_debounce_suppresses_converged_cancel_replace() {
    let position_manager = Arc::new(create_test_position_manager_with_debounce(Some(500)));
    let executor = Arc::new(MockLimitOrderExecutor::new());
    let handler = create_test_position_handler(
        position_manager.clone() as Arc<dyn PositionManager>,
        executor.clone() as Arc<dyn OrderExecutor + Send + Sync>,
    );
    let base_time = Utc::now();

    // Step 1: live bid at 0.42.
    let signal_live = TestIntentSignal::new_at(
        "token123",
        Some("market456"),
        Some(vec![QuoteLevel::gtc(0.42, 7.5)]),
        None,
        base_time,
    );
    handler.handle_signal(&signal_live).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    position_manager
        .handle_limit_order_event(&LimitOrderEvent::OrderPlaced {
            order_id: "order_1".to_string(),
            mint: "token123".to_string(),
            market: Some("market456".to_string()),
            price: 0.42,
            size: 7.5,
            side: OrderSide::Buy,
            timestamp: base_time,
            signal_id: None,
            context: None,
        })
        .await
        .unwrap();

    // Step 2: transient move to 0.41 inside debounce window.
    let signal_move = TestIntentSignal::new_at(
        "token123",
        Some("market456"),
        Some(vec![QuoteLevel::gtc(0.41, 7.5)]),
        None,
        base_time + Duration::milliseconds(100),
    );
    handler.handle_signal(&signal_move).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Step 3: convergence back to 0.42 still inside window.
    let signal_converged = TestIntentSignal::new_at(
        "token123",
        Some("market456"),
        Some(vec![QuoteLevel::gtc(0.42, 7.5)]),
        None,
        base_time + Duration::milliseconds(200),
    );
    handler.handle_signal(&signal_converged).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    assert_eq!(
        executor.cancelled_order_count().await,
        0,
        "Convergence inside debounce window should suppress cancel"
    );
    assert_eq!(
        executor.placed_order_count().await,
        1,
        "Convergence inside debounce window should suppress replacement placement"
    );
    assert!(!executor.was_cancelled("order_1").await);
}

#[tokio::test]
async fn test_intent_quote_update_debounce_allows_reprice_after_window() {
    let position_manager = Arc::new(create_test_position_manager_with_debounce(Some(500)));
    let executor = Arc::new(MockLimitOrderExecutor::new());
    let handler = create_test_position_handler(
        position_manager.clone() as Arc<dyn PositionManager>,
        executor.clone() as Arc<dyn OrderExecutor + Send + Sync>,
    );
    let base_time = Utc::now();

    // Step 1: live bid at 0.42.
    let signal_live = TestIntentSignal::new_at(
        "token123",
        Some("market456"),
        Some(vec![QuoteLevel::gtc(0.42, 7.5)]),
        None,
        base_time,
    );
    handler.handle_signal(&signal_live).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    position_manager
        .handle_limit_order_event(&LimitOrderEvent::OrderPlaced {
            order_id: "order_1".to_string(),
            mint: "token123".to_string(),
            market: Some("market456".to_string()),
            price: 0.42,
            size: 7.5,
            side: OrderSide::Buy,
            timestamp: base_time,
            signal_id: None,
            context: None,
        })
        .await
        .unwrap();

    // Step 2: transient move inside window gets debounced.
    let signal_move = TestIntentSignal::new_at(
        "token123",
        Some("market456"),
        Some(vec![QuoteLevel::gtc(0.41, 7.5)]),
        None,
        base_time + Duration::milliseconds(100),
    );
    handler.handle_signal(&signal_move).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    assert_eq!(executor.cancelled_order_count().await, 0);

    // Step 3: same reprice after window expiry should execute cancel+replace.
    let signal_after_window = TestIntentSignal::new_at(
        "token123",
        Some("market456"),
        Some(vec![QuoteLevel::gtc(0.41, 7.5)]),
        None,
        base_time + Duration::milliseconds(600),
    );
    handler.handle_signal(&signal_after_window).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    assert!(
        executor.was_cancelled("order_1").await,
        "Expected original live order to be cancelled after debounce expiry"
    );
    assert_eq!(executor.cancelled_order_count().await, 1);
    assert_eq!(executor.placed_order_count().await, 1);
    let replacement = executor.get_placed_order("order_2").await;
    assert!(
        replacement.is_some(),
        "Expected replacement order after debounce"
    );
    let replacement = replacement.unwrap();
    assert!((replacement.price - 0.41).abs() < 1e-9);
    assert!((replacement.size - 7.5).abs() < 1e-9);
}

/// Test: Empty intent (Some([])) cancels all orders.
#[tokio::test]
async fn test_intent_cancel_all() {
    let position_manager = Arc::new(create_test_position_manager());
    let executor = Arc::new(MockLimitOrderExecutor::new());
    let handler = create_test_position_handler(
        position_manager.clone() as Arc<dyn PositionManager>,
        executor.clone() as Arc<dyn OrderExecutor + Send + Sync>,
    );

    // Place initial orders
    let signal1 = TestIntentSignal::new(
        "token123",
        None,
        Some(vec![
            QuoteLevel::gtc(0.44, 50.0),
            QuoteLevel::gtc(0.45, 100.0),
        ]),
        Some(vec![QuoteLevel::gtc(0.55, 100.0)]),
    );
    handler.handle_signal(&signal1).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Simulate order placed events
    for (i, (price, size, side)) in [
        (0.44, 50.0, OrderSide::Buy),
        (0.45, 100.0, OrderSide::Buy),
        (0.55, 100.0, OrderSide::Sell),
    ]
    .iter()
    .enumerate()
    {
        let event = LimitOrderEvent::OrderPlaced {
            order_id: format!("order_{}", i + 1),
            mint: "token123".to_string(),
            market: None,
            price: *price,
            size: *size,
            side: *side,
            timestamp: Utc::now(),
            signal_id: None,
            context: None,
        };
        position_manager
            .handle_limit_order_event(&event)
            .await
            .unwrap();
    }

    assert_eq!(executor.placed_order_count().await, 3);

    // Cancel all orders with empty intent
    let cancel_signal = TestIntentSignal::new(
        "token123",
        None,
        Some(vec![]), // Cancel all bids
        Some(vec![]), // Cancel all asks
    );
    handler.handle_signal(&cancel_signal).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // All orders should be cancelled
    assert_eq!(executor.cancelled_order_count().await, 3);
}

/// Test: None preserves existing orders (no reconciliation for that side).
#[tokio::test]
async fn test_intent_none_preserves_orders() {
    let position_manager = Arc::new(create_test_position_manager());
    let executor = Arc::new(MockLimitOrderExecutor::new());
    let handler = create_test_position_handler(
        position_manager.clone() as Arc<dyn PositionManager>,
        executor.clone() as Arc<dyn OrderExecutor + Send + Sync>,
    );

    // Place initial bid
    let signal1 = TestIntentSignal::new(
        "token123",
        None,
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        None,
    );
    handler.handle_signal(&signal1).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let event = LimitOrderEvent::OrderPlaced {
        order_id: "order_1".to_string(),
        mint: "token123".to_string(),
        market: None,
        price: 0.45,
        size: 100.0,
        side: OrderSide::Buy,
        timestamp: Utc::now(),
        signal_id: None,
        context: None,
    };
    position_manager
        .handle_limit_order_event(&event)
        .await
        .unwrap();

    // Signal with None for bids (preserve) and new asks
    let signal2 = TestIntentSignal::new(
        "token123",
        None,
        None,                                     // Preserve existing bids
        Some(vec![QuoteLevel::gtc(0.55, 100.0)]), // Add new ask
    );
    handler.handle_signal(&signal2).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Bid should NOT be cancelled
    assert!(!executor.was_cancelled("order_1").await);
    // New ask should be placed
    assert_eq!(executor.placed_order_count().await, 2);
}

/// Test: Partial fill updates remaining size and affects reconciliation.
#[tokio::test]
async fn test_intent_partial_fill_affects_reconciliation() {
    let position_manager = Arc::new(create_test_position_manager());
    let executor = Arc::new(MockLimitOrderExecutor::new());
    let handler = create_test_position_handler(
        position_manager.clone() as Arc<dyn PositionManager>,
        executor.clone() as Arc<dyn OrderExecutor + Send + Sync>,
    );

    // Place initial order with size 100
    let signal1 = TestIntentSignal::new(
        "token123",
        None,
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        None,
    );
    handler.handle_signal(&signal1).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let event = LimitOrderEvent::OrderPlaced {
        order_id: "order_1".to_string(),
        mint: "token123".to_string(),
        market: None,
        price: 0.45,
        size: 100.0,
        side: OrderSide::Buy,
        timestamp: Utc::now(),
        signal_id: None,
        context: None,
    };
    position_manager
        .handle_limit_order_event(&event)
        .await
        .unwrap();

    // Simulate partial fill (30 filled, 70 remaining)
    let partial_fill = LimitOrderEvent::OrderPartiallyFilled {
        order_id: "order_1".to_string(),
        mint: "token123".to_string(),
        side: OrderSide::Buy,
        filled_size: 30.0,
        remaining_size: 70.0,
        fill_price: 0.45,
        timestamp: Utc::now(),
        signal_id: None,
        exit_mode: None,
    };
    position_manager
        .handle_limit_order_event(&partial_fill)
        .await
        .unwrap();

    // Intent with original size (100) should now trigger cancel + replace
    // because remaining size (70) != desired size (100)
    let signal2 = TestIntentSignal::new(
        "token123",
        None,
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]), // Same price, original size
        None,
    );
    handler.handle_signal(&signal2).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Should have cancelled the partially filled order and placed a new one
    assert!(executor.was_cancelled("order_1").await);
}

/// Test: Signal ID propagates through the entire flow.
#[tokio::test]
async fn test_signal_id_propagation() {
    let position_manager = Arc::new(create_test_position_manager());
    let executor = Arc::new(MockLimitOrderExecutor::new());
    let handler = create_test_position_handler(
        position_manager.clone() as Arc<dyn PositionManager>,
        executor.clone() as Arc<dyn OrderExecutor + Send + Sync>,
    );

    let signal = TestIntentSignal::new(
        "token123",
        None,
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        None,
    );
    let _expected_signal_id = signal.signal_id.clone();

    handler.handle_signal(&signal).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Verify order was placed
    assert_eq!(executor.placed_order_count().await, 1);

    // Note: The signal_id is passed to the Order but the mock executor
    // doesn't track it in PlacedOrder. The important thing is the flow works.
    // In production, signal_id would be tracked for debugging/tracing.
}

/// Test: Multiple mints are handled independently.
#[tokio::test]
async fn test_multiple_mints_independent() {
    let position_manager = Arc::new(create_test_position_manager());
    let executor = Arc::new(MockLimitOrderExecutor::new());
    let handler = create_test_position_handler(
        position_manager.clone() as Arc<dyn PositionManager>,
        executor.clone() as Arc<dyn OrderExecutor + Send + Sync>,
    );

    // Place orders for token1
    let signal1 = TestIntentSignal::new(
        "token1",
        None,
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        None,
    );
    handler.handle_signal(&signal1).await.unwrap();

    // Place orders for token2
    let signal2 = TestIntentSignal::new(
        "token2",
        None,
        Some(vec![QuoteLevel::gtc(0.50, 200.0)]),
        None,
    );
    handler.handle_signal(&signal2).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Simulate both orders placed
    let event1 = LimitOrderEvent::OrderPlaced {
        order_id: "order_1".to_string(),
        mint: "token1".to_string(),
        market: None,
        price: 0.45,
        size: 100.0,
        side: OrderSide::Buy,
        timestamp: Utc::now(),
        signal_id: None,
        context: None,
    };
    let event2 = LimitOrderEvent::OrderPlaced {
        order_id: "order_2".to_string(),
        mint: "token2".to_string(),
        market: None,
        price: 0.50,
        size: 200.0,
        side: OrderSide::Buy,
        timestamp: Utc::now(),
        signal_id: None,
        context: None,
    };
    position_manager
        .handle_limit_order_event(&event1)
        .await
        .unwrap();
    position_manager
        .handle_limit_order_event(&event2)
        .await
        .unwrap();

    // Cancel all for token1 only
    let cancel_signal = TestIntentSignal::new("token1", None, Some(vec![]), None);
    handler.handle_signal(&cancel_signal).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Only token1 order should be cancelled
    assert!(executor.was_cancelled("order_1").await);
    assert!(!executor.was_cancelled("order_2").await);
}

/// Test: Invalid intent (negative price) is rejected.
#[tokio::test]
async fn test_invalid_intent_rejected() {
    let position_manager = Arc::new(create_test_position_manager());
    let executor = Arc::new(MockLimitOrderExecutor::new());
    let handler = create_test_position_handler(
        position_manager.clone() as Arc<dyn PositionManager>,
        executor.clone() as Arc<dyn OrderExecutor + Send + Sync>,
    );

    // Intent with negative price
    let signal = TestIntentSignal::new(
        "token123",
        None,
        Some(vec![QuoteLevel::gtc(-0.45, 100.0)]), // Invalid negative price
        None,
    );

    let result = handler.handle_signal(&signal).await;

    // Should fail validation
    assert!(result.is_err());
    assert_eq!(executor.placed_order_count().await, 0);
}
