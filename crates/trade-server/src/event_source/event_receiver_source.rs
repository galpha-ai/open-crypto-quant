use anyhow::Result;
use async_trait::async_trait;
use popeyes_trading_types::Event;
use std::time::Duration;
use tx_sub_common::subscriber::EventReceiver;

use crate::domain::SystemEvent;
use crate::event_source::EventSource;

/// Wrapper that adapts tx-sub-common's `EventReceiver` trait to trade-server's `EventSource` trait.
///
/// This thin wrapper enables trade-server to use all Redis subscriber implementations
/// (List, Stream, Pubsub) from the tx-sub common library while maintaining the existing
/// `EventSource` interface used by `GenericEventCoordinator`.
pub struct EventReceiverSource {
    receiver: Box<dyn EventReceiver>,
}

impl EventReceiverSource {
    /// Create a new EventReceiverSource wrapping an EventReceiver implementation.
    pub fn new(receiver: Box<dyn EventReceiver>) -> Self {
        Self { receiver }
    }
}

/// Maps the top-level Event enum to SystemEvent variants.
fn event_to_system_event(event: Event) -> SystemEvent {
    match event {
        Event::Token(token_event) => SystemEvent::Token(token_event),
        Event::MarketData(market_data_event) => SystemEvent::MarketData(market_data_event),
    }
}

#[async_trait]
impl EventSource for EventReceiverSource {
    async fn try_next_event(&self) -> Result<Option<SystemEvent>> {
        // Use a short timeout for non-blocking poll behavior
        let received = self
            .receiver
            .try_next_event(Duration::from_millis(100))
            .await?;
        Ok(received.map(|r| event_to_system_event(r.event)))
    }

    async fn next_event(&self) -> Result<Option<SystemEvent>> {
        let received = self.receiver.next_event().await?;
        Ok(Some(event_to_system_event(received.event)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use popeyes_trading_types::{
        Dex, MarketDataEvent, OrderbookSource, OrderbookUpdateEvent, TokenEvent, TradeEventType,
        TradeSide,
    };
    use tx_sub_common::subscriber::ReceivedEvent;

    struct MockEventReceiver {
        events: std::sync::Mutex<Vec<Event>>,
    }

    impl MockEventReceiver {
        fn new(events: Vec<Event>) -> Self {
            Self {
                events: std::sync::Mutex::new(events),
            }
        }
    }

    #[async_trait]
    impl EventReceiver for MockEventReceiver {
        async fn next_event(&self) -> Result<ReceivedEvent> {
            let mut events = self.events.lock().unwrap();
            if events.is_empty() {
                // Block forever in real usage, but for tests we return an error
                anyhow::bail!("No more events");
            }
            let event = events.remove(0);
            Ok(ReceivedEvent {
                event,
                stream_id: None,
                source: "mock".to_string(),
            })
        }

        async fn try_next_event(&self, _timeout: Duration) -> Result<Option<ReceivedEvent>> {
            let mut events = self.events.lock().unwrap();
            if events.is_empty() {
                return Ok(None);
            }
            let event = events.remove(0);
            Ok(Some(ReceivedEvent {
                event,
                stream_id: None,
                source: "mock".to_string(),
            }))
        }

        fn name(&self) -> &str {
            "mock"
        }
    }

    fn create_test_token_event(mint: &str) -> Event {
        Event::Token(TokenEvent::Create(
            popeyes_trading_types::TokenCreationEvent {
                signature: "test_signature".to_string(),
                mint: mint.to_string(),
                trader_public_key: "test_trader".to_string(),
                tx_type: TradeEventType::Create,
                initial_buy: 0.0,
                bonding_curve_key: "test_bonding_curve".to_string(),
                v_tokens_in_bonding_curve: 1000000.0,
                v_sol_in_bonding_curve: 100.0,
                market_cap_sol: 100.0,
                name: "Test Token".to_string(),
                symbol: "TEST".to_string(),
                uri: "https://example.com/metadata.json".to_string(),
                timestamp: chrono::Utc::now(),
                slot: 12345,
                dex: Dex::PumpFun,
            },
        ))
    }

    fn create_test_market_data_event(asset_id: &str) -> Event {
        Event::MarketData(MarketDataEvent::OrderbookUpdate(OrderbookUpdateEvent {
            asset_id: asset_id.to_string(),
            market: "test_market".to_string(),
            price: 0.5,
            size: 100.0,
            side: TradeSide::Buy,
            hash: "test_hash".to_string(),
            best_bid: 0.5,
            best_ask: 0.51,
            timestamp: chrono::Utc::now().timestamp_millis(),
            source: OrderbookSource::Polymarket,
            market_metadata: None,
            observed_at: chrono::Utc::now(),
        }))
    }

    #[tokio::test]
    async fn test_event_receiver_source_try_next_event_token() {
        let events = vec![create_test_token_event("test_mint")];

        let mock_receiver = MockEventReceiver::new(events);
        let source = EventReceiverSource::new(Box::new(mock_receiver));

        let result = source.try_next_event().await.unwrap();
        assert!(result.is_some());

        if let Some(SystemEvent::Token(TokenEvent::Create(create_event))) = result {
            assert_eq!(create_event.mint, "test_mint");
        } else {
            panic!("Expected Token(Create) event");
        }

        // Second call should return None
        let result = source.try_next_event().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_event_receiver_source_try_next_event_market_data() {
        let events = vec![create_test_market_data_event("test_asset")];

        let mock_receiver = MockEventReceiver::new(events);
        let source = EventReceiverSource::new(Box::new(mock_receiver));

        let result = source.try_next_event().await.unwrap();
        assert!(result.is_some());

        if let Some(SystemEvent::MarketData(MarketDataEvent::OrderbookUpdate(update))) = result {
            assert_eq!(update.asset_id, "test_asset");
        } else {
            panic!("Expected MarketData(OrderbookUpdate) event");
        }
    }

    #[tokio::test]
    async fn test_event_receiver_source_next_event_token() {
        let events = vec![create_test_token_event("test_mint")];

        let mock_receiver = MockEventReceiver::new(events);
        let source = EventReceiverSource::new(Box::new(mock_receiver));

        let result = source.next_event().await.unwrap();
        assert!(result.is_some());

        if let Some(SystemEvent::Token(TokenEvent::Create(create_event))) = result {
            assert_eq!(create_event.mint, "test_mint");
        } else {
            panic!("Expected Token(Create) event");
        }
    }

    #[tokio::test]
    async fn test_event_receiver_source_next_event_market_data() {
        let events = vec![create_test_market_data_event("test_asset")];

        let mock_receiver = MockEventReceiver::new(events);
        let source = EventReceiverSource::new(Box::new(mock_receiver));

        let result = source.next_event().await.unwrap();
        assert!(result.is_some());

        if let Some(SystemEvent::MarketData(MarketDataEvent::OrderbookUpdate(update))) = result {
            assert_eq!(update.asset_id, "test_asset");
        } else {
            panic!("Expected MarketData(OrderbookUpdate) event");
        }
    }
}
