use std::collections::HashMap;

use chrono::Utc;
use popeyes_trading_types::{PolymarketTradeEvent, TradeSide};

use super::{PaperTradingConfig, *};
use crate::execution::{
    ExecutionEvent, Order, OrderStatus, OrderType,
    events::{LimitOrderEvent, OrderSide, RedemptionEvent, TimeInForce},
    executor::OrderExecutor,
};
use crate::signal::RedemptionAction;

/// Helper to create a PolymarketTradeEvent for testing.
/// Uses a timestamp 1 second in the future to ensure it's always after order placement.
fn create_polymarket_trade(
    asset_id: &str,
    side: TradeSide,
    price: f64,
    size: f64,
) -> PolymarketTradeEvent {
    PolymarketTradeEvent {
        asset_id: asset_id.to_string(),
        market: "test-market".to_string(),
        price,
        size,
        side,
        // Add 1 second buffer to ensure trade timestamp is after any order's eligible_for_fills_at
        // when no latency config is set (order eligible immediately at placement time)
        timestamp: (Utc::now() + chrono::Duration::seconds(1)).timestamp_millis(),
        fee_rate_bps: 0,
        market_metadata: None,
        observed_at: Utc::now(),
    }
}

// ============================================================================
// Basic Functionality Tests
// ============================================================================

#[tokio::test]
async fn test_new_creates_executor_with_defaults() {
    let executor = PaperTradingOrderExecutor::new();
    assert!(executor.enforces_inventory_constraints());
    assert_eq!(executor.pending_order_count().await, 0);
}

#[tokio::test]
async fn test_with_config_allows_custom_settings() {
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        enforce_inventory_constraints: false,
        ..Default::default()
    });
    assert!(!executor.enforces_inventory_constraints());
}

#[tokio::test]
async fn test_default_impl() {
    let executor = PaperTradingOrderExecutor::default();
    assert!(executor.enforces_inventory_constraints());
}

#[tokio::test]
async fn test_market_order_rejected() {
    let executor = PaperTradingOrderExecutor::new();

    #[allow(deprecated)]
    let order = Order {
        mint: "TOKEN1".to_string(),
        market: None,
        order_type: OrderType::Buy { sol_amount: 1.0 },
        price: Some(0.1),
        status: OrderStatus::Pending,
        timestamp: Utc::now(),
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };

    let result = executor.execute_market_order(order).await.unwrap();
    match result {
        ExecutionEvent::OrderRejected { reason, .. } => {
            assert!(reason.contains("only supports limit orders"));
        }
        _ => panic!("Expected OrderRejected event"),
    }
}

#[tokio::test]
async fn test_supports_limit_orders_returns_true() {
    let executor = PaperTradingOrderExecutor::new();
    assert!(executor.supports_limit_orders());
}

// ============================================================================
// Limit Order Placement Tests
// ============================================================================

#[tokio::test]
async fn test_execute_limit_buy_order_placed() {
    let executor = PaperTradingOrderExecutor::new();

    let order = Order::new_limit_buy(
        "asset123".to_string(),
        Some("market456".to_string()),
        100.0, // quote_amount
        0.50,  // limit_price
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        Some("signal789".to_string()),
        Some(12345),
    );

    let result = executor.execute_limit_order(order).await.unwrap();

    match result {
        LimitOrderEvent::OrderPlaced {
            order_id,
            mint,
            market,
            price,
            size,
            side,
            ..
        } => {
            assert!(order_id.starts_with("paper-order-"));
            assert_eq!(mint, "asset123");
            assert_eq!(market, Some("market456".to_string()));
            assert_eq!(price, 0.50);
            // Size is converted from quote to base: 100.0 quote / 0.50 price = 200.0 base units
            assert_eq!(size, 200.0);
            assert_eq!(side, OrderSide::Buy);
        }
        _ => panic!("Expected OrderPlaced event, got {:?}", result),
    }

    // Verify order is in pending orders
    let pending = executor.get_pending_orders().await;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].mint, "asset123");
    assert_eq!(pending[0].side, OrderSide::Buy);
}

#[tokio::test]
async fn test_execute_limit_sell_order_placed() {
    let executor = PaperTradingOrderExecutor::new();

    let order = Order::new_limit_sell(
        "asset123".to_string(),
        None,
        50.0,  // token_amount
        0.75,  // limit_price
        false, // clear_position
        TimeInForce::ImmediateOrCancel,
        Utc::now(),
        None,
        None,
    );

    let result = executor.execute_limit_order(order).await.unwrap();

    match result {
        LimitOrderEvent::OrderPlaced {
            price, size, side, ..
        } => {
            assert_eq!(price, 0.75);
            assert_eq!(size, 50.0);
            assert_eq!(side, OrderSide::Sell);
        }
        _ => panic!("Expected OrderPlaced event"),
    }
}

#[tokio::test]
async fn test_execute_limit_order_with_non_limit_order_type_fails() {
    let executor = PaperTradingOrderExecutor::new();

    #[allow(deprecated)]
    let order = Order {
        mint: "asset123".to_string(),
        market: None,
        order_type: OrderType::Buy { sol_amount: 1.0 },
        price: Some(0.5),
        status: OrderStatus::Pending,
        timestamp: Utc::now(),
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };

    let result = executor.execute_limit_order(order).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("non-limit order type")
    );
}

#[tokio::test]
async fn test_order_id_generation_is_unique() {
    let executor = PaperTradingOrderExecutor::new();

    let order1 = Order::new_limit_buy(
        "asset1".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    let order2 = Order::new_limit_buy(
        "asset2".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );

    let placed1 = executor.execute_limit_order(order1).await.unwrap();
    let placed2 = executor.execute_limit_order(order2).await.unwrap();

    let id1 = placed1.order_id().unwrap();
    let id2 = placed2.order_id().unwrap();

    assert_ne!(id1, id2);
    assert!(id1.starts_with("paper-order-"));
    assert!(id2.starts_with("paper-order-"));
}

// ============================================================================
// Cancel Order Tests
// ============================================================================

#[tokio::test]
async fn test_cancel_removes_order_and_returns_event() {
    let executor = PaperTradingOrderExecutor::new();

    // Place an order
    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();

    // Cancel it
    let cancelled = executor.cancel_order(&order_id).await.unwrap();

    match cancelled {
        LimitOrderEvent::OrderCancelled {
            order_id: cancelled_id,
            reason,
            ..
        } => {
            assert_eq!(cancelled_id, order_id);
            assert_eq!(reason, Some("User cancelled".to_string()));
        }
        _ => panic!("Expected OrderCancelled event"),
    }

    // Order should be removed
    assert!(executor.get_pending_orders().await.is_empty());
}

#[tokio::test]
async fn test_cancel_nonexistent_order_returns_success() {
    let executor = PaperTradingOrderExecutor::new();

    let result = executor.cancel_order("nonexistent-order-123").await;

    // Canceling a nonexistent order returns success since the goal
    // (order no longer active) is achieved - likely already filled or cancelled
    assert!(result.is_ok());
    match result.unwrap() {
        LimitOrderEvent::OrderCancelled {
            order_id, reason, ..
        } => {
            assert_eq!(order_id, "nonexistent-order-123");
            assert!(reason.unwrap().contains("already removed"));
        }
        _ => panic!("Expected OrderCancelled event"),
    }
}

// ============================================================================
// Fill Detection Tests
// ============================================================================

#[tokio::test]
async fn test_bid_order_fills_on_sell_trade_at_bid_price() {
    let executor = PaperTradingOrderExecutor::new();

    // Place a bid (buy) order at 0.50 with 100 quote amount
    // This converts to 200 base units (100 / 0.50 = 200)
    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // Simulate a SELL trade at exactly the bid price (seller hitting our bid)
    // Trade size of 200 matches our order size in base units
    let trade = create_polymarket_trade("asset123", TradeSide::Sell, 0.50, 200.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    // Should get a fill
    assert_eq!(fills.len(), 1);
    match &fills[0] {
        LimitOrderEvent::OrderPartiallyFilled {
            filled_size,
            remaining_size,
            fill_price,
            ..
        } => {
            assert_eq!(*filled_size, 200.0); // 100 quote / 0.50 price = 200 base
            assert_eq!(*remaining_size, 0.0); // Fully filled
            assert_eq!(*fill_price, 0.50); // Fill at our order price
        }
        _ => panic!("Expected OrderPartiallyFilled event"),
    }

    // Order should be removed from pending
    assert!(executor.get_pending_orders().await.is_empty());
}

#[tokio::test]
async fn test_bid_order_fills_on_sell_trade_below_bid_price() {
    let executor = PaperTradingOrderExecutor::new();

    // Place a bid at 0.55
    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.55,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // SELL trade at 0.50 (below our bid) - we should get filled
    let trade = create_polymarket_trade("asset123", TradeSide::Sell, 0.50, 100.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    assert_eq!(fills.len(), 1);
    match &fills[0] {
        LimitOrderEvent::OrderPartiallyFilled { fill_price, .. } => {
            assert_eq!(*fill_price, 0.55); // Fill at OUR order price, not trade price
        }
        _ => panic!("Expected fill"),
    }
}

#[tokio::test]
async fn test_ask_order_fills_on_buy_trade_at_ask_price() {
    // Disable inventory constraints for this test
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        enforce_inventory_constraints: false,
        ..Default::default()
    });

    // Place an ask (sell) order at 0.60
    let order = Order::new_limit_sell(
        "asset123".to_string(),
        None,
        50.0,
        0.60,
        false,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // BUY trade at exactly our ask price (buyer lifting our ask)
    let trade = create_polymarket_trade("asset123", TradeSide::Buy, 0.60, 50.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    assert_eq!(fills.len(), 1);
    match &fills[0] {
        LimitOrderEvent::OrderPartiallyFilled {
            filled_size,
            remaining_size,
            fill_price,
            ..
        } => {
            assert_eq!(*filled_size, 50.0);
            assert_eq!(*remaining_size, 0.0);
            assert_eq!(*fill_price, 0.60);
        }
        _ => panic!("Expected fill"),
    }
}

#[tokio::test]
async fn test_ask_order_fills_on_buy_trade_above_ask_price() {
    // Disable inventory constraints for this test
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        enforce_inventory_constraints: false,
        ..Default::default()
    });

    // Place an ask at 0.55
    let order = Order::new_limit_sell(
        "asset123".to_string(),
        None,
        50.0,
        0.55,
        false,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // BUY trade at 0.60 (above our ask) - we should get filled
    let trade = create_polymarket_trade("asset123", TradeSide::Buy, 0.60, 50.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    assert_eq!(fills.len(), 1);
    match &fills[0] {
        LimitOrderEvent::OrderPartiallyFilled { fill_price, .. } => {
            assert_eq!(*fill_price, 0.55); // Fill at OUR order price
        }
        _ => panic!("Expected fill"),
    }
}

#[tokio::test]
async fn test_partial_fill_updates_remaining_size() {
    let executor = PaperTradingOrderExecutor::new();

    // Place a bid for 100 quote units at 0.50 price
    // This converts to 200 base units (100 / 0.50 = 200)
    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // Trade only fills 60 base units (out of 200)
    let trade = create_polymarket_trade("asset123", TradeSide::Sell, 0.50, 60.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    assert_eq!(fills.len(), 1);
    match &fills[0] {
        LimitOrderEvent::OrderPartiallyFilled {
            filled_size,
            remaining_size,
            ..
        } => {
            assert_eq!(*filled_size, 60.0);
            assert_eq!(*remaining_size, 140.0); // 200 - 60 = 140
        }
        _ => panic!("Expected partial fill"),
    }

    // Order should still be pending with reduced size
    let pending = executor.get_pending_orders().await;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].remaining_size, 140.0);
}

#[tokio::test]
async fn test_trade_does_not_cross_price_no_fill() {
    let executor = PaperTradingOrderExecutor::new();

    // Place a bid at 0.50
    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // SELL trade at 0.55 (above our bid) - no fill
    let trade = create_polymarket_trade("asset123", TradeSide::Sell, 0.55, 100.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    assert!(fills.is_empty());

    // Order should still be pending
    let pending = executor.get_pending_orders().await;
    assert_eq!(pending.len(), 1);
}

#[tokio::test]
async fn test_buy_trade_does_not_fill_bid_order() {
    let executor = PaperTradingOrderExecutor::new();

    // Place a bid at 0.50
    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // BUY trade - bids don't fill on buy trades (buyer is lifting asks, not hitting bids)
    let trade = create_polymarket_trade("asset123", TradeSide::Buy, 0.50, 100.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    assert!(fills.is_empty());
}

#[tokio::test]
async fn test_sell_trade_does_not_fill_ask_order() {
    let executor = PaperTradingOrderExecutor::new();

    // Place an ask at 0.60
    let order = Order::new_limit_sell(
        "asset123".to_string(),
        None,
        50.0,
        0.60,
        false,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // SELL trade - asks don't fill on sell trades (seller is hitting bids, not lifting asks)
    let trade = create_polymarket_trade("asset123", TradeSide::Sell, 0.60, 50.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    assert!(fills.is_empty());
}

#[tokio::test]
async fn test_multiple_orders_fill_from_same_trade() {
    let executor = PaperTradingOrderExecutor::new();

    // Place two bid orders at different prices
    let order1 = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        50.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    let order2 = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        50.0,
        0.55, // Higher bid
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order1).await.unwrap();
    executor.execute_limit_order(order2).await.unwrap();

    // Large SELL trade at 0.45 - both orders should fill
    let trade = create_polymarket_trade("asset123", TradeSide::Sell, 0.45, 200.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    // Both orders should fill
    assert_eq!(fills.len(), 2);

    // Orders should be removed
    assert!(executor.get_pending_orders().await.is_empty());
}

#[tokio::test]
async fn test_trade_for_different_asset_no_fill() {
    let executor = PaperTradingOrderExecutor::new();

    // Place order for asset123
    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // Trade for different asset
    let trade = create_polymarket_trade("asset456", TradeSide::Sell, 0.50, 100.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    assert!(fills.is_empty());

    // Original order still pending
    assert_eq!(executor.get_pending_orders().await.len(), 1);
}

// ============================================================================
// Inventory Constraint Tests
// ============================================================================

#[tokio::test]
async fn test_sell_order_blocked_with_no_inventory() {
    // Default executor enforces inventory constraints
    let executor = PaperTradingOrderExecutor::new();
    assert!(executor.enforces_inventory_constraints());

    // Place a sell order
    let order = Order::new_limit_sell(
        "asset123".to_string(),
        None,
        50.0,
        0.60,
        false,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // BUY trade at our ask price - normally would fill
    let trade = create_polymarket_trade("asset123", TradeSide::Buy, 0.60, 50.0);
    // Empty inventory - no position held
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    // Should NOT fill because we have no inventory
    assert!(
        fills.is_empty(),
        "Sell order should not fill with zero inventory"
    );

    // Order should still be pending
    let pending = executor.get_pending_orders().await;
    assert_eq!(pending.len(), 1);
}

#[tokio::test]
async fn test_sell_order_fills_with_sufficient_inventory() {
    let executor = PaperTradingOrderExecutor::new();

    // Place a sell order for 50 units
    let order = Order::new_limit_sell(
        "asset123".to_string(),
        None,
        50.0,
        0.60,
        false,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // BUY trade at our ask price
    let trade = create_polymarket_trade("asset123", TradeSide::Buy, 0.60, 50.0);
    // We have 100 units of inventory
    let mut inventory: HashMap<String, f64> = HashMap::new();
    inventory.insert("asset123".to_string(), 100.0);
    let fills = executor
        .check_fills_from_trade(&trade, &inventory, f64::MAX)
        .await;

    // Should fill because we have sufficient inventory
    assert_eq!(fills.len(), 1);
    match &fills[0] {
        LimitOrderEvent::OrderPartiallyFilled {
            filled_size,
            remaining_size,
            ..
        } => {
            assert_eq!(*filled_size, 50.0);
            assert_eq!(*remaining_size, 0.0);
        }
        _ => panic!("Expected OrderPartiallyFilled event"),
    }
}

#[tokio::test]
async fn test_sell_order_capped_by_available_inventory() {
    let executor = PaperTradingOrderExecutor::new();

    // Place a sell order for 100 units
    let order = Order::new_limit_sell(
        "asset123".to_string(),
        None,
        100.0,
        0.60,
        false,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // BUY trade at our ask price for large amount
    let trade = create_polymarket_trade("asset123", TradeSide::Buy, 0.60, 200.0);
    // We only have 30 units of inventory
    let mut inventory: HashMap<String, f64> = HashMap::new();
    inventory.insert("asset123".to_string(), 30.0);
    let fills = executor
        .check_fills_from_trade(&trade, &inventory, f64::MAX)
        .await;

    // Should fill only 30 units (capped by inventory)
    assert_eq!(fills.len(), 1);
    match &fills[0] {
        LimitOrderEvent::OrderPartiallyFilled {
            filled_size,
            remaining_size,
            ..
        } => {
            assert_eq!(*filled_size, 30.0);
            assert_eq!(*remaining_size, 70.0); // Order has 70 remaining
        }
        _ => panic!("Expected OrderPartiallyFilled event"),
    }

    // Order should still be pending with remaining size
    let pending = executor.get_pending_orders().await;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].remaining_size, 70.0);
}

#[tokio::test]
async fn test_buy_order_not_affected_by_inventory_constraints() {
    let executor = PaperTradingOrderExecutor::new();

    // Place a buy order
    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // SELL trade at our bid price
    let trade = create_polymarket_trade("asset123", TradeSide::Sell, 0.50, 100.0);
    // Empty inventory - buy orders should not be affected
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    // Buy order should fill regardless of inventory
    assert_eq!(fills.len(), 1);
}

#[tokio::test]
async fn test_inventory_constraints_can_be_disabled() {
    // Create executor with inventory constraints disabled
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        enforce_inventory_constraints: false,
        ..Default::default()
    });
    assert!(!executor.enforces_inventory_constraints());

    // Place a sell order
    let order = Order::new_limit_sell(
        "asset123".to_string(),
        None,
        50.0,
        0.60,
        false,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // BUY trade at our ask price
    let trade = create_polymarket_trade("asset123", TradeSide::Buy, 0.60, 50.0);
    // Empty inventory - but constraints are disabled
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    // Should fill even with no inventory because constraints are disabled
    assert_eq!(
        fills.len(),
        1,
        "Sell order should fill when inventory constraints are disabled"
    );
}

#[tokio::test]
async fn test_multiple_sell_orders_respect_inventory() {
    let executor = PaperTradingOrderExecutor::new();

    // Place two sell orders for same asset
    let order1 = Order::new_limit_sell(
        "asset123".to_string(),
        None,
        50.0,
        0.60,
        false,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    let order2 = Order::new_limit_sell(
        "asset123".to_string(),
        None,
        50.0,
        0.65, // Higher ask
        false,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order1).await.unwrap();
    executor.execute_limit_order(order2).await.unwrap();

    // Large BUY trade that could fill both orders
    let trade = create_polymarket_trade("asset123", TradeSide::Buy, 0.70, 200.0);
    // Only 75 units of inventory
    let mut inventory: HashMap<String, f64> = HashMap::new();
    inventory.insert("asset123".to_string(), 75.0);
    let fills = executor
        .check_fills_from_trade(&trade, &inventory, f64::MAX)
        .await;

    // Both orders can partially fill but total fills limited to 75 units
    let total_filled: f64 = fills
        .iter()
        .map(|f| match f {
            LimitOrderEvent::OrderPartiallyFilled { filled_size, .. } => *filled_size,
            _ => 0.0,
        })
        .sum();

    assert_eq!(
        total_filled, 75.0,
        "Total fills should equal available inventory"
    );
}

// ============================================================================
// Redemption Tests
// ============================================================================

#[tokio::test]
async fn test_supports_redemption_returns_true() {
    let executor = PaperTradingOrderExecutor::new();
    assert!(executor.supports_redemption());
}

#[tokio::test]
async fn test_execute_redemption_returns_completed_event() {
    let executor = PaperTradingOrderExecutor::new();
    let timestamp = Utc::now();

    let action = RedemptionAction::new(
        "market-123".to_string(),
        "up-asset-456".to_string(),
        "down-asset-789".to_string(),
        50.0,
        timestamp,
    );

    let result = executor.execute_redemption(&action).await.unwrap();

    match result {
        RedemptionEvent::RedemptionCompleted {
            market,
            up_asset_id,
            down_asset_id,
            quantity,
            quote_received,
            timestamp: ts,
        } => {
            assert_eq!(market, "market-123");
            assert_eq!(up_asset_id, "up-asset-456");
            assert_eq!(down_asset_id, "down-asset-789");
            assert_eq!(quantity, 50.0);
            // 1 pair = $1.00
            assert_eq!(quote_received, 50.0);
            assert_eq!(ts, timestamp);
        }
        RedemptionEvent::RedemptionFailed { .. } => {
            panic!("Expected RedemptionCompleted, got RedemptionFailed");
        }
    }
}

#[tokio::test]
async fn test_execute_redemption_quote_equals_quantity() {
    let executor = PaperTradingOrderExecutor::new();

    // Test various quantities - in binary markets, 1 pair = $1.00
    for quantity in [1.0, 10.0, 100.0, 0.5, 1000.0] {
        let action = RedemptionAction::new(
            "market".to_string(),
            "up".to_string(),
            "down".to_string(),
            quantity,
            Utc::now(),
        );

        let result = executor.execute_redemption(&action).await.unwrap();

        if let RedemptionEvent::RedemptionCompleted {
            quote_received,
            quantity: q,
            ..
        } = result
        {
            assert_eq!(
                quote_received, q,
                "Quote received should equal quantity for binary markets"
            );
            assert_eq!(quote_received, quantity);
        } else {
            panic!("Expected RedemptionCompleted");
        }
    }
}

// ============================================================================
// Utility Method Tests
// ============================================================================

#[tokio::test]
async fn test_get_pending_order_by_id() {
    let executor = PaperTradingOrderExecutor::new();

    // Place an order
    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();

    // Get the order by ID
    let pending = executor.get_pending_order(&order_id).await;
    assert!(pending.is_some());
    assert_eq!(pending.unwrap().order_id, order_id);

    // Try to get a non-existent order
    let missing = executor.get_pending_order("nonexistent").await;
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_pending_order_count() {
    let executor = PaperTradingOrderExecutor::new();

    assert_eq!(executor.pending_order_count().await, 0);

    // Place some orders
    for i in 0..5 {
        let order = Order::new_limit_buy(
            format!("asset{}", i),
            None,
            100.0,
            0.50,
            TimeInForce::GoodTilCancelled,
            Utc::now(),
            None,
            None,
        );
        executor.execute_limit_order(order).await.unwrap();
    }

    assert_eq!(executor.pending_order_count().await, 5);
}

// ============================================================================
// Quote Balance Constraint Tests (Layer 2)
// ============================================================================

#[tokio::test]
async fn test_buy_order_blocked_with_zero_quote() {
    let executor = PaperTradingOrderExecutor::new();

    // Place a buy order for 100 quote units at 0.50 price
    // This converts to 200 base units (100 / 0.50 = 200)
    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0, // quote_amount
        0.50,  // price
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // SELL trade at our bid price - normally would fill
    let trade = create_polymarket_trade("asset123", TradeSide::Sell, 0.50, 200.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();

    // Zero quote available - buy should be skipped entirely
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, 0.0)
        .await;

    // Should NOT fill because quote balance is zero
    assert!(
        fills.is_empty(),
        "Buy order should not fill with zero quote balance"
    );

    // Order should still be pending
    let pending = executor.get_pending_orders().await;
    assert_eq!(pending.len(), 1);
}

#[tokio::test]
async fn test_buy_order_fills_with_sufficient_quote() {
    let executor = PaperTradingOrderExecutor::new();

    // Place a buy order for 100 quote units at 0.50 price
    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // SELL trade at our bid price
    let trade = create_polymarket_trade("asset123", TradeSide::Sell, 0.50, 200.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();

    // Plenty of quote available (1000 > 100 needed)
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, 1000.0)
        .await;

    // Should fill
    assert_eq!(fills.len(), 1);
    match &fills[0] {
        LimitOrderEvent::OrderPartiallyFilled { filled_size, .. } => {
            assert_eq!(*filled_size, 200.0); // Full fill
        }
        _ => panic!("Expected fill"),
    }
}

#[tokio::test]
async fn test_buy_order_partial_fill_due_to_quote_constraint() {
    let executor = PaperTradingOrderExecutor::new();

    // Place a buy order for 100 quote units at 0.50 price = 200 base units
    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // SELL trade at our bid price
    let trade = create_polymarket_trade("asset123", TradeSide::Sell, 0.50, 200.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();

    // Only 60 quote available - can only buy 60/0.50 = 120 base units
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, 60.0)
        .await;

    // Should partial fill
    assert_eq!(fills.len(), 1);
    match &fills[0] {
        LimitOrderEvent::OrderPartiallyFilled {
            filled_size,
            remaining_size,
            ..
        } => {
            assert!(
                (filled_size - 120.0).abs() < 0.001,
                "Should fill 120 units (60 quote / 0.50 price)"
            );
            assert!(
                (remaining_size - 80.0).abs() < 0.001,
                "Should have 80 units remaining"
            );
        }
        _ => panic!("Expected partial fill"),
    }
}

#[tokio::test]
async fn test_multiple_buy_orders_respect_quote_across_fills() {
    let executor = PaperTradingOrderExecutor::new();

    // Place two buy orders, each needing 50 quote (100 base units at 0.50)
    let order1 = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        50.0, // 50 quote = 100 base units at 0.50
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    let order2 = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        50.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order1).await.unwrap();
    executor.execute_limit_order(order2).await.unwrap();

    // Large SELL trade that could fill both orders
    let trade = create_polymarket_trade("asset123", TradeSide::Sell, 0.50, 500.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();

    // Only 75 quote available - can fill one order fully (50) and partial second (25)
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, 75.0)
        .await;

    // Should have fills from both orders
    let total_filled: f64 = fills
        .iter()
        .map(|f| match f {
            LimitOrderEvent::OrderPartiallyFilled { filled_size, .. } => *filled_size,
            _ => 0.0,
        })
        .sum();

    // Total filled should be 150 base units (75 quote / 0.50 price)
    assert!(
        (total_filled - 150.0).abs() < 0.001,
        "Total fills should be limited by quote balance"
    );
}

#[tokio::test]
async fn test_sell_order_not_affected_by_quote_constraint() {
    // Disable inventory constraints for this test
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        enforce_inventory_constraints: false,
        ..Default::default()
    });

    // Place a sell order
    let order = Order::new_limit_sell(
        "asset123".to_string(),
        None,
        50.0,
        0.60,
        false,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // BUY trade at our ask price
    let trade = create_polymarket_trade("asset123", TradeSide::Buy, 0.60, 50.0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();

    // Even with zero quote balance, sell orders should fill (quote constraint is for buys)
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, 0.0)
        .await;

    // Sell order should fill regardless of quote balance
    assert_eq!(fills.len(), 1);
}

// ============================================================================
// Latency Simulation Tests
// ============================================================================

use crate::config::LatencySimulationConfig;

/// Helper to create a PolymarketTradeEvent with a specific timestamp
fn create_polymarket_trade_with_timestamp(
    asset_id: &str,
    side: TradeSide,
    price: f64,
    size: f64,
    timestamp_ms: i64,
) -> PolymarketTradeEvent {
    PolymarketTradeEvent {
        asset_id: asset_id.to_string(),
        market: "test-market".to_string(),
        price,
        size,
        side,
        timestamp: timestamp_ms,
        fee_rate_bps: 0,
        market_metadata: None,
        observed_at: Utc::now(),
    }
}

#[tokio::test]
async fn test_cancel_intent_persists_while_submit_pending() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 1000,
        max_place_latency_ms: 1000,
        seed: None,
        min_cancel_latency_ms: Some(200),
        max_cancel_latency_ms: Some(200),
        min_quote_lifetime_ms: None,
    };
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        latency_config: Some(latency_config),
        ..Default::default()
    });

    let base_ts = Utc::now().timestamp_millis();
    let order_timestamp = chrono::DateTime::from_timestamp_millis(base_ts).unwrap();
    let order = Order {
        mint: "asset123".to_string(),
        market: None,
        order_type: OrderType::LimitBuy {
            quote_amount: 100.0,
            limit_price: 0.50,
            time_in_force: TimeInForce::GoodTilCancelled,
        },
        price: Some(0.50),
        status: OrderStatus::Pending,
        timestamp: order_timestamp,
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();

    // Cancel before placement confirmation evidence arrives.
    let _ = executor.cancel_order(&order_id).await.unwrap();

    let empty_inventory: HashMap<String, f64> = HashMap::new();

    let before_eligible = create_polymarket_trade_with_timestamp(
        "asset123",
        TradeSide::Buy,
        0.70,
        10.0,
        base_ts + 900,
    );
    let early_events = executor
        .check_fills_from_trade(&before_eligible, &empty_inventory, f64::MAX)
        .await;
    assert!(
        early_events.is_empty(),
        "No lifecycle terminal event should emit before placement eligibility"
    );

    let before_cancel_confirm = create_polymarket_trade_with_timestamp(
        "asset123",
        TradeSide::Buy,
        0.70,
        10.0,
        base_ts + 1100,
    );
    let mid_events = executor
        .check_fills_from_trade(&before_cancel_confirm, &empty_inventory, f64::MAX)
        .await;
    assert!(
        mid_events.is_empty(),
        "Cancel should remain pending before confirmation deadline"
    );

    let at_cancel_confirm = create_polymarket_trade_with_timestamp(
        "asset123",
        TradeSide::Buy,
        0.70,
        10.0,
        base_ts + 1200,
    );
    let final_events = executor
        .check_fills_from_trade(&at_cancel_confirm, &empty_inventory, f64::MAX)
        .await;

    assert_eq!(final_events.len(), 1);
    assert!(
        matches!(
            &final_events[0],
            LimitOrderEvent::OrderCancelled { order_id: cancelled_id, .. } if cancelled_id == &order_id
        ),
        "Expected terminal cancellation confirmation event"
    );
    assert!(
        final_events
            .iter()
            .all(|event| !matches!(event, LimitOrderEvent::OrderPlaced { .. })),
        "Submit-pending cancel path should not emit OrderPlaced"
    );
    assert!(
        executor.get_pending_order(&order_id).await.is_none(),
        "Order should be removed once cancellation is confirmed"
    );
}

#[tokio::test]
async fn test_cancel_confirmation_is_evidence_driven_after_open() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 1000,
        max_place_latency_ms: 1000,
        seed: None,
        min_cancel_latency_ms: Some(500),
        max_cancel_latency_ms: Some(500),
        min_quote_lifetime_ms: None,
    };
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        latency_config: Some(latency_config),
        ..Default::default()
    });

    let base_ts = Utc::now().timestamp_millis();
    let order_timestamp = chrono::DateTime::from_timestamp_millis(base_ts).unwrap();
    let order = Order {
        mint: "asset123".to_string(),
        market: None,
        order_type: OrderType::LimitBuy {
            quote_amount: 100.0,
            limit_price: 0.50,
            time_in_force: TimeInForce::GoodTilCancelled,
        },
        price: Some(0.50),
        status: OrderStatus::Pending,
        timestamp: order_timestamp,
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();
    let empty_inventory: HashMap<String, f64> = HashMap::new();

    // Transition submit-pending -> open.
    let placement_trade = create_polymarket_trade_with_timestamp(
        "asset123",
        TradeSide::Buy,
        0.70,
        10.0,
        base_ts + 1000,
    );
    let placement_events = executor
        .check_fills_from_trade(&placement_trade, &empty_inventory, f64::MAX)
        .await;
    assert!(
        placement_events
            .iter()
            .any(|event| matches!(event, LimitOrderEvent::OrderPlaced { .. })),
        "Expected deferred OrderPlaced event once placement is confirmed"
    );

    let _ = executor.cancel_order(&order_id).await.unwrap();

    let before_cancel_confirm = create_polymarket_trade_with_timestamp(
        "asset123",
        TradeSide::Buy,
        0.70,
        10.0,
        base_ts + 1400,
    );
    let early_cancel_events = executor
        .check_fills_from_trade(&before_cancel_confirm, &empty_inventory, f64::MAX)
        .await;
    assert!(
        early_cancel_events
            .iter()
            .all(|event| !matches!(event, LimitOrderEvent::OrderCancelled { .. })),
        "Cancel should not be terminal before confirmation evidence deadline"
    );

    let at_cancel_confirm = create_polymarket_trade_with_timestamp(
        "asset123",
        TradeSide::Buy,
        0.70,
        10.0,
        base_ts + 1500,
    );
    let confirmed_events = executor
        .check_fills_from_trade(&at_cancel_confirm, &empty_inventory, f64::MAX)
        .await;
    assert_eq!(
        confirmed_events
            .iter()
            .filter(|event| matches!(event, LimitOrderEvent::OrderCancelled { .. }))
            .count(),
        1,
        "Expected one terminal cancellation event at confirmation time"
    );
    assert!(executor.get_pending_order(&order_id).await.is_none());
}

#[tokio::test]
async fn test_cancel_terminal_event_is_deduplicated() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 1000,
        max_place_latency_ms: 1000,
        seed: None,
        min_cancel_latency_ms: Some(300),
        max_cancel_latency_ms: Some(300),
        min_quote_lifetime_ms: None,
    };
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        latency_config: Some(latency_config),
        ..Default::default()
    });

    let base_ts = Utc::now().timestamp_millis();
    let order_timestamp = chrono::DateTime::from_timestamp_millis(base_ts).unwrap();
    let order = Order {
        mint: "asset123".to_string(),
        market: None,
        order_type: OrderType::LimitBuy {
            quote_amount: 100.0,
            limit_price: 0.50,
            time_in_force: TimeInForce::GoodTilCancelled,
        },
        price: Some(0.50),
        status: OrderStatus::Pending,
        timestamp: order_timestamp,
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();
    let empty_inventory: HashMap<String, f64> = HashMap::new();

    let placement_trade = create_polymarket_trade_with_timestamp(
        "asset123",
        TradeSide::Buy,
        0.70,
        10.0,
        base_ts + 1000,
    );
    let _ = executor
        .check_fills_from_trade(&placement_trade, &empty_inventory, f64::MAX)
        .await;

    // Duplicate cancel intents should still produce one terminal cancellation.
    let _ = executor.cancel_order(&order_id).await.unwrap();
    let _ = executor.cancel_order(&order_id).await.unwrap();

    let confirmation_trade = create_polymarket_trade_with_timestamp(
        "asset123",
        TradeSide::Buy,
        0.70,
        10.0,
        base_ts + 1300,
    );
    let confirmation_events = executor
        .check_fills_from_trade(&confirmation_trade, &empty_inventory, f64::MAX)
        .await;
    assert_eq!(
        confirmation_events
            .iter()
            .filter(|event| matches!(event, LimitOrderEvent::OrderCancelled { .. }))
            .count(),
        1,
        "Expected a single terminal cancellation event"
    );

    let duplicate_terminal_evidence = create_polymarket_trade_with_timestamp(
        "asset123",
        TradeSide::Buy,
        0.70,
        10.0,
        base_ts + 1600,
    );
    let duplicate_events = executor
        .check_fills_from_trade(&duplicate_terminal_evidence, &empty_inventory, f64::MAX)
        .await;
    assert!(
        duplicate_events
            .iter()
            .all(|event| !matches!(event, LimitOrderEvent::OrderCancelled { .. })),
        "No duplicate cancellation should emit after terminal state"
    );
}

#[tokio::test]
async fn test_with_latency_config_constructor() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 800,
        max_place_latency_ms: 1500,
        seed: None,
        min_cancel_latency_ms: None,
        max_cancel_latency_ms: None,
        min_quote_lifetime_ms: None,
    };
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        latency_config: Some(latency_config),
        ..Default::default()
    });
    assert!(executor.enforces_inventory_constraints());
    assert_eq!(executor.pending_order_count().await, 0);
}

#[tokio::test]
async fn test_order_not_filled_during_latency_window() {
    // Use fixed latency for deterministic test (min = max)
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 1000,
        max_place_latency_ms: 1000, // Fixed latency for determinism
        seed: None,
        min_cancel_latency_ms: None,
        max_cancel_latency_ms: None,
        min_quote_lifetime_ms: None,
    };
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        latency_config: Some(latency_config),
        ..Default::default()
    });

    // Order timestamp at t=0
    let order_timestamp = chrono::DateTime::from_timestamp_millis(0).unwrap();
    let order = Order {
        mint: "asset123".to_string(),
        market: None,
        order_type: OrderType::LimitBuy {
            quote_amount: 100.0,
            limit_price: 0.50,
            time_in_force: TimeInForce::GoodTilCancelled,
        },
        price: Some(0.50),
        status: OrderStatus::Pending,
        timestamp: order_timestamp,
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    executor.execute_limit_order(order).await.unwrap();

    // Trade at t=500ms should NOT fill (order eligible at t=1000ms)
    let trade =
        create_polymarket_trade_with_timestamp("asset123", TradeSide::Sell, 0.49, 200.0, 500);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    assert!(
        fills.is_empty(),
        "Order should not fill before eligibility time (500ms < 1050ms)"
    );

    // Order should still be pending
    assert_eq!(executor.pending_order_count().await, 1);
}

#[tokio::test]
async fn test_order_fills_after_latency_window() {
    // Use fixed latency for deterministic test
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 1000,
        max_place_latency_ms: 1000, // Fixed latency for determinism
        seed: None,
        min_cancel_latency_ms: None,
        max_cancel_latency_ms: None,
        min_quote_lifetime_ms: None,
    };
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        latency_config: Some(latency_config),
        ..Default::default()
    });

    // Order timestamp at t=0
    let order_timestamp = chrono::DateTime::from_timestamp_millis(0).unwrap();
    let order = Order {
        mint: "asset123".to_string(),
        market: None,
        order_type: OrderType::LimitBuy {
            quote_amount: 100.0,
            limit_price: 0.50,
            time_in_force: TimeInForce::GoodTilCancelled,
        },
        price: Some(0.50),
        status: OrderStatus::Pending,
        timestamp: order_timestamp,
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    executor.execute_limit_order(order).await.unwrap();

    // Trade at t=1100ms SHOULD fill (order eligible at t=1000ms)
    let trade =
        create_polymarket_trade_with_timestamp("asset123", TradeSide::Sell, 0.49, 200.0, 1100);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    assert_eq!(
        fills.len(),
        2,
        "Deferred placement should emit OrderPlaced plus fill after eligibility time"
    );
    assert!(
        fills
            .iter()
            .any(|event| matches!(event, LimitOrderEvent::OrderPlaced { .. })),
        "Expected deferred OrderPlaced event"
    );
    assert!(
        fills.iter().any(|event| matches!(
            event,
            LimitOrderEvent::OrderPartiallyFilled {
                remaining_size,
                ..
            } if *remaining_size == 0.0
        )),
        "Expected full fill event"
    );

    // Order should be removed from pending
    assert!(executor.get_pending_orders().await.is_empty());
}

#[tokio::test]
async fn test_order_fills_exactly_at_eligibility_boundary() {
    // Use fixed latency for deterministic test
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 1000,
        max_place_latency_ms: 1000,
        seed: None,
        min_cancel_latency_ms: None,
        max_cancel_latency_ms: None,
        min_quote_lifetime_ms: None,
    };
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        latency_config: Some(latency_config),
        ..Default::default()
    });

    // Order timestamp at t=0
    let order_timestamp = chrono::DateTime::from_timestamp_millis(0).unwrap();
    let order = Order {
        mint: "asset123".to_string(),
        market: None,
        order_type: OrderType::LimitBuy {
            quote_amount: 100.0,
            limit_price: 0.50,
            time_in_force: TimeInForce::GoodTilCancelled,
        },
        price: Some(0.50),
        status: OrderStatus::Pending,
        timestamp: order_timestamp,
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    executor.execute_limit_order(order).await.unwrap();

    // Trade exactly at eligibility time (t=1050ms) should fill
    let trade =
        create_polymarket_trade_with_timestamp("asset123", TradeSide::Sell, 0.49, 200.0, 1050);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    assert_eq!(
        fills.len(),
        2,
        "Deferred placement should emit OrderPlaced plus fill at eligibility boundary"
    );
    assert!(
        fills
            .iter()
            .any(|event| matches!(event, LimitOrderEvent::OrderPlaced { .. })),
        "Expected deferred OrderPlaced event"
    );
    assert!(
        fills.iter().any(|event| matches!(
            event,
            LimitOrderEvent::OrderPartiallyFilled {
                remaining_size,
                ..
            } if *remaining_size == 0.0
        )),
        "Expected full fill event"
    );
}

#[tokio::test]
async fn test_no_latency_config_means_immediate_eligibility() {
    // Create executor without latency config (default)
    let executor = PaperTradingOrderExecutor::new();

    // Order timestamp at t=0
    let order_timestamp = chrono::DateTime::from_timestamp_millis(0).unwrap();
    let order = Order {
        mint: "asset123".to_string(),
        market: None,
        order_type: OrderType::LimitBuy {
            quote_amount: 100.0,
            limit_price: 0.50,
            time_in_force: TimeInForce::GoodTilCancelled,
        },
        price: Some(0.50),
        status: OrderStatus::Pending,
        timestamp: order_timestamp,
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    executor.execute_limit_order(order).await.unwrap();

    // Trade at t=1ms should fill immediately (no latency configured)
    let trade = create_polymarket_trade_with_timestamp("asset123", TradeSide::Sell, 0.49, 200.0, 1);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fills = executor
        .check_fills_from_trade(&trade, &empty_inventory, f64::MAX)
        .await;

    assert_eq!(
        fills.len(),
        1,
        "Order should fill immediately without latency config"
    );
}

#[tokio::test]
async fn test_latency_within_configured_range() {
    // Use a range of latencies
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 800,
        max_place_latency_ms: 1500,
        seed: None,
        min_cancel_latency_ms: None,
        max_cancel_latency_ms: None,
        min_quote_lifetime_ms: None,
    };
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        latency_config: Some(latency_config),
        ..Default::default()
    });

    // Place an order at t=0
    let order_timestamp = chrono::DateTime::from_timestamp_millis(0).unwrap();
    let order = Order {
        mint: "asset123".to_string(),
        market: None,
        order_type: OrderType::LimitBuy {
            quote_amount: 100.0,
            limit_price: 0.50,
            time_in_force: TimeInForce::GoodTilCancelled,
        },
        price: Some(0.50),
        status: OrderStatus::Pending,
        timestamp: order_timestamp,
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    executor.execute_limit_order(order).await.unwrap();

    // Check the pending order's eligible_for_fills_at
    let pending = executor.get_pending_orders().await;
    assert_eq!(pending.len(), 1);

    let eligibility_time = pending[0].eligible_for_fills_at;
    let order_time = pending[0].placed_at;
    let latency_ms = (eligibility_time - order_time).num_milliseconds();

    // Total latency should be within [800..1500] ms
    // So total range is [850..1550]
    assert!(
        latency_ms >= 800 && latency_ms <= 1500,
        "Latency {} should be within [850, 1550]ms range",
        latency_ms
    );
}

#[tokio::test]
async fn test_multiple_orders_have_independent_eligibility_times() {
    // Use a range of latencies
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 800,
        max_place_latency_ms: 1500,
        seed: None,
        min_cancel_latency_ms: None,
        max_cancel_latency_ms: None,
        min_quote_lifetime_ms: None,
    };
    let executor = PaperTradingOrderExecutor::with_config(PaperTradingConfig {
        latency_config: Some(latency_config),
        ..Default::default()
    });

    // Place two orders at the same timestamp
    let order_timestamp = chrono::DateTime::from_timestamp_millis(0).unwrap();

    let order1 = Order {
        mint: "asset1".to_string(),
        market: None,
        order_type: OrderType::LimitBuy {
            quote_amount: 100.0,
            limit_price: 0.50,
            time_in_force: TimeInForce::GoodTilCancelled,
        },
        price: Some(0.50),
        status: OrderStatus::Pending,
        timestamp: order_timestamp,
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    let order2 = Order {
        mint: "asset2".to_string(),
        market: None,
        order_type: OrderType::LimitBuy {
            quote_amount: 100.0,
            limit_price: 0.50,
            time_in_force: TimeInForce::GoodTilCancelled,
        },
        price: Some(0.50),
        status: OrderStatus::Pending,
        timestamp: order_timestamp,
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };

    executor.execute_limit_order(order1).await.unwrap();
    executor.execute_limit_order(order2).await.unwrap();

    let pending = executor.get_pending_orders().await;
    assert_eq!(pending.len(), 2);

    // Both orders should have eligibility times within the valid range
    for order in &pending {
        let latency_ms = (order.eligible_for_fills_at - order.placed_at).num_milliseconds();
        assert!(
            latency_ms >= 800 && latency_ms <= 1500,
            "Order {} latency {} should be within range",
            order.order_id,
            latency_ms
        );
    }
}
