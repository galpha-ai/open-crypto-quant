use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use popeyes_trading_types::{MarketDataEvent, PolymarketTradeEvent, TradeSide};
use prometheus::Registry;

use crate::domain::SystemEvent;
use crate::event_coordinator::{
    CapturingEventCoordinator, EventCollector, EventCoordinator, EventCoordinatorError,
    InMemoryEventCollector,
};
use crate::execution::{ExecutionEvent, LimitOrderEvent, Order, OrderExecutor};
use crate::notifier::{Notifiable, Notifier};
use crate::position::{ConfigurableExitStrategy, InMemoryPositionManager, PositionManager};
use crate::signal::NoopSignalGenerator;
use crate::trade_server::TradeServer;

struct MockNotifier;

#[async_trait]
impl Notifier for MockNotifier {
    async fn notify(&self, _notification: &(dyn Notifiable + Sync)) -> Result<()> {
        Ok(())
    }
}

struct QueueEventCoordinator {
    events: Mutex<VecDeque<SystemEvent>>,
}

impl QueueEventCoordinator {
    fn new(events: Vec<SystemEvent>) -> Self {
        Self {
            events: Mutex::new(VecDeque::from(events)),
        }
    }
}

#[async_trait]
impl EventCoordinator for QueueEventCoordinator {
    async fn next_event(&self) -> Result<SystemEvent> {
        let mut events = self.events.lock().unwrap();
        events
            .pop_front()
            .ok_or(EventCoordinatorError::NoMoreEvents.into())
    }

    async fn enqueue_event(&self, event: SystemEvent) -> Result<()> {
        self.events.lock().unwrap().push_back(event);
        Ok(())
    }
}

struct DeferredFillExecutor {
    fill_events: Vec<LimitOrderEvent>,
}

impl DeferredFillExecutor {
    fn new(fill_events: Vec<LimitOrderEvent>) -> Self {
        Self { fill_events }
    }
}

#[async_trait]
impl OrderExecutor for DeferredFillExecutor {
    async fn execute_market_order(&self, order: Order) -> Result<ExecutionEvent> {
        Ok(ExecutionEvent::OrderRejected {
            mint: order.mint,
            reason: "mock executor only simulates trade fills".to_string(),
        })
    }

    async fn handle_token_trade(
        &self,
        _trade: &popeyes_trading_types::TokenTradeEvent,
    ) -> Result<()> {
        Ok(())
    }

    async fn check_fills_from_trade(
        &self,
        _trade: &PolymarketTradeEvent,
        _inventory: &HashMap<String, f64>,
        _available_quote: f64,
    ) -> Vec<LimitOrderEvent> {
        self.fill_events.clone()
    }

    fn simulates_fills(&self) -> bool {
        true
    }

    fn defers_limit_order_events(&self) -> bool {
        true
    }
}

fn create_test_position_manager() -> InMemoryPositionManager {
    let registry = Registry::new();
    let exit_strategy = Arc::new(ConfigurableExitStrategy::new(
        0.5,
        0.2,
        chrono::Duration::minutes(5),
        3,
    ));

    InMemoryPositionManager::new(
        0.1,
        10_000.0,
        chrono::Duration::minutes(5),
        5,
        &registry,
        exit_strategy,
    )
}

fn create_trade_event() -> PolymarketTradeEvent {
    PolymarketTradeEvent {
        asset_id: "asset-1".to_string(),
        market: "market-1".to_string(),
        price: 0.42,
        size: 100.0,
        side: TradeSide::Sell,
        timestamp: Utc::now().timestamp_millis(),
        observed_at: Utc::now(),
        fee_rate_bps: 0,
        market_metadata: None,
    }
}

fn create_deferred_lifecycle_events() -> Vec<LimitOrderEvent> {
    let now = Utc::now();
    let order_id = "paper-order-1".to_string();
    vec![
        LimitOrderEvent::OrderPlaced {
            order_id: order_id.clone(),
            mint: "asset-1".to_string(),
            market: Some("market-1".to_string()),
            price: 0.42,
            size: 100.0,
            side: crate::execution::OrderSide::Buy,
            timestamp: now,
            signal_id: Some("signal-1".to_string()),
            context: None,
        },
        LimitOrderEvent::OrderCancelled {
            order_id,
            reason: Some("test cancel".to_string()),
            timestamp: now,
        },
    ]
}

#[tokio::test]
async fn test_deferred_simulated_lifecycle_events_are_captured_as_limit_order_events() {
    let base_coordinator = Arc::new(QueueEventCoordinator::new(vec![SystemEvent::MarketData(
        MarketDataEvent::PolymarketTrade(create_trade_event()),
    )]));
    let collector = Arc::new(InMemoryEventCollector::new());
    let capturing = Arc::new(CapturingEventCoordinator::new(
        base_coordinator,
        collector.clone(),
    ));

    let event_coordinator: Arc<dyn EventCoordinator> = capturing;
    let notifier: Arc<dyn Notifier> = Arc::new(MockNotifier);
    let position_manager: Arc<dyn PositionManager> = Arc::new(create_test_position_manager());
    let order_executor: Arc<dyn OrderExecutor + Send + Sync> =
        Arc::new(DeferredFillExecutor::new(create_deferred_lifecycle_events()));
    let exit_strategy = Arc::new(ConfigurableExitStrategy::new(
        0.5,
        0.2,
        chrono::Duration::minutes(5),
        3,
    ));

    let mut server = TradeServer::new(
        event_coordinator,
        vec![Box::new(NoopSignalGenerator)],
        notifier,
        position_manager,
        order_executor,
        exit_strategy,
        3,
        60_000,
        Registry::new(),
    );

    server.run().await.unwrap();

    let event_types: Vec<String> = collector
        .events()
        .into_iter()
        .map(|e| e.event_type)
        .collect();
    assert!(
        event_types
            .iter()
            .any(|event_type| event_type == "LimitOrder.OrderPlaced"),
        "expected LimitOrder.OrderPlaced in captured events: {event_types:?}"
    );
    assert!(
        event_types
            .iter()
            .any(|event_type| event_type == "LimitOrder.OrderCancelled"),
        "expected LimitOrder.OrderCancelled in captured events: {event_types:?}"
    );
}
