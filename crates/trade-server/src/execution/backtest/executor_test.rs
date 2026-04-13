use std::collections::HashMap;

use chrono::{DateTime, Utc};
use popeyes_trading_types::{PolymarketTradeEvent, PumpFunTradeEvent, TradeEventType, TradeSide};

use super::*;
use crate::config::LatencySimulationConfig;
use crate::execution::{
    ExecutionEvent, Order, OrderStatus, OrderType,
    events::{LimitOrderEvent, OrderSide, RedemptionEvent, TimeInForce},
    executor::OrderExecutor,
};
use crate::signal::RedemptionAction;

#[tokio::test]
async fn test_price_tracking() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

    // Create test trade events
    let trade1 = PumpFunTradeEvent::new(
        "sig1".to_string(),
        "TOKEN1".to_string(),
        "trader1".to_string(),
        TradeEventType::Buy,
        100.0,      // token_amount
        Some(1.74), // sol_amount
        Some(1000.0),
        Some(100.0),
        Some(100.0),
        None, // real_sol_reserves
        None, // real_token_reserves
        None, // fee_recipient
        None, // fee_basis_points
        None, // fee_amount
        None, // creator
        None, // creator_fee_basis_points
        None, // creator_fee_amount
        Utc::now(),
        0,
    );

    let trade2 = PumpFunTradeEvent::new(
        "sig2".to_string(),
        "TOKEN2".to_string(),
        "trader2".to_string(),
        TradeEventType::Buy,
        50.0,
        Some(0.13), // sol_amount
        Some(500.0),
        Some(50.0),
        Some(50.0),
        None, // real_sol_reserves
        None, // real_token_reserves
        None, // fee_recipient
        None, // fee_basis_points
        None, // fee_amount
        None, // creator
        None, // creator_fee_basis_points
        None, // creator_fee_amount
        Utc::now(),
        0,
    );

    // Handle trades to update prices
    executor.handle_token_trade(&trade1.into()).await.unwrap();
    executor.handle_token_trade(&trade2.into()).await.unwrap();

    // Verify prices are tracked correctly
    assert_eq!(executor.last_prices("TOKEN1").await, Some(0.1));
    assert_eq!(executor.last_prices("TOKEN2").await, Some(0.1));
}

#[tokio::test]
async fn test_slippage_calculation() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build(); // 1% slippage

    // Setup initial price
    let trade = PumpFunTradeEvent::new(
        "sig1".to_string(),
        "TOKEN1".to_string(),
        "trader1".to_string(),
        TradeEventType::Buy,
        100.0,     // token_amount
        Some(1.0), // sol_amount
        Some(1000.0),
        Some(100.0),
        Some(100.0),
        None, // real_sol_reserves
        None, // real_token_reserves
        None, // fee_recipient
        None, // fee_basis_points
        None, // fee_amount
        None, // creator
        None, // creator_fee_basis_points
        None, // creator_fee_amount
        Utc::now(),
        0,
    );
    executor.handle_token_trade(&trade.into()).await.unwrap();

    // Test buy order with slippage
    #[allow(deprecated)]
    let buy_order = Order {
        mint: "TOKEN1".to_string(),
        market: None,
        order_type: OrderType::Buy { sol_amount: 1.0 }, // 1 SOL buy
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

    let result = executor.execute_market_order(buy_order).await.unwrap();
    if let ExecutionEvent::OrderFilled { price, .. } = result {
        assert_eq!(price, Some(0.101)); // 0.1 * 1.01
    } else {
        panic!("Expected OrderFilled event");
    }

    // Test sell order with slippage
    #[allow(deprecated)]
    let sell_order = Order {
        mint: "TOKEN1".to_string(),
        market: None,
        order_type: OrderType::Sell {
            token_amount: 10.0,
            clear_position: false,
        }, // Sell 10 tokens
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

    let result = executor.execute_market_order(sell_order).await.unwrap();
    if let ExecutionEvent::OrderFilled { price, .. } = result {
        assert_eq!(price, Some(0.099)); // 0.1 * 0.99
    } else {
        panic!("Expected OrderFilled event");
    }
}

#[tokio::test]
async fn test_missing_price_data() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

    #[allow(deprecated)]
    let order = Order {
        mint: "UNKNOWN".to_string(),
        market: None,
        order_type: OrderType::Buy { sol_amount: 1.0 }, // 1 SOL buy
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

    let result = executor.execute_market_order(order).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "No price data for UNKNOWN");
}

// ============================================================================
// Limit Order Tests
// ============================================================================

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

/// Helper to create a PolymarketTradeEvent with a specific timestamp (ms).
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
async fn test_execute_limit_buy_order_placed() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
            assert!(order_id.starts_with("bt-order-"));
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
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
async fn test_bid_order_fills_on_sell_trade_at_bid_price() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

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
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    assert_eq!(fills.len(), 1);
    match &fills[0] {
        LimitOrderEvent::OrderPartiallyFilled { fill_price, .. } => {
            assert_eq!(*fill_price, 0.55); // Fill at OUR order price, not trade price
        }
        _ => panic!("Expected fill"),
    }
}

#[tokio::test]
async fn test_bid_order_fills_on_mirrored_buy_trade_on_complement_asset() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();
    let empty_inventory: HashMap<String, f64> = HashMap::new();

    // Seed complement mapping for a binary market by observing both assets in the same market.
    let _ = executor
        .handle_polymarket_trade(
            &create_polymarket_trade("asset_yes", TradeSide::Buy, 0.60, 1.0),
            &empty_inventory,
            f64::MAX,
        )
        .await
        .unwrap();
    let _ = executor
        .handle_polymarket_trade(
            &create_polymarket_trade("asset_no", TradeSide::Buy, 0.40, 1.0),
            &empty_inventory,
            f64::MAX,
        )
        .await
        .unwrap();

    // Place a bid on the complement asset at 0.41 (base size = 41 / 0.41 = 100).
    let order = Order::new_limit_buy(
        "asset_no".to_string(),
        None,
        41.0,
        0.41,
        TimeInForce::GoodTilCancelled,
        Utc::now(),
        None,
        None,
    );
    executor.execute_limit_order(order).await.unwrap();

    // A BUY trade on YES at 0.60 is mirrored as a SELL trade on NO at 0.40 (1 - 0.60).
    // That mirrored SELL should hit our bid at 0.41.
    let trade = create_polymarket_trade("asset_yes", TradeSide::Buy, 0.60, 100.0);
    let fills = executor
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    assert_eq!(fills.len(), 1);
    match &fills[0] {
        LimitOrderEvent::OrderPartiallyFilled {
            mint,
            side,
            filled_size,
            remaining_size,
            fill_price,
            ..
        } => {
            assert_eq!(mint, "asset_no");
            assert_eq!(*side, OrderSide::Buy);
            assert_eq!(*filled_size, 100.0);
            assert_eq!(*remaining_size, 0.0);
            assert_eq!(*fill_price, 0.41);
        }
        other => panic!("Expected OrderPartiallyFilled, got {:?}", other),
    }

    // Order should be removed from pending
    assert!(executor.get_pending_orders().await.is_empty());
}

#[tokio::test]
async fn test_ask_order_fills_on_buy_trade_at_ask_price() {
    // Disable inventory constraints for this legacy test
    let executor = BacktestOrderExecutor::builder()
        .slippage(0.01)
        .enforce_inventory_constraints(false)
        .build();

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
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

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
    // Disable inventory constraints for this legacy test
    let executor = BacktestOrderExecutor::builder()
        .slippage(0.01)
        .enforce_inventory_constraints(false)
        .build();

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
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

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
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

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
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    assert!(fills.is_empty());

    // Order should still be pending
    let pending = executor.get_pending_orders().await;
    assert_eq!(pending.len(), 1);
}

#[tokio::test]
async fn test_buy_trade_does_not_fill_bid_order() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    assert!(fills.is_empty());
}

#[tokio::test]
async fn test_sell_trade_does_not_fill_ask_order() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    assert!(fills.is_empty());
}

#[tokio::test]
async fn test_cancel_removes_order_and_returns_event() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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

#[tokio::test]
async fn test_cancel_grace_period_allows_fill() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 0,
        max_place_latency_ms: 0,
        seed: None,
        min_cancel_latency_ms: Some(1000),
        max_cancel_latency_ms: Some(1000),
        min_quote_lifetime_ms: None,
    };

    let executor = BacktestOrderExecutor::builder()
        .latency_config(Some(latency_config))
        .build();

    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();

    executor.cancel_order(&order_id).await.unwrap();

    let trade =
        create_polymarket_trade_with_timestamp("asset123", TradeSide::Sell, 0.50, 200.0, 500);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let events = executor
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    assert!(
        events
            .iter()
            .any(|e| matches!(e, LimitOrderEvent::OrderPartiallyFilled { .. })),
        "Expected fill during cancel latency window"
    );
}

#[tokio::test]
async fn test_submit_pending_cancel_executes_after_place_confirmation() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 1000,
        max_place_latency_ms: 1000,
        seed: None,
        min_cancel_latency_ms: Some(0),
        max_cancel_latency_ms: Some(0),
        min_quote_lifetime_ms: None,
    };

    let executor = BacktestOrderExecutor::builder()
        .latency_config(Some(latency_config))
        .build();

    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();

    let empty_inventory: HashMap<String, f64> = HashMap::new();

    // Advance the simulation clock before cancellation so cancel latency uses simulation time.
    let tick_100 = create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 100);
    let pre_cancel_events = executor
        .handle_polymarket_trade(&tick_100, &empty_inventory, f64::MAX)
        .await
        .unwrap();
    assert!(pre_cancel_events.is_empty());

    executor.cancel_order(&order_id).await.unwrap();

    // Cancellation is requested while still submit-pending; it should not finalize yet.
    let tick_500 = create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 500);
    let before_live_events = executor
        .handle_polymarket_trade(&tick_500, &empty_inventory, f64::MAX)
        .await
        .unwrap();
    assert!(
        before_live_events
            .iter()
            .all(|event| !matches!(event, LimitOrderEvent::OrderCancelled { .. }))
    );

    // Once placement confirms, queued cancel intent can transition to terminal cancellation.
    let tick_1000 =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 1000);
    let at_live_events = executor
        .handle_polymarket_trade(&tick_1000, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    let cancelled_at_live = at_live_events
        .iter()
        .filter(|event| {
            matches!(
                event,
                LimitOrderEvent::OrderCancelled {
                    order_id: cancelled_id,
                    ..
                } if cancelled_id == &order_id
            )
        })
        .count();
    assert_eq!(cancelled_at_live, 1);
    assert!(
        at_live_events
            .iter()
            .all(|event| !matches!(event, LimitOrderEvent::OrderPlaced { .. }))
    );
}

#[tokio::test]
async fn test_post_cancel_no_fill() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 0,
        max_place_latency_ms: 0,
        seed: None,
        min_cancel_latency_ms: Some(1000),
        max_cancel_latency_ms: Some(1000),
        min_quote_lifetime_ms: None,
    };

    let executor = BacktestOrderExecutor::builder()
        .latency_config(Some(latency_config))
        .build();

    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();

    executor.cancel_order(&order_id).await.unwrap();

    let trade =
        create_polymarket_trade_with_timestamp("asset123", TradeSide::Sell, 0.50, 200.0, 1500);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let events = executor
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    assert!(
        events
            .iter()
            .all(|e| !matches!(e, LimitOrderEvent::OrderPartiallyFilled { .. })),
        "Expected no fills after cancel latency elapsed"
    );
}

#[tokio::test]
async fn test_partial_fill_then_cancel_finalization_emits_single_terminal_cancel() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 0,
        max_place_latency_ms: 0,
        seed: None,
        min_cancel_latency_ms: Some(1000),
        max_cancel_latency_ms: Some(1000),
        min_quote_lifetime_ms: None,
    };

    let executor = BacktestOrderExecutor::builder()
        .latency_config(Some(latency_config))
        .build();

    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();

    executor.cancel_order(&order_id).await.unwrap();

    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let partial_fill_trade =
        create_polymarket_trade_with_timestamp("asset123", TradeSide::Sell, 0.50, 50.0, 500);
    let partial_events = executor
        .handle_polymarket_trade(&partial_fill_trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();
    assert!(
        partial_events.iter().any(|event| {
            matches!(
                event,
                LimitOrderEvent::OrderPartiallyFilled {
                    order_id: filled_id,
                    remaining_size,
                    ..
                } if filled_id == &order_id && *remaining_size > 0.0
            )
        }),
        "Expected partial fill before cancel terminal finalization"
    );

    let cancel_tick =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 1500);
    let cancel_events = executor
        .handle_polymarket_trade(&cancel_tick, &empty_inventory, f64::MAX)
        .await
        .unwrap();
    let cancel_count = cancel_events
        .iter()
        .filter(|event| {
            matches!(
                event,
                LimitOrderEvent::OrderCancelled {
                    order_id: cancelled_id,
                    ..
                } if cancelled_id == &order_id
            )
        })
        .count();
    assert_eq!(cancel_count, 1);

    let dedup_tick =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 2500);
    let dedup_events = executor
        .handle_polymarket_trade(&dedup_tick, &empty_inventory, f64::MAX)
        .await
        .unwrap();
    assert!(
        dedup_events.iter().all(|event| {
            !matches!(
                event,
                LimitOrderEvent::OrderCancelled {
                    order_id: cancelled_id,
                    ..
                } if cancelled_id == &order_id
            )
        }),
        "Expected exactly one terminal cancel event"
    );
}

#[tokio::test]
async fn test_duplicate_cancel_requests_emit_single_terminal_cancel() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 0,
        max_place_latency_ms: 0,
        seed: None,
        min_cancel_latency_ms: Some(1000),
        max_cancel_latency_ms: Some(1000),
        min_quote_lifetime_ms: None,
    };

    let executor = BacktestOrderExecutor::builder()
        .latency_config(Some(latency_config))
        .build();

    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();

    executor.cancel_order(&order_id).await.unwrap();
    executor.cancel_order(&order_id).await.unwrap();

    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let cancel_tick =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 1500);
    let cancel_events = executor
        .handle_polymarket_trade(&cancel_tick, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    let cancel_count = cancel_events
        .iter()
        .filter(|event| {
            matches!(
                event,
                LimitOrderEvent::OrderCancelled {
                    order_id: cancelled_id,
                    ..
                } if cancelled_id == &order_id
            )
        })
        .count();
    assert_eq!(cancel_count, 1);
}

#[tokio::test]
async fn test_fill_before_cancel_finalization_suppresses_late_cancel_terminal() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 0,
        max_place_latency_ms: 0,
        seed: None,
        min_cancel_latency_ms: Some(1000),
        max_cancel_latency_ms: Some(1000),
        min_quote_lifetime_ms: None,
    };

    let executor = BacktestOrderExecutor::builder()
        .latency_config(Some(latency_config))
        .build();

    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();

    executor.cancel_order(&order_id).await.unwrap();

    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fill_trade =
        create_polymarket_trade_with_timestamp("asset123", TradeSide::Sell, 0.50, 200.0, 500);
    let fill_events = executor
        .handle_polymarket_trade(&fill_trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();
    assert!(
        fill_events.iter().any(|event| {
            matches!(
                event,
                LimitOrderEvent::OrderPartiallyFilled {
                    order_id: filled_id,
                    remaining_size,
                    ..
                } if filled_id == &order_id && *remaining_size == 0.0
            )
        }),
        "Expected terminal fill before cancel finalization"
    );

    let post_cancel_deadline_tick =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 1500);
    let post_events = executor
        .handle_polymarket_trade(&post_cancel_deadline_tick, &empty_inventory, f64::MAX)
        .await
        .unwrap();
    assert!(
        post_events.iter().all(|event| {
            !matches!(
                event,
                LimitOrderEvent::OrderCancelled {
                    order_id: cancelled_id,
                    ..
                } if cancelled_id == &order_id
            )
        }),
        "Cancel terminal should be suppressed after terminal fill"
    );
}

#[tokio::test]
async fn test_cancel_after_terminal_fill_does_not_emit_cancel_terminal() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 0,
        max_place_latency_ms: 0,
        seed: None,
        min_cancel_latency_ms: Some(1000),
        max_cancel_latency_ms: Some(1000),
        min_quote_lifetime_ms: None,
    };

    let executor = BacktestOrderExecutor::builder()
        .latency_config(Some(latency_config))
        .build();

    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();

    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let fill_tick =
        create_polymarket_trade_with_timestamp("asset123", TradeSide::Sell, 0.50, 200.0, 500);
    let fill_events = executor
        .handle_polymarket_trade(&fill_tick, &empty_inventory, f64::MAX)
        .await
        .unwrap();
    assert!(
        fill_events.iter().any(|event| {
            matches!(
                event,
                LimitOrderEvent::OrderPartiallyFilled {
                    order_id: filled_id,
                    remaining_size,
                    ..
                } if filled_id == &order_id && *remaining_size == 0.0
            )
        }),
        "Expected order to reach terminal fill before cancel request"
    );

    executor.cancel_order(&order_id).await.unwrap();

    let post_cancel_tick =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 2000);
    let post_cancel_events = executor
        .handle_polymarket_trade(&post_cancel_tick, &empty_inventory, f64::MAX)
        .await
        .unwrap();
    assert!(
        post_cancel_events.iter().all(|event| {
            !matches!(
                event,
                LimitOrderEvent::OrderCancelled {
                    order_id: cancelled_id,
                    ..
                } if cancelled_id == &order_id
            )
        }),
        "Cancel after terminal fill should not emit terminal cancel"
    );
}

#[tokio::test]
async fn test_coalescing_uses_latest_replacement_quote() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 0,
        max_place_latency_ms: 0,
        seed: None,
        min_cancel_latency_ms: Some(1000),
        max_cancel_latency_ms: Some(1000),
        min_quote_lifetime_ms: None,
    };

    let executor = BacktestOrderExecutor::builder()
        .latency_config(Some(latency_config))
        .build();

    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();

    let trade = create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 0);
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let _ = executor
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    executor.cancel_order(&order_id).await.unwrap();

    let replacement1 = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.60,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    executor.execute_limit_order(replacement1).await.unwrap();

    let replacement2 = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.55,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    executor.execute_limit_order(replacement2).await.unwrap();

    let advance_trade =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 1000);
    let events = executor
        .handle_polymarket_trade(&advance_trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    let placed_events: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            LimitOrderEvent::OrderPlaced { price, .. } => Some(*price),
            _ => None,
        })
        .collect();

    assert_eq!(placed_events.len(), 1);
    assert_eq!(placed_events[0], 0.55);
}

#[tokio::test]
async fn test_noop_replacement_on_live_quote_reuses_existing_order() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 0,
        max_place_latency_ms: 0,
        seed: None,
        min_cancel_latency_ms: Some(1000),
        max_cancel_latency_ms: Some(1000),
        min_quote_lifetime_ms: None,
    };

    let executor = BacktestOrderExecutor::builder()
        .latency_config(Some(latency_config))
        .build();

    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    let placed = executor.execute_limit_order(order).await.unwrap();
    let live_order_id = placed.order_id().unwrap().to_string();

    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let make_live_trade =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 0);
    let _ = executor
        .handle_polymarket_trade(&make_live_trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    let noop_replacement = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    let noop_result = executor
        .execute_limit_order(noop_replacement)
        .await
        .unwrap();
    assert_eq!(
        noop_result.order_id().unwrap(),
        live_order_id,
        "No-op replacement should keep the current live order id"
    );

    let advance_trade =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 1000);
    let events = executor
        .handle_polymarket_trade(&advance_trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    assert!(
        events
            .iter()
            .all(|event| !matches!(event, LimitOrderEvent::OrderCancelled { .. })),
        "No-op replacement should not trigger cancellation events"
    );
    assert!(
        events
            .iter()
            .all(|event| !matches!(event, LimitOrderEvent::OrderPlaced { .. })),
        "No-op replacement should not emit deferred placement events"
    );
    assert_eq!(executor.get_pending_orders().await.len(), 1);
}

#[tokio::test]
async fn test_converged_replacement_keeps_pending_cancel_and_requeues() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 0,
        max_place_latency_ms: 0,
        seed: None,
        min_cancel_latency_ms: Some(1000),
        max_cancel_latency_ms: Some(1000),
        min_quote_lifetime_ms: None,
    };

    let executor = BacktestOrderExecutor::builder()
        .latency_config(Some(latency_config))
        .build();

    let original = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    let placed = executor.execute_limit_order(original).await.unwrap();
    let live_order_id = placed.order_id().unwrap().to_string();

    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let make_live_trade =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 0);
    let _ = executor
        .handle_polymarket_trade(&make_live_trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    let transient_replacement = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.60,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    let transient_result = executor
        .execute_limit_order(transient_replacement)
        .await
        .unwrap();
    let replacement_order_id = transient_result.order_id().unwrap().to_string();

    let converged_replacement = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        None,
        None,
    );
    let converged_result = executor
        .execute_limit_order(converged_replacement)
        .await
        .unwrap();
    assert_eq!(
        converged_result.order_id().unwrap(),
        replacement_order_id,
        "Converged update should keep the already-scheduled replacement order id"
    );
    assert_ne!(
        converged_result.order_id().unwrap(),
        live_order_id,
        "Once canceling is armed, convergence must not restore the live order id"
    );

    let advance_trade =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 1000);
    let events = executor
        .handle_polymarket_trade(&advance_trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    assert!(
        events
            .iter()
            .any(|event| matches!(event, LimitOrderEvent::OrderCancelled { order_id, .. } if order_id == &live_order_id)),
        "Expected pending cancel to execute for the original live order"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            LimitOrderEvent::OrderPlaced {
                order_id,
                price,
                size,
                side,
                ..
            } if order_id == &replacement_order_id
                && *side == OrderSide::Buy
                && *price == 0.50
                && *size == 200.0
        )),
        "Expected converged quote to be re-placed after cancellation completes"
    );

    let pending_orders = executor.get_pending_orders().await;
    assert_eq!(pending_orders.len(), 1);
    assert_eq!(pending_orders[0].order_id, replacement_order_id);
    assert_eq!(pending_orders[0].price, 0.50);
    assert_eq!(pending_orders[0].remaining_size, 200.0);
}

#[tokio::test]
async fn test_repro_same_level_cancel_requeue_after_converged_replacement() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 0,
        max_place_latency_ms: 0,
        seed: None,
        min_cancel_latency_ms: Some(1000),
        max_cancel_latency_ms: Some(1000),
        min_quote_lifetime_ms: None,
    };

    let executor = BacktestOrderExecutor::builder()
        .latency_config(Some(latency_config))
        .build();

    // Step 1: Live bid Buy 7.5 @ 0.42.
    let original = Order::new_limit_buy(
        "asset123".to_string(),
        Some("market456".to_string()),
        3.15, // quote amount => 7.5 base at 0.42
        0.42,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        Some("sig-original".to_string()),
        None,
    );
    let placed = executor.execute_limit_order(original).await.unwrap();
    let live_order_id = placed.order_id().unwrap().to_string();

    // Advance clock to make the order live.
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let make_live_trade =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 0);
    let _ = executor
        .handle_polymarket_trade(&make_live_trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    // Step 2: New signal moves to 0.41, transition lane into Canceling with replacement.
    let moved = Order::new_limit_buy(
        "asset123".to_string(),
        Some("market456".to_string()),
        3.075, // quote amount => 7.5 base at 0.41
        0.41,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(100).unwrap(),
        Some("sig-move".to_string()),
        None,
    );
    let moved_result = executor.execute_limit_order(moved).await.unwrap();
    let replacement_order_id = moved_result.order_id().unwrap().to_string();

    // Step 3: Before cancel completes, signal converges back to 0.42 @ 7.5.
    let converged = Order::new_limit_buy(
        "asset123".to_string(),
        Some("market456".to_string()),
        3.15, // quote amount => 7.5 base at 0.42
        0.42,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(200).unwrap(),
        Some("sig-converged".to_string()),
        None,
    );
    let converged_result = executor.execute_limit_order(converged).await.unwrap();
    assert_eq!(
        converged_result.order_id().unwrap(),
        replacement_order_id,
        "Converged update should keep the already scheduled replacement order id"
    );
    assert_ne!(converged_result.order_id().unwrap(), live_order_id);

    // Step 4/5 assertion: cancel remains armed, so cancel+replace churn still executes.
    let advance_to_cancel_time =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 1100);
    let events = executor
        .handle_polymarket_trade(&advance_to_cancel_time, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    assert!(
        events
            .iter()
            .any(|event| matches!(event, LimitOrderEvent::OrderCancelled { order_id, .. } if order_id == &live_order_id)),
        "Repro: live order should still cancel once canceling is armed"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            LimitOrderEvent::OrderPlaced {
                order_id,
                price,
                size,
                side,
                ..
            } if order_id == &replacement_order_id
                && *side == OrderSide::Buy
                && *price == 0.42
                && *size == 7.5
        )),
        "Repro: replacement should be re-placed at converged level after cancel completes"
    );

    let pending_orders = executor.get_pending_orders().await;
    assert_eq!(pending_orders.len(), 1);
    assert_eq!(pending_orders[0].order_id, replacement_order_id);
    assert_eq!(pending_orders[0].price, 0.42);
    assert_eq!(pending_orders[0].remaining_size, 7.5);
}

#[tokio::test]
async fn test_repro_converged_replacement_with_tiny_size_drift_still_cancels_and_requeues() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 0,
        max_place_latency_ms: 0,
        seed: None,
        min_cancel_latency_ms: Some(1000),
        max_cancel_latency_ms: Some(1000),
        min_quote_lifetime_ms: None,
    };

    let executor = BacktestOrderExecutor::builder()
        .latency_config(Some(latency_config))
        .build();

    let original = Order::new_limit_buy(
        "asset123".to_string(),
        Some("market456".to_string()),
        3.15, // 7.5 @ 0.42
        0.42,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(0).unwrap(),
        Some("sig-original".to_string()),
        None,
    );
    let placed = executor.execute_limit_order(original).await.unwrap();
    let live_order_id = placed.order_id().unwrap().to_string();

    let empty_inventory: HashMap<String, f64> = HashMap::new();
    let make_live_trade =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 0);
    let _ = executor
        .handle_polymarket_trade(&make_live_trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    let moved = Order::new_limit_buy(
        "asset123".to_string(),
        Some("market456".to_string()),
        3.075, // 7.5 @ 0.41
        0.41,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(100).unwrap(),
        Some("sig-move".to_string()),
        None,
    );
    let moved_result = executor.execute_limit_order(moved).await.unwrap();
    let replacement_order_id = moved_result.order_id().unwrap().to_string();

    // Simulate downstream normalized quote amount with tiny numeric drift.
    // This corresponds to size 7.49999999 at 0.42, economically same 7.5 level.
    let converged_with_drift = Order::new_limit_buy(
        "asset123".to_string(),
        Some("market456".to_string()),
        3.149_999_995_8,
        0.42,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(200).unwrap(),
        Some("sig-converged".to_string()),
        None,
    );
    let converged_result = executor
        .execute_limit_order(converged_with_drift)
        .await
        .unwrap();

    // Once canceling is armed, convergence back (including tiny size drift) does not
    // unschedule cancellation; it only updates the pending replacement quote.
    assert_eq!(
        converged_result.order_id().unwrap(),
        replacement_order_id,
        "Expected converged quote with drift to reuse scheduled replacement id"
    );
    assert_ne!(converged_result.order_id().unwrap(), live_order_id);

    let advance_to_cancel_time =
        create_polymarket_trade_with_timestamp("other", TradeSide::Buy, 0.50, 1.0, 1100);
    let events = executor
        .handle_polymarket_trade(&advance_to_cancel_time, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    assert!(
        events
            .iter()
            .any(|event| matches!(event, LimitOrderEvent::OrderCancelled { order_id, .. } if order_id == &live_order_id)),
        "Expected live order cancellation to remain scheduled after convergence"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            LimitOrderEvent::OrderPlaced {
                order_id,
                price,
                size,
                side,
                ..
            } if order_id == &replacement_order_id
                && *side == OrderSide::Buy
                && *price == 0.42
                && (*size - 7.499_999_99).abs() < 1e-6
        )),
        "Expected converged quote with drift to be re-placed after cancellation"
    );
}

#[tokio::test]
async fn test_cancel_timestamp_uses_simulation_time() {
    let latency_config = LatencySimulationConfig {
        min_place_latency_ms: 0,
        max_place_latency_ms: 0,
        seed: None,
        min_cancel_latency_ms: Some(0),
        max_cancel_latency_ms: Some(0),
        min_quote_lifetime_ms: None,
    };

    let executor = BacktestOrderExecutor::builder()
        .latency_config(Some(latency_config))
        .build();

    let order = Order::new_limit_buy(
        "asset123".to_string(),
        None,
        100.0,
        0.50,
        TimeInForce::GoodTilCancelled,
        DateTime::from_timestamp_millis(1000).unwrap(),
        None,
        None,
    );
    let placed = executor.execute_limit_order(order).await.unwrap();
    let order_id = placed.order_id().unwrap().to_string();

    let cancel_event = executor.cancel_order(&order_id).await.unwrap();
    match cancel_event {
        LimitOrderEvent::OrderCancelled { timestamp, .. } => {
            assert_eq!(timestamp.timestamp_millis(), 1000);
        }
        _ => panic!("Expected OrderCancelled event"),
    }
}

#[tokio::test]
async fn test_multiple_orders_fill_from_same_trade() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    // Both orders should fill
    assert_eq!(fills.len(), 2);

    // Orders should be removed
    assert!(executor.get_pending_orders().await.is_empty());
}

#[tokio::test]
async fn test_trade_for_different_asset_no_fill() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    assert!(fills.is_empty());

    // Original order still pending
    assert_eq!(executor.get_pending_orders().await.len(), 1);
}

#[tokio::test]
async fn test_supports_limit_orders_returns_true() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();
    assert!(executor.supports_limit_orders());
}

#[tokio::test]
async fn test_execute_limit_order_with_non_limit_order_type_fails() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
    assert!(id1.starts_with("bt-order-"));
    assert!(id2.starts_with("bt-order-"));
}

// ============================================================================
// Inventory Constraint Tests
// ============================================================================

#[tokio::test]
async fn test_sell_order_blocked_with_no_inventory() {
    // Default executor enforces inventory constraints
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();
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
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

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
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
        .handle_polymarket_trade(&trade, &inventory, f64::MAX)
        .await
        .unwrap();

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
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
        .handle_polymarket_trade(&trade, &inventory, f64::MAX)
        .await
        .unwrap();

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
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    // Buy order should fill regardless of inventory
    assert_eq!(fills.len(), 1);
}

#[tokio::test]
async fn test_inventory_constraints_can_be_disabled() {
    // Create executor with inventory constraints disabled
    let executor = BacktestOrderExecutor::builder()
        .slippage(0.01)
        .enforce_inventory_constraints(false)
        .build();
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
        .handle_polymarket_trade(&trade, &empty_inventory, f64::MAX)
        .await
        .unwrap();

    // Should fill even with no inventory because constraints are disabled
    assert_eq!(
        fills.len(),
        1,
        "Sell order should fill when inventory constraints are disabled"
    );
}

#[tokio::test]
async fn test_multiple_sell_orders_respect_inventory() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
        .handle_polymarket_trade(&trade, &inventory, f64::MAX)
        .await
        .unwrap();

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
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();
    assert!(executor.supports_redemption());
}

#[tokio::test]
async fn test_execute_redemption_returns_completed_event() {
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();
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
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();

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
