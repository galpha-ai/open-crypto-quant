//! Integration tests for backtest event coordination.
//!
//! These tests verify the full event flow between the coordinator,
//! executor, and collector in a backtest scenario.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use popeyes_trading_types::{
    MarketDataEvent, OrderSummary, OrderbookSnapshotEvent, OrderbookSource, PolymarketTradeEvent,
    TradeSide,
};

use crate::{
    backtest::{BacktestEventCoordinator, BacktestTimeline},
    domain::SystemEvent,
    event_coordinator::{EventCollector, EventCoordinator, InMemoryEventCollector},
    execution::{
        OrderExecutor,
        backtest::BacktestOrderExecutor,
        events::{LimitOrderEvent, OrderSide, TimeInForce},
        order::{Order, OrderType},
    },
};

fn create_snapshot(timestamp: i64, asset_id: &str) -> OrderbookSnapshotEvent {
    OrderbookSnapshotEvent {
        asset_id: asset_id.to_string(),
        market: "test-market".to_string(),
        bids: vec![OrderSummary {
            price: 0.50,
            size: 100.0,
        }],
        asks: vec![OrderSummary {
            price: 0.55,
            size: 100.0,
        }],
        hash: String::new(),
        timestamp,
        source: OrderbookSource::Polymarket,
        market_metadata: None,
        observed_at: chrono::Utc::now(),
    }
}

fn create_trade(
    timestamp: i64,
    asset_id: &str,
    price: f64,
    side: TradeSide,
) -> PolymarketTradeEvent {
    PolymarketTradeEvent {
        asset_id: asset_id.to_string(),
        market: "test-market".to_string(),
        price,
        size: 50.0,
        side,
        timestamp,
        fee_rate_bps: 0,
        market_metadata: None,
        observed_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn test_event_coordination_flow() {
    // Setup: Create timeline with snapshots and trades
    let snapshots = vec![
        create_snapshot(1000, "asset1"),
        create_snapshot(2000, "asset1"),
    ];
    let trades = vec![
        create_trade(1100, "asset1", 0.48, TradeSide::Sell), // This sell trade can fill our bid
        create_trade(1500, "asset1", 0.52, TradeSide::Buy),  // This buy trade won't fill our bid
    ];

    let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();
    let coordinator = Arc::new(BacktestEventCoordinator::new(
        timeline,
        Duration::from_secs(3600),
    ));
    // Disable inventory constraints for this test since we don't track positions
    let executor = BacktestOrderExecutor::builder()
        .slippage(0.01)
        .enforce_inventory_constraints(false)
        .build();
    let collector = InMemoryEventCollector::new();

    // Process events
    let mut event_count = 0;
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    loop {
        let event = match coordinator.next_event().await {
            Ok(e) => e,
            Err(_) => break,
        };

        collector.record(&event);
        event_count += 1;

        // Process Polymarket trades through executor
        if let SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(ref trade)) = event {
            let fills = executor
                .handle_polymarket_trade(trade, &empty_inventory, f64::MAX)
                .await
                .unwrap();
            for fill in fills {
                collector.record(&SystemEvent::LimitOrder(fill.clone()));
                coordinator
                    .enqueue_event(SystemEvent::LimitOrder(fill))
                    .await
                    .unwrap();
            }
        }

        // Prevent infinite loop in tests
        if event_count > 100 {
            break;
        }
    }

    // Verify events were collected
    let events = collector.events();
    assert!(!events.is_empty(), "Should have collected events");

    // Verify we got snapshots
    let snapshot_count = events
        .iter()
        .filter(|e| e.event_type.contains("OrderbookSnapshot"))
        .count();
    assert_eq!(snapshot_count, 2, "Should have 2 snapshots");

    // Verify we got trades
    let trade_count = events
        .iter()
        .filter(|e| e.event_type.contains("PolymarketTrade"))
        .count();
    assert_eq!(trade_count, 2, "Should have 2 trades");
}

#[tokio::test]
async fn test_fill_events_enqueued_with_priority() {
    // Setup: Create minimal timeline
    let snapshots = vec![create_snapshot(1000, "asset1")];
    let trades = vec![
        create_trade(1100, "asset1", 0.48, TradeSide::Sell), // Will fill our bid
    ];

    let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();
    let coordinator = Arc::new(BacktestEventCoordinator::new(
        timeline,
        Duration::from_secs(3600),
    ));
    // Disable inventory constraints for this test since we don't track positions
    let executor = BacktestOrderExecutor::builder()
        .slippage(0.01)
        .enforce_inventory_constraints(false)
        .build();

    // Place a limit buy order at 0.50
    // Use a timestamp before the trade (t=1100) to ensure the order is eligible for fills
    let order_timestamp = chrono::DateTime::from_timestamp_millis(0).unwrap();
    let order = Order {
        mint: "asset1".to_string(),
        market: Some("test-market".to_string()),
        order_type: OrderType::LimitBuy {
            quote_amount: 100.0,
            limit_price: 0.50,
            time_in_force: TimeInForce::GoodTilCancelled,
        },
        price: Some(0.50),
        status: crate::execution::order::OrderStatus::Pending,
        timestamp: order_timestamp,
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    let placed = executor.execute_limit_order(order).await.unwrap();
    assert!(matches!(placed, LimitOrderEvent::OrderPlaced { .. }));

    // Get first event (trade) - trades are delivered before snapshot for causality
    let event1 = coordinator.next_event().await.unwrap();
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    if let SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(ref trade)) = event1 {
        let fills = executor
            .handle_polymarket_trade(trade, &empty_inventory, f64::MAX)
            .await
            .unwrap();

        // We should have a fill
        assert_eq!(fills.len(), 1, "Should have 1 fill");
        assert!(matches!(
            fills[0],
            LimitOrderEvent::OrderPartiallyFilled { .. }
        ));

        // Enqueue the fill event
        coordinator.enqueue_limit_order_events(fills);
    } else {
        panic!(
            "Expected PolymarketTrade event, got: {:?}",
            event1.event_type()
        );
    }

    // The next event should be the enqueued fill (priority 1)
    let event2 = coordinator.next_event().await.unwrap();
    assert!(
        matches!(
            event2,
            SystemEvent::LimitOrder(LimitOrderEvent::OrderPartiallyFilled { .. })
        ),
        "Expected enqueued fill event, got: {:?}",
        event2.event_type()
    );

    // Then the snapshot
    let event3 = coordinator.next_event().await.unwrap();
    assert!(
        matches!(
            event3,
            SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(_))
        ),
        "Expected snapshot, got: {:?}",
        event3.event_type()
    );
}

#[tokio::test]
async fn test_timer_events_fire_between_ticks() {
    // Setup: Create timeline spanning multiple timer intervals
    let snapshots = vec![
        create_snapshot(0, "asset1"),    // t=0s
        create_snapshot(5000, "asset1"), // t=5s
    ];
    let trades = vec![];

    let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();
    // Timer every 2 seconds
    let coordinator = BacktestEventCoordinator::new(timeline, Duration::from_secs(2));

    let mut events = Vec::new();
    for _ in 0..20 {
        match coordinator.next_event().await {
            Ok(e) => events.push(e),
            Err(_) => break,
        }
    }

    // Count event types
    let snapshot_count = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(_))
            )
        })
        .count();
    let timer_count = events
        .iter()
        .filter(|e| matches!(e, SystemEvent::Timer(_)))
        .count();

    assert_eq!(snapshot_count, 2, "Should have 2 snapshots");
    // Timer events should fire between snapshots (at t=2s, t=4s)
    assert!(
        timer_count >= 1,
        "Should have at least 1 timer event, got {}",
        timer_count
    );
}

#[tokio::test]
async fn test_no_more_events_when_exhausted() {
    let snapshots = vec![create_snapshot(1000, "asset1")];
    let trades = vec![];
    let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

    let coordinator = BacktestEventCoordinator::new(timeline, Duration::from_secs(3600));

    // Get the snapshot
    let _ = coordinator.next_event().await.unwrap();

    // Should return error
    let result = coordinator.next_event().await;
    assert!(result.is_err());

    // Subsequent calls should also return error
    let result2 = coordinator.next_event().await;
    assert!(result2.is_err());
}

/// Test that events are delivered in the expected tick-based order.
///
/// The coordinator delivers events with this priority within each tick:
/// 1. Trades for the tick (before snapshot, to allow fills against existing orders)
/// 2. Orderbook snapshot for the tick
///
/// This means trades with timestamps AFTER the snapshot's timestamp are delivered
/// BEFORE the snapshot. This is intentional for causality: trades represent what
/// happened against the orderbook state at that time, allowing fills before the
/// strategy reacts to the new snapshot.
#[tokio::test]
async fn test_event_delivery_order_within_ticks() {
    let snapshots = vec![
        create_snapshot(1000, "asset1"),
        create_snapshot(3000, "asset1"),
    ];
    let trades = vec![
        create_trade(1500, "asset1", 0.51, TradeSide::Buy),
        create_trade(2000, "asset1", 0.52, TradeSide::Sell),
    ];

    let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();
    let coordinator = BacktestEventCoordinator::new(timeline, Duration::from_secs(3600));

    let mut events_received = Vec::new();

    for _ in 0..10 {
        match coordinator.next_event().await {
            Ok(event) => {
                let event_info = match &event {
                    SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(s)) => {
                        format!("Snapshot(ts={})", s.timestamp)
                    }
                    SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(t)) => {
                        format!("Trade(ts={})", t.timestamp)
                    }
                    _ => "Other".to_string(),
                };
                events_received.push(event_info);
            }
            Err(_) => break,
        }
    }

    // Verify the expected order:
    // Tick 1: trades at 1500, 2000 (both belong to tick starting at 1000), then snapshot at 1000
    // Tick 2: snapshot at 3000 (no trades)
    assert_eq!(
        events_received.len(),
        4,
        "Should receive 4 events: {:?}",
        events_received
    );
    assert_eq!(events_received[0], "Trade(ts=1500)");
    assert_eq!(events_received[1], "Trade(ts=2000)");
    assert_eq!(events_received[2], "Snapshot(ts=1000)");
    assert_eq!(events_received[3], "Snapshot(ts=3000)");
}

#[tokio::test]
async fn test_collector_records_all_event_types() {
    let snapshots = vec![create_snapshot(1000, "asset1")];
    let trades = vec![create_trade(1100, "asset1", 0.51, TradeSide::Buy)];

    let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();
    let coordinator = BacktestEventCoordinator::new(timeline, Duration::from_secs(1));
    let collector = InMemoryEventCollector::new();

    // Collect all events
    for _ in 0..10 {
        match coordinator.next_event().await {
            Ok(event) => collector.record(&event),
            Err(_) => break,
        }
    }

    let events = collector.events();

    // Verify we captured different event types
    let has_snapshot = events
        .iter()
        .any(|e| e.event_type.contains("OrderbookSnapshot"));
    let has_trade = events
        .iter()
        .any(|e| e.event_type.contains("PolymarketTrade"));

    assert!(has_snapshot, "Should have recorded snapshot event");
    assert!(has_trade, "Should have recorded trade event");

    // Verify all events have valid JSON data
    for event in &events {
        assert!(!event.timestamp.is_empty(), "Event should have timestamp");
        assert!(!event.event_type.is_empty(), "Event should have type");
        assert!(
            event.data.is_object() || event.data.is_null(),
            "Event data should be JSON object"
        );
    }
}

/// Test that sell limit orders are filled when a BUY trade crosses at or above the ask price.
///
/// This tests the opposite case from `test_fill_events_enqueued_with_priority`:
/// - Place a limit SELL order at 0.55
/// - A BUY trade at 0.57 should fill it (crosses our ask)
#[tokio::test]
async fn test_sell_limit_order_fill() {
    // Setup: Create minimal timeline
    let snapshots = vec![create_snapshot(1000, "asset1")];
    let trades = vec![
        create_trade(1100, "asset1", 0.57, TradeSide::Buy), // Will fill our ask at 0.55
    ];

    let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();
    let coordinator = Arc::new(BacktestEventCoordinator::new(
        timeline,
        Duration::from_secs(3600),
    ));
    // Disable inventory constraints for this test since we don't track positions
    let executor = BacktestOrderExecutor::builder()
        .slippage(0.01)
        .enforce_inventory_constraints(false)
        .build();

    // Place a limit sell order at 0.55
    // Use a timestamp before the trade (t=1100) to ensure the order is eligible for fills
    let order_timestamp = chrono::DateTime::from_timestamp_millis(0).unwrap();
    let order = Order {
        mint: "asset1".to_string(),
        market: Some("test-market".to_string()),
        order_type: OrderType::LimitSell {
            token_amount: 50.0,
            limit_price: 0.55,
            time_in_force: TimeInForce::GoodTilCancelled,
            clear_position: false,
        },
        price: Some(0.55),
        status: crate::execution::order::OrderStatus::Pending,
        timestamp: order_timestamp,
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    let placed = executor.execute_limit_order(order).await.unwrap();
    assert!(matches!(placed, LimitOrderEvent::OrderPlaced { .. }));

    // Verify the order was placed correctly
    if let LimitOrderEvent::OrderPlaced {
        side, price, size, ..
    } = &placed
    {
        assert_eq!(*side, OrderSide::Sell, "Order should be a sell order");
        assert!((price - 0.55).abs() < 0.001, "Price should be 0.55");
        assert!((size - 50.0).abs() < 0.001, "Size should be 50.0");
    }

    // Get first event (trade) - trades are delivered before snapshot for causality
    let event1 = coordinator.next_event().await.unwrap();
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    if let SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(ref trade)) = event1 {
        // Verify the trade is a BUY at 0.57
        assert_eq!(trade.side, TradeSide::Buy, "Trade should be a buy");
        assert!(
            (trade.price - 0.57).abs() < 0.001,
            "Trade price should be 0.57"
        );

        let fills = executor
            .handle_polymarket_trade(trade, &empty_inventory, f64::MAX)
            .await
            .unwrap();

        // We should have a fill - BUY trade at 0.57 should fill our SELL order at 0.55
        assert_eq!(fills.len(), 1, "Should have 1 fill for sell limit order");

        // Verify the fill details
        match &fills[0] {
            LimitOrderEvent::OrderPartiallyFilled {
                side,
                filled_size,
                fill_price,
                remaining_size,
                ..
            } => {
                assert_eq!(*side, OrderSide::Sell, "Fill should be for sell order");
                assert!(
                    (filled_size - 50.0).abs() < 0.001,
                    "Filled size should be 50.0"
                );
                assert!(
                    (fill_price - 0.55).abs() < 0.001,
                    "Fill price should be at our limit (0.55)"
                );
                assert!(
                    (remaining_size - 0.0).abs() < 0.001,
                    "Remaining size should be 0 (fully filled)"
                );
            }
            other => panic!("Expected OrderPartiallyFilled event, got: {:?}", other),
        }

        // Enqueue the fill event
        coordinator.enqueue_limit_order_events(fills);
    } else {
        panic!(
            "Expected PolymarketTrade event, got: {:?}",
            event1.event_type()
        );
    }

    // The next event should be the enqueued fill (priority 1)
    let event2 = coordinator.next_event().await.unwrap();
    assert!(
        matches!(
            event2,
            SystemEvent::LimitOrder(LimitOrderEvent::OrderPartiallyFilled { .. })
        ),
        "Expected enqueued fill event, got: {:?}",
        event2.event_type()
    );

    // Then the snapshot
    let event3 = coordinator.next_event().await.unwrap();
    assert!(
        matches!(
            event3,
            SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(_))
        ),
        "Expected snapshot, got: {:?}",
        event3.event_type()
    );
}

/// Test sell limit order fill with inventory constraints enabled (realistic scenario).
///
/// When `enforce_inventory_constraints` is true (default), sell orders can only
/// fill if there is positive inventory of the asset.
#[tokio::test]
async fn test_sell_limit_order_fill_with_inventory_constraint() {
    // Setup: Create minimal timeline
    let snapshots = vec![create_snapshot(1000, "asset1")];
    let trades = vec![
        create_trade(1100, "asset1", 0.57, TradeSide::Buy), // Would fill our ask at 0.55
    ];

    let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();
    let coordinator = Arc::new(BacktestEventCoordinator::new(
        timeline,
        Duration::from_secs(3600),
    ));
    // Enable inventory constraints (default behavior)
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build();
    assert!(
        executor.enforces_inventory_constraints(),
        "Executor should enforce inventory constraints"
    );

    // Place a limit sell order at 0.55
    let order = Order {
        mint: "asset1".to_string(),
        market: Some("test-market".to_string()),
        order_type: OrderType::LimitSell {
            token_amount: 50.0,
            limit_price: 0.55,
            time_in_force: TimeInForce::GoodTilCancelled,
            clear_position: false,
        },
        price: Some(0.55),
        status: crate::execution::order::OrderStatus::Pending,
        // Use a timestamp before the trade (t=1100) to ensure the order is eligible for fills
        timestamp: chrono::DateTime::from_timestamp_millis(0).unwrap(),
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    let placed = executor.execute_limit_order(order).await.unwrap();
    assert!(matches!(placed, LimitOrderEvent::OrderPlaced { .. }));

    // Get first event (trade) - trades are delivered before snapshot for causality
    let event1 = coordinator.next_event().await.unwrap();

    // Test case 1: No inventory - sell should NOT fill
    let empty_inventory: HashMap<String, f64> = HashMap::new();
    if let SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(ref trade)) = event1 {
        let fills = executor
            .handle_polymarket_trade(trade, &empty_inventory, f64::MAX)
            .await
            .unwrap();
        assert_eq!(
            fills.len(),
            0,
            "Should NOT fill sell order when no inventory available"
        );
    } else {
        panic!(
            "Expected PolymarketTrade event, got: {:?}",
            event1.event_type()
        );
    }

    // Test case 2: With inventory - sell should fill
    // Create a new executor and order since the previous order is still pending
    let executor2 = BacktestOrderExecutor::builder().slippage(0.01).build();
    let order2 = Order {
        mint: "asset1".to_string(),
        market: Some("test-market".to_string()),
        order_type: OrderType::LimitSell {
            token_amount: 50.0,
            limit_price: 0.55,
            time_in_force: TimeInForce::GoodTilCancelled,
            clear_position: false,
        },
        price: Some(0.55),
        status: crate::execution::order::OrderStatus::Pending,
        // Use a timestamp before the trade (t=1100) to ensure the order is eligible for fills
        timestamp: chrono::DateTime::from_timestamp_millis(0).unwrap(),
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    executor2.execute_limit_order(order2).await.unwrap();

    // Provide inventory
    let mut inventory_with_position: HashMap<String, f64> = HashMap::new();
    inventory_with_position.insert("asset1".to_string(), 100.0); // We own 100 units

    if let SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(ref trade)) = event1 {
        let fills = executor2
            .handle_polymarket_trade(trade, &inventory_with_position, f64::MAX)
            .await
            .unwrap();
        assert_eq!(
            fills.len(),
            1,
            "Should fill sell order when inventory is available"
        );

        // Verify fill details
        match &fills[0] {
            LimitOrderEvent::OrderPartiallyFilled {
                side, filled_size, ..
            } => {
                assert_eq!(*side, OrderSide::Sell, "Fill should be for sell order");
                assert!(
                    (filled_size - 50.0).abs() < 0.001,
                    "Filled size should be 50.0"
                );
            }
            other => panic!("Expected OrderPartiallyFilled, got: {:?}", other),
        }
    }
}

/// Test that sell limit orders respect partial inventory constraints.
///
/// If we have 30 units but want to sell 50, we should only fill 30.
#[tokio::test]
async fn test_sell_limit_order_partial_fill_due_to_inventory() {
    let snapshots = vec![create_snapshot(1000, "asset1")];
    let trades = vec![
        create_trade(1100, "asset1", 0.57, TradeSide::Buy), // Would fill our ask at 0.55
    ];

    let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();
    let _coordinator = BacktestEventCoordinator::new(timeline, Duration::from_secs(3600));
    let executor = BacktestOrderExecutor::builder().slippage(0.01).build(); // Inventory constraints enabled

    // Place a limit sell order for 50 units at 0.55
    // Use a timestamp before the trade (t=1100) to ensure the order is eligible for fills
    let order = Order {
        mint: "asset1".to_string(),
        market: Some("test-market".to_string()),
        order_type: OrderType::LimitSell {
            token_amount: 50.0,
            limit_price: 0.55,
            time_in_force: TimeInForce::GoodTilCancelled,
            clear_position: false,
        },
        price: Some(0.55),
        status: crate::execution::order::OrderStatus::Pending,
        timestamp: chrono::DateTime::from_timestamp_millis(0).unwrap(),
        signal_slot: None,
        signal_id: None,
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: None,
    };
    executor.execute_limit_order(order).await.unwrap();

    // We only have 30 units
    let mut inventory: HashMap<String, f64> = HashMap::new();
    inventory.insert("asset1".to_string(), 30.0);

    let trade = create_trade(1100, "asset1", 0.57, TradeSide::Buy);
    let fills = executor
        .handle_polymarket_trade(&trade, &inventory, f64::MAX)
        .await
        .unwrap();

    assert_eq!(fills.len(), 1, "Should have 1 fill");

    match &fills[0] {
        LimitOrderEvent::OrderPartiallyFilled {
            filled_size,
            remaining_size,
            ..
        } => {
            // Should only fill 30 (our inventory), not 50 (order size)
            assert!(
                (filled_size - 30.0).abs() < 0.001,
                "Filled size should be 30.0 (limited by inventory)"
            );
            assert!(
                (remaining_size - 20.0).abs() < 0.001,
                "Remaining size should be 20.0"
            );
        }
        other => panic!("Expected OrderPartiallyFilled, got: {:?}", other),
    }

    // Verify the order is still pending with 20 remaining
    let pending = executor.get_pending_orders().await;
    assert_eq!(pending.len(), 1, "Order should still be pending");
    assert!(
        (pending[0].remaining_size - 20.0).abs() < 0.001,
        "Order should have 20 remaining"
    );
}
