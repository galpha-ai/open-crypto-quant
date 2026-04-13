//! Tests for intent-based order reconciliation.
//!
//! These tests verify the reconciliation algorithm that compares desired order
//! state (from `OrderIntent`) against current pending orders and generates
//! the necessary actions (cancels and placements).

use chrono::Utc;
use std::sync::Arc;

use crate::execution::LimitOrderEvent;
use crate::execution::{OrderSide, OrderType};
use crate::position::InMemoryPositionManager;
use crate::position::PositionManager;
use crate::position::errors::PositionError;
use crate::position::exit_strategy::ConfigurableExitStrategy;
use crate::position::pending_order::PendingLimitOrder;
use crate::signal::{OrderIntent, QuoteLevel};

fn create_test_manager() -> InMemoryPositionManager {
    let registry = prometheus::Registry::new();
    let exit_strategy = Arc::new(ConfigurableExitStrategy::new(
        0.5,                          // take_profit_threshold
        0.2,                          // stop_loss_threshold
        chrono::Duration::minutes(5), // max_holding_period
        3,                            // max_sell_failures
    ));

    InMemoryPositionManager::new(
        0.1,     // trade_amount_sol
        10000.0, // initial_sol - large enough for test buy orders
        chrono::Duration::minutes(5),
        5, // max_open_positions
        &registry,
        exit_strategy,
    )
}

fn create_test_manager_with_debounce(
    min_quote_lifetime_ms: Option<u64>,
) -> InMemoryPositionManager {
    let registry = prometheus::Registry::new();
    let exit_strategy = Arc::new(ConfigurableExitStrategy::new(
        0.5,                          // take_profit_threshold
        0.2,                          // stop_loss_threshold
        chrono::Duration::minutes(5), // max_holding_period
        3,                            // max_sell_failures
    ));

    InMemoryPositionManager::new_with_quote_lifetime(
        0.1,     // trade_amount_sol
        10000.0, // initial_sol - large enough for test buy orders
        chrono::Duration::minutes(5),
        5, // max_open_positions
        min_quote_lifetime_ms,
        &registry,
        exit_strategy,
    )
}

/// Add a pending order directly to the manager state for testing.
fn add_pending_order(
    manager: &InMemoryPositionManager,
    order_id: &str,
    mint: &str,
    side: OrderSide,
    price: f64,
    size: f64,
) {
    let timestamp = Utc::now();
    let pending_order = PendingLimitOrder::new(
        order_id.to_string(),
        mint.to_string(),
        Some("market1".to_string()),
        side,
        price,
        size,
        timestamp,
        None,
    );

    manager.add_pending_order_for_testing(mint, order_id, pending_order);
}

// === Basic Reconciliation Tests ===

#[tokio::test]
async fn test_empty_current_state_generates_placements() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // Intent with one bid and one ask
    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        Some(vec![QuoteLevel::gtc(0.55, 100.0)]),
        "signal1".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    // Should generate 2 placement orders (no cancels)
    assert_eq!(orders.len(), 2);
    assert!(orders.iter().all(|o| o.order_type.is_limit()));
    assert!(orders.iter().any(|o| matches!(&o.order_type, OrderType::LimitBuy { limit_price, .. } if (*limit_price - 0.45).abs() < 1e-9)));
    assert!(orders.iter().any(|o| matches!(&o.order_type, OrderType::LimitSell { limit_price, .. } if (*limit_price - 0.55).abs() < 1e-9)));
}

#[tokio::test]
async fn test_matching_state_generates_no_orders() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // Add existing orders that match the intent
    add_pending_order(&manager, "bid1", "token123", OrderSide::Buy, 0.45, 100.0);
    add_pending_order(&manager, "ask1", "token123", OrderSide::Sell, 0.55, 100.0);

    // Intent matches current state exactly
    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        Some(vec![QuoteLevel::gtc(0.55, 100.0)]),
        "signal1".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    // Should generate no orders (state matches intent)
    assert!(orders.is_empty());
}

#[tokio::test]
async fn test_empty_bids_cancels_all_bids() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // Add existing bid orders
    add_pending_order(&manager, "bid1", "token123", OrderSide::Buy, 0.45, 100.0);
    add_pending_order(&manager, "bid2", "token123", OrderSide::Buy, 0.44, 50.0);

    // Intent with Some([]) to cancel all bids
    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![]), // Cancel all bids
        None,         // Preserve asks (no asks exist anyway)
        "signal1".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    // Should generate 2 cancel orders for bids
    assert_eq!(orders.len(), 2);
    assert!(orders.iter().all(|o| o.order_type.is_cancel()));
}

#[tokio::test]
async fn test_none_preserves_existing_orders() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // Add existing bid orders
    add_pending_order(&manager, "bid1", "token123", OrderSide::Buy, 0.45, 100.0);

    // Intent with None for bids (preserve) and Some([]) for asks (clear)
    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        None,         // Preserve existing bids
        Some(vec![]), // Cancel all asks (none exist)
        "signal1".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    // Should generate no orders (bids preserved, no asks to cancel)
    assert!(orders.is_empty());
}

#[tokio::test]
async fn test_price_change_generates_cancel_and_placement() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // Add existing bid at old price
    add_pending_order(&manager, "bid1", "token123", OrderSide::Buy, 0.45, 100.0);

    // Intent wants bid at new price
    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![QuoteLevel::gtc(0.46, 100.0)]), // Different price
        None,
        "signal1".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    // Should generate 1 cancel + 1 placement
    assert_eq!(orders.len(), 2);

    // Verify cancel comes first
    assert!(orders[0].order_type.is_cancel());
    assert_eq!(orders[0].order_type.cancel_order_id(), Some("bid1"));

    // Verify placement comes second
    assert!(orders[1].order_type.is_limit());
    assert!(
        matches!(&orders[1].order_type, OrderType::LimitBuy { limit_price, .. } if (*limit_price - 0.46).abs() < 1e-9)
    );
}

#[tokio::test]
async fn test_quote_debounce_coalesces_updates_until_expiry() {
    let manager = create_test_manager_with_debounce(Some(500));
    let base_time = Utc::now();

    let pending_order = PendingLimitOrder::new(
        "bid1".to_string(),
        "token123".to_string(),
        Some("market1".to_string()),
        OrderSide::Buy,
        0.45,
        100.0,
        base_time,
        None,
    );
    manager.add_pending_order_for_testing("token123", "bid1", pending_order);

    let intent1 = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![QuoteLevel::gtc(0.46, 100.0)]),
        None,
        "signal1".to_string(),
        base_time + chrono::Duration::milliseconds(100),
        None,
    );
    let orders1 = manager.reconcile_intent(&intent1).await.unwrap();
    assert!(orders1.is_empty());

    let intent2 = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![QuoteLevel::gtc(0.47, 100.0)]),
        None,
        "signal2".to_string(),
        base_time + chrono::Duration::milliseconds(200),
        None,
    );
    let orders2 = manager.reconcile_intent(&intent2).await.unwrap();
    assert!(orders2.is_empty());

    let intent3 = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        None,
        None,
        "signal3".to_string(),
        base_time + chrono::Duration::milliseconds(600),
        None,
    );
    let orders3 = manager.reconcile_intent(&intent3).await.unwrap();
    assert_eq!(orders3.len(), 2);
    assert!(orders3[0].order_type.is_cancel());
    assert!(
        matches!(&orders3[1].order_type, OrderType::LimitBuy { limit_price, .. } if (*limit_price - 0.47).abs() < 1e-9)
    );
}

#[tokio::test]
async fn test_quote_debounce_bypasses_cancel_all() {
    let manager = create_test_manager_with_debounce(Some(500));
    let base_time = Utc::now();

    let pending_order = PendingLimitOrder::new(
        "bid1".to_string(),
        "token123".to_string(),
        Some("market1".to_string()),
        OrderSide::Buy,
        0.45,
        100.0,
        base_time,
        None,
    );
    manager.add_pending_order_for_testing("token123", "bid1", pending_order);

    let cancel_intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![]),
        None,
        "signal-cancel".to_string(),
        base_time + chrono::Duration::milliseconds(100),
        None,
    );
    let orders = manager.reconcile_intent(&cancel_intent).await.unwrap();
    assert_eq!(orders.len(), 1);
    assert!(orders[0].order_type.is_cancel());
    assert_eq!(orders[0].order_type.cancel_order_id(), Some("bid1"));
}

#[tokio::test]
async fn test_latency_config_without_quote_lifetime_preserves_legacy_behavior() {
    let manager = create_test_manager_with_debounce(None);
    let base_time = Utc::now();

    let pending_order = PendingLimitOrder::new(
        "bid1".to_string(),
        "token123".to_string(),
        Some("market1".to_string()),
        OrderSide::Buy,
        0.45,
        100.0,
        base_time,
        None,
    );
    manager.add_pending_order_for_testing("token123", "bid1", pending_order);

    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![QuoteLevel::gtc(0.46, 100.0)]),
        None,
        "signal1".to_string(),
        base_time + chrono::Duration::milliseconds(100),
        None,
    );
    let orders = manager.reconcile_intent(&intent).await.unwrap();
    assert_eq!(orders.len(), 2);
    assert!(orders[0].order_type.is_cancel());
    assert!(
        matches!(&orders[1].order_type, OrderType::LimitBuy { limit_price, .. } if (*limit_price - 0.46).abs() < 1e-9)
    );
}

// === Order Lifecycle Tests ===

#[tokio::test]
async fn test_order_placed_adds_to_pending() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    let event = LimitOrderEvent::OrderPlaced {
        order_id: "order123".to_string(),
        mint: "token123".to_string(),
        market: Some("market1".to_string()),
        price: 0.45,
        size: 100.0,
        side: OrderSide::Buy,
        timestamp,
        signal_id: None,
        context: None,
    };

    manager.handle_limit_order_event(&event).await.unwrap();

    // Verify order was added
    let orders = manager.get_pending_orders_for_mint("token123");
    assert_eq!(orders.len(), 1);
    assert!(orders.contains_key("order123"));
    assert_eq!(orders["order123"].price, 0.45);
    assert_eq!(orders["order123"].remaining_size, 100.0);
}

#[tokio::test]
async fn test_partial_fill_updates_remaining_size() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // First place the order
    add_pending_order(
        &manager,
        "order123",
        "token123",
        OrderSide::Buy,
        0.45,
        100.0,
    );

    // Then simulate partial fill
    let event = LimitOrderEvent::OrderPartiallyFilled {
        order_id: "order123".to_string(),
        mint: "token123".to_string(),
        side: OrderSide::Buy,
        filled_size: 30.0,
        remaining_size: 70.0,
        fill_price: 0.45,
        timestamp,
        signal_id: None,
        exit_mode: None,
    };

    manager.handle_limit_order_event(&event).await.unwrap();

    // Verify remaining size was updated
    let orders = manager.get_pending_orders_for_mint("token123");
    assert_eq!(orders["order123"].remaining_size, 70.0);
    assert!(orders["order123"].is_partially_filled());
}

#[tokio::test]
async fn test_order_cancelled_removes_from_pending() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // Add order
    add_pending_order(
        &manager,
        "order123",
        "token123",
        OrderSide::Buy,
        0.45,
        100.0,
    );

    // Cancel it
    let event = LimitOrderEvent::OrderCancelled {
        order_id: "order123".to_string(),
        reason: Some("user requested".to_string()),
        timestamp,
    };

    manager.handle_limit_order_event(&event).await.unwrap();

    // Verify order was removed
    let orders = manager.get_pending_orders_for_mint("token123");
    assert!(orders.is_empty());
}

#[tokio::test]
async fn test_cancel_acknowledgement_keeps_order_pending() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    add_pending_order(
        &manager,
        "order123",
        "token123",
        OrderSide::Buy,
        0.45,
        100.0,
    );

    let event = LimitOrderEvent::OrderCancelled {
        order_id: "order123".to_string(),
        reason: Some("Cancel requested (awaiting confirmation)".to_string()),
        timestamp,
    };

    manager.handle_limit_order_event(&event).await.unwrap();

    let orders = manager.get_pending_orders_for_mint("token123");
    assert_eq!(orders.len(), 1);
    assert!(orders.contains_key("order123"));
}

#[tokio::test]
async fn test_order_expired_removes_from_pending() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // Add order
    add_pending_order(
        &manager,
        "order123",
        "token123",
        OrderSide::Buy,
        0.45,
        100.0,
    );

    // Expire it
    let event = LimitOrderEvent::OrderExpired {
        order_id: "order123".to_string(),
        timestamp,
    };

    manager.handle_limit_order_event(&event).await.unwrap();

    // Verify order was removed
    let orders = manager.get_pending_orders_for_mint("token123");
    assert!(orders.is_empty());
}

#[tokio::test]
async fn test_order_rejected_does_not_add() {
    let manager = create_test_manager();

    // Order rejected event (order was never pending)
    let event = LimitOrderEvent::OrderRejected {
        reason: "insufficient funds".to_string(),
    };

    manager.handle_limit_order_event(&event).await.unwrap();

    // Nothing to verify - just ensure it doesn't panic
}

// === Validation Tests ===

#[tokio::test]
async fn test_invalid_negative_price_rejected() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    let intent = OrderIntent::new(
        "token123".to_string(),
        None,
        Some(vec![QuoteLevel::gtc(-0.45, 100.0)]), // Invalid negative price
        None,
        "signal1".to_string(),
        timestamp,
        None,
    );

    let result = manager.reconcile_intent(&intent).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        PositionError::InvalidIntent(_)
    ));
}

#[tokio::test]
async fn test_invalid_zero_size_rejected() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    let intent = OrderIntent::new(
        "token123".to_string(),
        None,
        Some(vec![QuoteLevel::gtc(0.45, 0.0)]), // Invalid zero size
        None,
        "signal1".to_string(),
        timestamp,
        None,
    );

    let result = manager.reconcile_intent(&intent).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        PositionError::InvalidIntent(_)
    ));
}

// === Integration Tests ===

#[tokio::test]
async fn test_signal_id_propagates_to_orders() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        None,
        "my_signal_123".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].signal_id, Some("my_signal_123".to_string()));
}

#[tokio::test]
async fn test_cancel_orders_come_before_placements() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // Add an order at old price
    add_pending_order(&manager, "old_bid", "token123", OrderSide::Buy, 0.40, 100.0);

    // Intent wants a new bid at different price
    let intent = OrderIntent::new(
        "token123".to_string(),
        None,
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        None,
        "signal1".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    assert_eq!(orders.len(), 2);
    // First order should be cancel
    assert!(orders[0].order_type.is_cancel());
    // Second should be placement
    assert!(orders[1].order_type.is_limit());
}

#[tokio::test]
async fn test_multiple_levels_reconciliation() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // Add some existing orders
    add_pending_order(&manager, "bid1", "token123", OrderSide::Buy, 0.44, 100.0); // Keep
    add_pending_order(&manager, "bid2", "token123", OrderSide::Buy, 0.43, 50.0); // Cancel
    add_pending_order(&manager, "ask1", "token123", OrderSide::Sell, 0.56, 100.0); // Cancel

    // Intent: keep bid1, add new ask at 0.55
    let intent = OrderIntent::new(
        "token123".to_string(),
        None,
        Some(vec![
            QuoteLevel::gtc(0.44, 100.0), // Keep existing
            QuoteLevel::gtc(0.45, 50.0),  // New
        ]),
        Some(vec![QuoteLevel::gtc(0.55, 100.0)]), // New
        "signal1".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    // Expect: cancel bid2 (0.43), cancel ask1 (0.56), place bid (0.45), place ask (0.55)
    assert_eq!(orders.len(), 4);

    // Cancels should come first
    let cancel_count = orders
        .iter()
        .take_while(|o| o.order_type.is_cancel())
        .count();
    assert_eq!(cancel_count, 2);

    // Verify specific cancels
    let cancelled_ids: Vec<_> = orders
        .iter()
        .filter(|o| o.order_type.is_cancel())
        .filter_map(|o| o.order_type.cancel_order_id())
        .collect();
    assert!(cancelled_ids.contains(&"bid2"));
    assert!(cancelled_ids.contains(&"ask1"));
}

#[tokio::test]
async fn test_idempotent_reconciliation() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        Some(vec![QuoteLevel::gtc(0.55, 100.0)]),
        "signal1".to_string(),
        timestamp,
        None,
    );

    // First reconciliation - generates placements
    let orders1 = manager.reconcile_intent(&intent).await.unwrap();
    assert_eq!(orders1.len(), 2);

    // Simulate orders being placed by adding them to pending state
    for order in &orders1 {
        if let OrderType::LimitBuy {
            limit_price,
            quote_amount,
            ..
        } = &order.order_type
        {
            let size = quote_amount / limit_price;
            add_pending_order(
                &manager,
                "bid_placed",
                &order.mint,
                OrderSide::Buy,
                *limit_price,
                size,
            );
        }
        if let OrderType::LimitSell {
            limit_price,
            token_amount,
            ..
        } = &order.order_type
        {
            add_pending_order(
                &manager,
                "ask_placed",
                &order.mint,
                OrderSide::Sell,
                *limit_price,
                *token_amount,
            );
        }
    }

    // Second reconciliation with same intent - should generate no orders
    let orders2 = manager.reconcile_intent(&intent).await.unwrap();
    assert!(
        orders2.is_empty(),
        "Idempotent reconciliation should generate no orders"
    );
}

// === Quote Balance Constraint Tests (Layer 1) ===

fn create_low_balance_manager() -> InMemoryPositionManager {
    let registry = prometheus::Registry::new();
    let exit_strategy = Arc::new(ConfigurableExitStrategy::new(
        0.5,                          // take_profit_threshold
        0.2,                          // stop_loss_threshold
        chrono::Duration::minutes(5), // max_holding_period
        3,                            // max_sell_failures
    ));

    InMemoryPositionManager::new(
        0.1,  // trade_amount_sol
        50.0, // initial_sol - low balance for testing
        chrono::Duration::minutes(5),
        5, // max_open_positions
        &registry,
        exit_strategy,
    )
}

#[tokio::test]
async fn test_buy_orders_filtered_when_quote_insufficient() {
    let manager = create_low_balance_manager(); // 50.0 available quote
    let timestamp = Utc::now();

    // Intent with one large bid (needs 100 quote) and one small bid (needs 25 quote)
    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![
            QuoteLevel::gtc(0.50, 50.0),  // 25 quote needed (50 units * 0.50 price)
            QuoteLevel::gtc(0.40, 250.0), // 100 quote needed (250 units * 0.40 price)
        ]),
        None,
        "signal1".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    // With 50 quote available:
    // - First bid (25 quote) should be allowed, leaving 25 quote
    // - Second bid (100 quote) should be filtered out since 100 > 25
    assert_eq!(
        orders.len(),
        1,
        "Only the affordable buy order should be generated"
    );

    // Verify it's the smaller order
    match &orders[0].order_type {
        OrderType::LimitBuy { quote_amount, .. } => {
            assert!(
                (quote_amount - 25.0).abs() < 0.001,
                "Should be the 25 quote order"
            );
        }
        _ => panic!("Expected LimitBuy order"),
    }
}

#[tokio::test]
async fn test_sell_orders_not_affected_by_quote_balance() {
    let manager = create_low_balance_manager(); // 50.0 available quote
    let timestamp = Utc::now();

    // Intent with large sell orders (should not be affected by quote balance)
    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        None,
        Some(vec![
            QuoteLevel::gtc(0.55, 1000.0), // Large sell - unaffected by quote
            QuoteLevel::gtc(0.60, 500.0),  // Another large sell
        ]),
        "signal1".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    // Both sell orders should be generated regardless of quote balance
    assert_eq!(
        orders.len(),
        2,
        "Sell orders should not be filtered by quote balance"
    );

    // Verify both are sell orders
    assert!(
        orders
            .iter()
            .all(|o| matches!(o.order_type, OrderType::LimitSell { .. }))
    );
}

#[tokio::test]
async fn test_cancel_orders_not_affected_by_quote_balance() {
    let manager = create_low_balance_manager(); // 50.0 available quote
    let timestamp = Utc::now();

    // Add existing orders
    add_pending_order(&manager, "bid1", "token123", OrderSide::Buy, 0.45, 1000.0);
    add_pending_order(&manager, "bid2", "token123", OrderSide::Buy, 0.44, 500.0);

    // Intent to cancel all bids
    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![]), // Cancel all bids
        None,
        "signal1".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    // Both cancel orders should be generated regardless of quote balance
    assert_eq!(orders.len(), 2, "Cancel orders should not be filtered");
    assert!(orders.iter().all(|o| o.order_type.is_cancel()));
}

// === In-Flight Order Tracking Tests ===
//
// These tests verify that the reconciliation engine considers in-flight orders
// to prevent duplicate order placement during the gap between order submission
// and OrderPlaced event processing.

use crate::position::pending_order::InFlightOrder;

/// Test: In-flight orders are considered during reconciliation, preventing duplicates.
///
/// This verifies the fix for the race condition where multiple intents could
/// arrive before the first order is recorded in pending_limit_orders.
#[tokio::test]
async fn test_in_flight_order_prevents_duplicate_placement() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // Intent with one bid
    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        None,
        "signal1".to_string(),
        timestamp,
        None,
    );

    // Register an in-flight order matching the intent BEFORE reconciliation
    let in_flight = InFlightOrder::new(
        "flight1".to_string(),
        "token123".to_string(),
        Some("market1".to_string()),
        OrderSide::Buy,
        0.45,
        100.0,
        timestamp,
        Some("signal1".to_string()),
    );
    manager.register_in_flight_order("flight1".to_string(), in_flight);

    // Reconciliation should see the in-flight order and NOT generate a duplicate
    let orders = manager.reconcile_intent(&intent).await.unwrap();
    assert!(
        orders.is_empty(),
        "No orders should be generated when in-flight order matches intent"
    );
}

/// Test: In-flight orders with different parameters don't block new placements.
#[tokio::test]
async fn test_in_flight_order_does_not_block_different_price() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // Register an in-flight order at one price
    let in_flight = InFlightOrder::new(
        "flight1".to_string(),
        "token123".to_string(),
        Some("market1".to_string()),
        OrderSide::Buy,
        0.44, // Different price
        100.0,
        timestamp,
        None,
    );
    manager.register_in_flight_order("flight1".to_string(), in_flight);

    // Intent wants a bid at a different price
    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]), // Different price
        None,
        "signal1".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    // Should generate a placement for the new price
    // Note: We don't generate a cancel for in-flight orders (they can't be cancelled yet)
    assert_eq!(
        orders.len(),
        1,
        "Should generate placement for new price level"
    );
    assert!(orders[0].order_type.is_limit());
}

/// Test: Cancel orders for in-flight orders are filtered out.
///
/// In-flight orders can't be cancelled because they haven't been confirmed
/// by the venue yet. The reconciliation engine should filter out such cancels.
#[tokio::test]
async fn test_in_flight_order_cancel_is_filtered() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // Register an in-flight order
    let in_flight = InFlightOrder::new(
        "flight1".to_string(),
        "token123".to_string(),
        Some("market1".to_string()),
        OrderSide::Buy,
        0.45,
        100.0,
        timestamp,
        None,
    );
    manager.register_in_flight_order("flight1".to_string(), in_flight);

    // Intent wants NO bids (empty list means cancel all)
    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![]), // Cancel all bids
        None,
        "signal1".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    // Should NOT generate a cancel for the in-flight order
    // (you can't cancel an order that hasn't been confirmed yet)
    assert!(
        orders.is_empty(),
        "Cancel for in-flight order should be filtered out"
    );
}

/// Test: In-flight order is removed when matching OrderPlaced arrives.
#[tokio::test]
async fn test_in_flight_order_transitions_to_pending_on_order_placed() {
    let manager = create_test_manager();
    let timestamp = Utc::now();

    // Register an in-flight order
    let in_flight = InFlightOrder::new(
        "flight1".to_string(),
        "token123".to_string(),
        Some("market1".to_string()),
        OrderSide::Buy,
        0.45,
        100.0,
        timestamp,
        None,
    );
    manager.register_in_flight_order("flight1".to_string(), in_flight);

    // Simulate OrderPlaced event arriving (handled by handle_limit_order_event)
    let placed_event = LimitOrderEvent::OrderPlaced {
        order_id: "venue_order_1".to_string(),
        mint: "token123".to_string(),
        market: Some("market1".to_string()),
        price: 0.45,
        size: 100.0,
        side: OrderSide::Buy,
        timestamp,
        signal_id: None,
        context: None,
    };
    manager
        .handle_limit_order_event(&placed_event)
        .await
        .unwrap();

    // Intent with same price - should now match the PENDING order (not in-flight)
    let intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
        None,
        "signal2".to_string(),
        timestamp,
        None,
    );

    let orders = manager.reconcile_intent(&intent).await.unwrap();

    // Should generate no orders because pending order matches
    assert!(
        orders.is_empty(),
        "No orders needed - pending order matches intent"
    );

    // Now cancel all bids - should cancel the PENDING order (not in-flight anymore)
    let cancel_intent = OrderIntent::new(
        "token123".to_string(),
        Some("market1".to_string()),
        Some(vec![]), // Cancel all
        None,
        "signal3".to_string(),
        timestamp,
        None,
    );

    let cancel_orders = manager.reconcile_intent(&cancel_intent).await.unwrap();

    // Should generate cancel for the pending order
    assert_eq!(
        cancel_orders.len(),
        1,
        "Should generate cancel for pending order"
    );
    assert!(cancel_orders[0].order_type.is_cancel());
    assert_eq!(
        cancel_orders[0].order_type.cancel_order_id(),
        Some("venue_order_1")
    );
}
