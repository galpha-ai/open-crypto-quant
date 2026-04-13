use std::{any::Any, option::Option, sync::Arc};

use anyhow::Result;
use chrono::{DateTime, Utc};
use float_cmp::approx_eq;
use prometheus::Registry;
use solana_sdk::signature::Signature;
use tracing::debug;
use tracing_subscriber::EnvFilter;

use crate::{
    domain::TimerEvent,
    execution::{ExecutionEvent, LimitOrderEvent, OrderSide, OrderType},
    notifier::Notifiable,
    position::{
        ConfigurableExitStrategy, ExitStrategy, InMemoryPositionManager, PositionEvent,
        PositionManager, errors::PositionError, pending_order::PendingLimitOrder,
    },
    signal::TradableSignal,
};

#[derive(Debug, Clone)]
struct MockSignal {
    mint: Option<String>,
    price: Option<f64>,
    timestamp: Option<DateTime<Utc>>,
    slot: Option<u64>,
}

impl TradableSignal for MockSignal {
    fn signal_id(&self) -> &str {
        "id"
    }

    fn signal_type(&self) -> &str {
        "mock"
    }

    fn get_mint(&self) -> Option<&str> {
        self.mint.as_deref()
    }

    fn get_price(&self) -> Option<f64> {
        self.price
    }

    fn get_timestamp(&self) -> Option<DateTime<Utc>> {
        self.timestamp
    }

    fn get_slot(&self) -> Option<u64> {
        self.slot
    }

    fn as_notifiable(&self) -> Option<Box<dyn Notifiable + Send + Sync>> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_creator(&self) -> Option<solana_sdk::pubkey::Pubkey> {
        None
    }

    fn passes_filter(&self) -> bool {
        true // Mock signals always pass filter for tests
    }

    fn to_json(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({
            "signal_id": self.signal_id(),
            "signal_type": self.signal_type(),
            "mint": self.mint,
            "price": self.price,
            "timestamp": self.timestamp,
            "slot": self.slot
        }))
    }
}

fn create_test_exit_strategy() -> Arc<dyn ExitStrategy> {
    Arc::new(ConfigurableExitStrategy::new(
        0.2,                         // 20% take profit
        0.1,                         // 10% stop loss
        chrono::Duration::hours(24), // 24 hour max holding period
        10,                          // max sell failures
    ))
}

#[tokio::test]
async fn test_initial_position_creation() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        10.0,
        chrono::Duration::hours(24), // 24 hour max holding period
        10,                          // max_open_positions
        &registry,
        exit_strategy,
    );

    // Test initial position creation
    let event = ExecutionEvent::OrderFilled {
        mint: "TESTMINT".to_string(),
        token_amount_change: 100.0,
        quote_amount_change: None,
        price: Some(1.0),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };

    let result = manager.handle_execution(&event).await?;
    match result {
        PositionEvent::PositionCreated {
            position,
            available_quote,
        } => {
            assert_eq!(position.mint, "TESTMINT");
            assert_eq!(position.amount, 100.0);
            assert_eq!(position.entry_price, Some(1.0));
            assert_eq!(position.current_price, Some(1.0));
            assert_eq!(position.pnl_pct, Some(0.0));
            // Verify available_quote is included (initial 10.0 - some spent)
            assert!(available_quote <= 10.0);
        }
        PositionEvent::PositionUpdated { .. } => panic!("Unexpected PositionUpdated event"),
        PositionEvent::PositionClosed { .. } => panic!("Unexpected PositionClosed event"),
    }

    Ok(())
}

#[tokio::test]
async fn test_try_mark_pending_sell() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        10.0,
        chrono::Duration::hours(24), // 24 hour max holding period
        10,                          // max_open_positions
        &registry,
        exit_strategy,
    );

    // Should successfully mark a new position as pending sell
    let result = manager.try_mark_pending_sell("TESTMINT").await;
    assert!(result, "Expected to successfully mark as pending sell");
    assert!(
        manager.is_pending_sell("TESTMINT"),
        "Position should be marked as pending sell"
    );

    // Should fail to mark the same position again
    let result = manager.try_mark_pending_sell("TESTMINT").await;
    assert!(
        !result,
        "Expected to fail when marking the same position again"
    );

    // Should successfully mark a different position
    let result = manager.try_mark_pending_sell("ANOTHERMINT").await;
    assert!(result, "Expected to successfully mark different position");
    assert!(
        manager.is_pending_sell("ANOTHERMINT"),
        "Position should be marked as pending sell"
    );

    Ok(())
}

#[tokio::test]
async fn test_position_average_price_calculation() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        10.0,
        chrono::Duration::hours(24), // 24 hour max holding period
        10,                          // max_open_positions
        &registry,
        exit_strategy,
    );

    // First buy
    let event1 = ExecutionEvent::OrderFilled {
        mint: "TESTMINT".to_string(),
        token_amount_change: 100.0,
        quote_amount_change: None,
        price: Some(1.0),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&event1).await?;

    // Second buy at different price
    let event2 = ExecutionEvent::OrderFilled {
        mint: "TESTMINT".to_string(),
        token_amount_change: 200.0,
        quote_amount_change: None,
        price: Some(2.0),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };

    let result = manager.handle_execution(&event2).await?;
    match result {
        PositionEvent::PositionUpdated {
            position,
            source: _,
            available_quote: _,
        } => {
            assert_eq!(position.mint, "TESTMINT");
            assert_eq!(position.amount, 300.0);
            assert_eq!(position.entry_price, Some(1.6666666666666667)); // (100*1 + 200*2)/300
            assert_eq!(position.current_price, Some(2.0));
            // Use approx_eq! for floating point comparison with epsilon
            assert!(approx_eq!(
                f64,
                position.pnl_pct.unwrap(),
                20.0,
                epsilon = 0.0001
            ));
        }
        PositionEvent::PositionCreated { .. } => panic!("Unexpected PositionCreated event"),
        PositionEvent::PositionClosed { .. } => panic!("Unexpected PositionClosed event"),
    }

    Ok(())
}

#[tokio::test]
async fn test_position_price_update() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        10.0,
        chrono::Duration::hours(24), // 24 hour max holding period
        10,                          // max_open_positions
        &registry,
        exit_strategy,
    );

    // Create initial position
    let event = ExecutionEvent::OrderFilled {
        mint: "TESTMINT".to_string(),
        token_amount_change: 100.0,
        quote_amount_change: None,
        price: Some(1.0),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&event).await?;

    // Update price
    let update_result = manager.update_price("TESTMINT", 1.5, Utc::now()).await?;
    match update_result {
        PositionEvent::PositionUpdated {
            position,
            source: _,
            available_quote: _,
        } => {
            assert_eq!(position.mint, "TESTMINT");
            assert_eq!(position.amount, 100.0);
            assert_eq!(position.entry_price, Some(1.0));
            assert_eq!(position.current_price, Some(1.5));
            assert_eq!(position.pnl_pct, Some(50.0)); // ((1.5 - 1.0)/1.0)*100
        }
        PositionEvent::PositionCreated { .. } => panic!("Unexpected PositionCreated event"),
        PositionEvent::PositionClosed { .. } => panic!("Unexpected PositionClosed event"),
    }

    Ok(())
}

#[tokio::test]
async fn test_position_sell_reduces_amount() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        10.0,
        chrono::Duration::hours(24), // 24 hour max holding period
        10,                          // max_open_positions
        &registry,
        exit_strategy,
    );

    // Initial buy
    let buy_event = ExecutionEvent::OrderFilled {
        mint: "TESTMINT".to_string(),
        token_amount_change: 100.0,
        quote_amount_change: None,
        price: Some(1.0),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&buy_event).await?;

    // Sell half
    let sell_event = ExecutionEvent::OrderFilled {
        mint: "TESTMINT".to_string(),
        token_amount_change: -50.0,
        quote_amount_change: None,
        price: Some(1.5),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };

    let result = manager.handle_execution(&sell_event).await?;
    match result {
        PositionEvent::PositionUpdated {
            position,
            source: _,
            available_quote: _,
        } => {
            assert_eq!(position.mint, "TESTMINT");
            assert_eq!(position.amount, 50.0);
            assert_eq!(position.entry_price, Some(1.0)); // Entry price shouldn't change
            assert_eq!(position.current_price, Some(1.5));
            assert_eq!(position.pnl_pct, Some(50.0)); // ((1.5 - 1.0)/1.0)*100
        }
        PositionEvent::PositionCreated { .. } => panic!("Unexpected PositionCreated event"),
        PositionEvent::PositionClosed { .. } => panic!("Unexpected PositionClosed event"),
    }

    Ok(())
}

#[tokio::test]
async fn test_position_closing() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        10.0,
        chrono::Duration::hours(24), // 24 hour max holding period
        10,                          // max_open_positions
        &registry,
        exit_strategy,
    );

    // Initial buy
    let buy_event = ExecutionEvent::OrderFilled {
        mint: "TESTMINT".to_string(),
        token_amount_change: 100.0,
        quote_amount_change: None,
        price: Some(1.0),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&buy_event).await?;

    // Sell all
    let sell_event = ExecutionEvent::OrderFilled {
        mint: "TESTMINT".to_string(),
        token_amount_change: -100.0,
        quote_amount_change: None,
        price: Some(1.5),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: true,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };

    let result = manager.handle_execution(&sell_event).await?;
    match result {
        PositionEvent::PositionClosed {
            position,
            realized_pnl_sol,
            pnl_pct,
            available_quote,
            ..
        } => {
            assert_eq!(position.mint, "TESTMINT");
            assert_eq!(realized_pnl_sol, Some(50.0));
            assert_eq!(pnl_pct, Some(50.0));
            // available_quote should be returned
            assert!(available_quote >= 0.0);
        }
        PositionEvent::PositionCreated { .. } => panic!("Unexpected PositionCreated event"),
        PositionEvent::PositionUpdated { .. } => panic!("Unexpected PositionUpdated event"),
    }

    // Verify position is removed
    assert!(manager.get_position("TESTMINT").await.is_none());

    Ok(())
}

#[tokio::test]
async fn test_timer_handling_with_various_ages() -> Result<()> {
    let _guard = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        10.0,
        chrono::Duration::hours(24), // 24 hour max holding period
        10,                          // max_open_positions
        &registry,
        exit_strategy,
    );
    let now = Utc::now();

    // Create position with entry time 23 hours ago
    let event = ExecutionEvent::OrderFilled {
        mint: "TESTMINT".to_string(),
        token_amount_change: 100.0,
        quote_amount_change: None,
        price: Some(1.0),
        timestamp: now - chrono::Duration::hours(23),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&event).await?;

    // Timer event at 23.5 hours - should not trigger sell
    let timer_event = TimerEvent {
        timestamp: now + chrono::Duration::minutes(30),
    };
    debug!("Testing timer event at 23.5 hours (before max holding period)");
    let position = manager.get_position("TESTMINT").await.unwrap();
    debug!(
        "Position age: {:?}",
        timer_event
            .timestamp
            .signed_duration_since(position.entry_time)
    );
    debug!("Max holding period: 24 hours");
    let orders = manager.handle_timer(&timer_event).await?;
    debug!("Orders generated: {:?}", orders);
    assert!(
        orders.is_empty(),
        "Expected no orders but got {} orders",
        orders.len()
    );

    // Timer event at 24.5 hours after position creation - should trigger sell
    let timer_event = TimerEvent {
        timestamp: now + chrono::Duration::minutes(90),
    };
    debug!("Testing timer event at 24.5 hours (after max holding period)");
    let position = manager.get_position("TESTMINT").await.unwrap();
    debug!(
        "Position age: {:?}",
        timer_event
            .timestamp
            .signed_duration_since(position.entry_time)
    );
    debug!("Max holding period: 24 hours");
    debug!(
        "Pending sells before timer: {:?}",
        manager.get_pending_sells()
    );
    let orders = manager.handle_timer(&timer_event).await?;
    debug!("Orders generated: {:?}", orders);
    debug!(
        "Pending sells after timer: {:?}",
        manager.get_pending_sells()
    );
    assert_eq!(
        orders.len(),
        1,
        "Expected 1 order but got {} orders",
        orders.len()
    );
    assert_eq!(orders[0].mint, "TESTMINT");
    match &orders[0].order_type {
        OrderType::MarketSell {
            token_amount,
            clear_position: _,
        } => assert_eq!(*token_amount, 100.0),
        _ => panic!("Expected sell order"),
    }

    Ok(())
}

#[tokio::test]
async fn test_duplicate_timer_event_handling() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        10.0,
        chrono::Duration::hours(24), // 24 hour max holding period
        10,                          // max_open_positions
        &registry,
        exit_strategy,
    );
    let now = Utc::now();

    // Create position with entry time 25 hours ago
    let event = ExecutionEvent::OrderFilled {
        mint: "TESTMINT".to_string(),
        token_amount_change: 100.0,
        quote_amount_change: None,
        price: Some(1.0),
        timestamp: now - chrono::Duration::hours(25),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&event).await?;

    // First timer event - should trigger sell
    let timer_event = TimerEvent { timestamp: now };
    let orders = manager.handle_timer(&timer_event).await?;
    assert_eq!(orders.len(), 1);

    // Manually mark as pending sell as the position_handler would do
    manager.try_mark_pending_sell("TESTMINT").await;

    // Second timer event - should not trigger another sell
    let orders = manager.handle_timer(&timer_event).await?;
    assert!(orders.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_pending_sell_tracking() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        10.0,
        chrono::Duration::hours(24), // 24 hour max holding period
        10,                          // max_open_positions
        &registry,
        exit_strategy,
    );
    let now = Utc::now();

    // Create position with entry time 25 hours ago
    let execution_event = ExecutionEvent::OrderFilled {
        mint: "TESTMINT".to_string(),
        token_amount_change: 100.0,
        quote_amount_change: None,
        price: Some(1.0),
        timestamp: now - chrono::Duration::hours(25),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&execution_event).await?;

    // Timer event - should trigger sell and mark as pending
    let timer_event = TimerEvent { timestamp: now };
    let orders = manager.handle_timer(&timer_event).await?;
    assert_eq!(orders.len(), 1);

    // Manually mark as pending sell as the position_handler would do
    manager.try_mark_pending_sell("TESTMINT").await;
    assert!(manager.is_pending_sell("TESTMINT"));

    // Simulate order completion
    let fill_event = ExecutionEvent::OrderFilled {
        mint: "TESTMINT".to_string(),
        token_amount_change: -100.0,
        quote_amount_change: None,
        price: Some(1.0),
        timestamp: now,
        slippage: None,
        clear_position: true,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    debug!(
        "Before handling execution - pending sells: {:?}",
        manager.get_pending_sells()
    );
    debug!(
        "Current position: {:?}",
        manager.get_position("TESTMINT").await
    );
    let result = manager.handle_execution(&fill_event).await?;
    debug!(
        "After handling execution - pending sells: {:?}",
        manager.get_pending_sells()
    );
    debug!("Execution result: {:?}", result);
    debug!(
        "Updated position: {:?}",
        manager.get_position("TESTMINT").await
    );

    // Verify pending sell is cleared
    assert!(
        !manager.is_pending_sell("TESTMINT"),
        "Pending sell flag not cleared for TESTMINT. Current pending sells: {:?}",
        manager.get_pending_sells()
    );

    Ok(())
}

#[tokio::test]
async fn test_reject_signal_when_max_positions_reached() -> Result<()> {
    let registry = Registry::new();
    let max_positions = 1;
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        10.0,
        chrono::Duration::hours(24), // 24 hour max holding period
        max_positions,
        &registry,
        exit_strategy,
    );

    // Create first position
    let signal1 = MockSignal {
        mint: Some("MINT1".to_string()),
        price: Some(1.0),
        timestamp: Some(Utc::now()),
        slot: Some(1000),
    };

    let order1_result = manager.handle_signal(&signal1).await;
    assert!(order1_result.is_ok());
    assert!(order1_result.as_ref().unwrap().is_some());

    let fill_event1 = ExecutionEvent::OrderFilled {
        mint: "MINT1".to_string(),
        token_amount_change: 1.0,
        quote_amount_change: Some(-0.1),
        price: Some(1.0),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&fill_event1).await?;

    assert_eq!(manager.get_open_position_count().await, max_positions);
    assert!(manager.get_position("MINT1").await.is_some());

    // Attempt to create second position
    let signal2 = MockSignal {
        mint: Some("MINT2".to_string()),
        price: Some(2.0),
        timestamp: Some(Utc::now()),
        slot: Some(1001),
    };

    let order2_result = manager.handle_signal(&signal2).await;
    assert!(order2_result.is_err());

    match order2_result {
        Err(PositionError::MaxOpenPositionsReached(limit)) => {
            assert_eq!(limit, max_positions);
        }
        _ => panic!("Expected MaxOpenPositionsReached error"),
    }

    assert_eq!(manager.get_open_position_count().await, max_positions);
    assert!(manager.get_position("MINT2").await.is_none());

    Ok(())
}

#[tokio::test]
async fn test_rejected_order_does_not_update_position() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        10.0,
        chrono::Duration::hours(24), // 24 hour max holding period
        10,                          // max_open_positions
        &registry,
        exit_strategy,
    );

    // Rejected order
    let event = ExecutionEvent::OrderRejected {
        mint: "TESTMINT".to_string(),
        reason: "test".to_string(),
    };

    let result = manager.handle_execution(&event).await;
    assert!(result.is_err());

    // Verify no position was created
    assert!(manager.get_position("TESTMINT").await.is_none());

    Ok(())
}

#[tokio::test]
async fn test_add_orphan_position() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        10.0,
        chrono::Duration::hours(24), // 24 hour max holding period
        10,                          // max_open_positions
        &registry,
        exit_strategy,
    );

    // Add orphan position
    let mint = "ORPHANMINT".to_string();
    let amount = 100u64;
    manager.add_orphan_position(mint.clone(), amount).await?;

    // Verify position was created
    let position = manager.get_position(&mint).await;
    assert!(position.is_some());

    let position = position.unwrap();
    assert_eq!(position.mint, mint);
    assert_eq!(position.amount, amount as f64);
    assert_eq!(position.entry_price, None); // Orphan positions have no entry price
    assert_eq!(position.current_price, None); // Orphan positions have no current price
    assert_eq!(position.pnl_pct, None); // Orphan positions have no PnL
    assert_eq!(position.entry_slot, 0); // Orphan positions have entry slot 0
    assert_eq!(position.sell_failure_count, 0);

    // Verify mint is marked as bought to prevent repeat buys
    assert!(!manager.try_mark_for_buying(&mint).await);

    Ok(())
}

#[tokio::test]
async fn test_cash_flow_tracking() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        100.0, // Starting balance
        chrono::Duration::hours(24),
        10,
        &registry,
        exit_strategy,
    );

    // Initial state
    assert!((manager.get_available_quote().await - 100.0).abs() < 1e-9);
    assert_eq!(manager.get_total_quote_spent().await, 0.0);
    assert_eq!(manager.get_total_quote_received().await, 0.0);
    assert_eq!(manager.get_net_cash_flow().await, 0.0);

    // Buy 10 tokens at price 1.0 (10 quote spent)
    let buy_event = ExecutionEvent::OrderFilled {
        mint: "TEST".to_string(),
        token_amount_change: 10.0,
        quote_amount_change: Some(10.0),
        price: Some(1.0),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&buy_event).await?;

    assert!((manager.get_available_quote().await - 90.0).abs() < 1e-9);
    assert!((manager.get_total_quote_spent().await - 10.0).abs() < 1e-9);
    assert_eq!(manager.get_total_quote_received().await, 0.0);
    assert!((manager.get_net_cash_flow().await - (-10.0)).abs() < 1e-9);

    // Sell 10 tokens at price 1.2 (12 quote received)
    let sell_event = ExecutionEvent::OrderFilled {
        mint: "TEST".to_string(),
        token_amount_change: -10.0,
        quote_amount_change: Some(12.0),
        price: Some(1.2),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: true,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&sell_event).await?;

    assert!((manager.get_available_quote().await - 102.0).abs() < 1e-9);
    assert!((manager.get_total_quote_spent().await - 10.0).abs() < 1e-9);
    assert!((manager.get_total_quote_received().await - 12.0).abs() < 1e-9);
    // Net cash flow = 12 - 10 = 2 profit
    assert!((manager.get_net_cash_flow().await - 2.0).abs() < 1e-9);

    Ok(())
}

#[tokio::test]
async fn test_total_pnl_with_inventory() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        100.0,
        chrono::Duration::hours(24),
        10,
        &registry,
        exit_strategy,
    );

    // Buy 10 tokens at price 1.0 (10 quote spent)
    let buy_event = ExecutionEvent::OrderFilled {
        mint: "TEST".to_string(),
        token_amount_change: 10.0,
        quote_amount_change: Some(10.0),
        price: Some(1.0),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&buy_event).await?;

    // Net cash flow = -10 (we spent 10, received 0)
    assert!((manager.get_net_cash_flow().await - (-10.0)).abs() < 1e-9);

    // Position has 10 tokens at current price 1.0
    // Inventory MTM = 10 * 1.0 = 10
    // Total PnL = -10 + 10 = 0
    assert!((manager.get_total_pnl().await - 0.0).abs() < 1e-9);

    // Update price to 1.5
    manager.update_price("TEST", 1.5, Utc::now()).await?;

    // Inventory MTM = 10 * 1.5 = 15
    // Total PnL = -10 + 15 = 5
    assert!((manager.get_total_pnl().await - 5.0).abs() < 1e-9);

    // Unrealized PnL = 10 * (1.5 - 1.0) = 5
    assert!((manager.get_total_unrealized_pnl().await - 5.0).abs() < 1e-9);

    Ok(())
}

#[tokio::test]
async fn test_market_maker_cash_flow() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        100.0,
        chrono::Duration::hours(24),
        10,
        &registry,
        exit_strategy,
    );

    // Simulate market maker: alternating buys and sells without fully closing
    // Round 1: Buy 10 at 1.00
    let buy1 = ExecutionEvent::OrderFilled {
        mint: "TEST".to_string(),
        token_amount_change: 10.0,
        quote_amount_change: Some(10.0),
        price: Some(1.0),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&buy1).await?;

    // Round 1: Sell 5 at 1.02 (partial sell, capturing spread)
    let sell1 = ExecutionEvent::OrderFilled {
        mint: "TEST".to_string(),
        token_amount_change: -5.0,
        quote_amount_change: Some(5.1), // 5 * 1.02
        price: Some(1.02),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&sell1).await?;

    // Round 2: Buy 5 at 1.01
    let buy2 = ExecutionEvent::OrderFilled {
        mint: "TEST".to_string(),
        token_amount_change: 5.0,
        quote_amount_change: Some(5.05), // 5 * 1.01
        price: Some(1.01),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&buy2).await?;

    // Round 2: Sell 5 at 1.03
    let sell2 = ExecutionEvent::OrderFilled {
        mint: "TEST".to_string(),
        token_amount_change: -5.0,
        quote_amount_change: Some(5.15), // 5 * 1.03
        price: Some(1.03),
        timestamp: Utc::now(),
        slippage: None,
        clear_position: false,
        force_position_clear: false,
        execution_latency_in_slots: None,
        signal_id: None,
        confirmed_slot: Some(0),
        confirmed_signature: Some(Signature::default()),
        exit_mode: None,
    };
    manager.handle_execution(&sell2).await?;

    // Total spent = 10 + 5.05 = 15.05
    // Total received = 5.1 + 5.15 = 10.25
    // Net cash flow = 10.25 - 15.05 = -4.80 (still have inventory)
    // Remaining position = 10 - 5 + 5 - 5 = 5 tokens at ~1.0 entry

    assert!((manager.get_total_quote_spent().await - 15.05).abs() < 1e-9);
    assert!((manager.get_total_quote_received().await - 10.25).abs() < 1e-9);
    assert!((manager.get_net_cash_flow().await - (-4.80)).abs() < 1e-9);

    // Position should have ~5 tokens
    let position = manager.get_position("TEST").await.unwrap();
    assert!((position.amount - 5.0).abs() < 1e-9);

    Ok(())
}

#[tokio::test]
async fn test_fully_filled_order_removed_from_pending() -> Result<()> {
    let registry = Registry::new();
    let exit_strategy = create_test_exit_strategy();
    let manager = InMemoryPositionManager::new(
        0.1,
        100.0,
        chrono::Duration::hours(24),
        10,
        &registry,
        exit_strategy,
    );

    let mint = "TESTMINT".to_string();
    let order_id = "order1".to_string();
    let timestamp = Utc::now();

    // Add a pending limit order
    let pending_order = PendingLimitOrder::new(
        order_id.clone(),
        mint.clone(),
        Some("market1".to_string()),
        OrderSide::Buy,
        1.0,   // price
        100.0, // size
        timestamp,
        None,
    );
    manager.add_pending_order_for_testing(&mint, &order_id, pending_order);

    // Verify order is in pending state
    let pending_orders = manager.get_pending_orders_for_mint(&mint);
    assert_eq!(pending_orders.len(), 1);
    assert!(pending_orders.contains_key(&order_id));

    // Simulate partial fill (50 remaining)
    let partial_fill_event = LimitOrderEvent::OrderPartiallyFilled {
        order_id: order_id.clone(),
        mint: mint.clone(),
        side: OrderSide::Buy,
        filled_size: 50.0,
        remaining_size: 50.0,
        fill_price: 1.0,
        timestamp: Utc::now(),
        signal_id: None,
        exit_mode: None,
    };
    manager
        .handle_limit_order_event(&partial_fill_event)
        .await?;

    // Verify order is still in pending state with updated size
    let pending_orders = manager.get_pending_orders_for_mint(&mint);
    assert_eq!(pending_orders.len(), 1);
    let order = pending_orders.get(&order_id).unwrap();
    assert!((order.remaining_size - 50.0).abs() < 1e-9);

    // Simulate full fill (remaining_size = 0)
    let full_fill_event = LimitOrderEvent::OrderPartiallyFilled {
        order_id: order_id.clone(),
        mint: mint.clone(),
        side: OrderSide::Buy,
        filled_size: 50.0,
        remaining_size: 0.0,
        fill_price: 1.0,
        timestamp: Utc::now(),
        signal_id: None,
        exit_mode: None,
    };
    manager.handle_limit_order_event(&full_fill_event).await?;

    // Verify order is removed from pending state
    let pending_orders = manager.get_pending_orders_for_mint(&mint);
    assert!(
        pending_orders.is_empty(),
        "Fully filled order should be removed from pending state"
    );

    Ok(())
}
