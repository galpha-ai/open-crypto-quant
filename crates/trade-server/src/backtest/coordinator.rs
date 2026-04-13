//! Backtest event coordinator for delivering events in chronological order.
//!
//! The BacktestEventCoordinator delivers events from a BacktestTimeline in the correct
//! priority order: enqueued events > buffered trades > timer events > next snapshot.

use std::{collections::VecDeque, sync::Mutex, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use popeyes_trading_types::{
    MarketDataEvent, OrderbookSnapshotEvent, OrderbookUpdateEvent, PolymarketTradeEvent,
    SpotPriceUpdate,
};
use tracing::{debug, trace};

use crate::{
    domain::{SystemEvent, TimerEvent},
    event_coordinator::{EventCoordinator, EventCoordinatorError},
    orderbook_tracker::OrderbookTracker,
};

use super::{BacktestTick, BacktestTimeline};

/// State for timer event generation.
struct TimerState {
    /// Interval between timer events
    interval: Duration,
    /// Next timer timestamp
    next_timer: DateTime<Utc>,
    /// Timeline end time (timers stop after this)
    end_time: Option<DateTime<Utc>>,
}

impl TimerState {
    fn new(interval: Duration, start_time: DateTime<Utc>, end_time: Option<DateTime<Utc>>) -> Self {
        Self {
            interval,
            next_timer: start_time,
            end_time,
        }
    }

    /// Check if a timer event should be emitted at the current simulation time.
    fn should_emit(&self, current_time: DateTime<Utc>) -> bool {
        if let Some(end) = self.end_time {
            if self.next_timer > end {
                return false;
            }
        }
        self.next_timer <= current_time
    }

    /// Get the next timer event timestamp and advance the timer state.
    fn emit(&mut self) -> DateTime<Utc> {
        let ts = self.next_timer;
        self.next_timer +=
            chrono::Duration::from_std(self.interval).unwrap_or(chrono::Duration::seconds(1));
        ts
    }
}

/// Internal state of the backtest event coordinator (timeline-based).
struct TimelineCoordinatorState {
    /// The timeline of backtest ticks
    timeline: BacktestTimeline,
    /// Current tick being processed (None if not started or exhausted)
    current_tick: Option<BacktestTick>,
    /// Whether the current tick's snapshot has been delivered
    snapshot_delivered: bool,
    /// Index of the next trade to process within the current tick
    trade_index: usize,
    /// Index of the next spot price to process within the current tick
    spot_price_index: usize,
    /// Events enqueued by the system (fill events, position events, etc.)
    enqueued_events: VecDeque<SystemEvent>,
    /// Timer state for generating periodic events
    timer_state: TimerState,
    /// Current simulation time
    current_time: DateTime<Utc>,
    /// Whether we've finished all ticks
    exhausted: bool,
}

/// Internal state of the backtest event coordinator (streaming, update-aware).
struct StreamingCoordinatorState {
    snapshots: std::iter::Peekable<std::vec::IntoIter<OrderbookSnapshotEvent>>,
    updates: std::iter::Peekable<std::vec::IntoIter<OrderbookUpdateEvent>>,
    trades: std::iter::Peekable<std::vec::IntoIter<PolymarketTradeEvent>>,
    spot_prices: Option<std::iter::Peekable<std::vec::IntoIter<SpotPriceUpdate>>>,

    tracker: OrderbookTracker,

    enqueued_events: VecDeque<SystemEvent>,
    timer_state: TimerState,
    current_time: DateTime<Utc>,
    first_snapshot_ts: i64,
    exhausted: bool,
}

/// Internal state of the backtest event coordinator.
enum CoordinatorState {
    Timeline(TimelineCoordinatorState),
    Streaming(StreamingCoordinatorState),
}

enum OrderbookInput {
    Snapshot(OrderbookSnapshotEvent),
    Update(OrderbookUpdateEvent),
}

/// Backtest event coordinator that delivers events from a timeline.
///
/// Event delivery priority:
/// 1. Enqueued events (fill events, position events)
/// 2. Buffered trade events for current tick (before snapshot)
/// 3. Orderbook snapshot for current tick
/// 4. Timer events (if interval elapsed)
/// 5. Advance to next tick
///
/// Trade events are delivered BEFORE the snapshot to reflect real-world causality:
/// trades at time T occur against the orderbook state before our strategy reacts
/// to the snapshot at T. This allows fills to occur against orders placed from
/// the previous snapshot, before the current snapshot triggers order updates.
///
/// When the timeline is exhausted, returns `EventCoordinatorError::NoMoreEvents`.
pub struct BacktestEventCoordinator {
    state: Mutex<CoordinatorState>,
}

impl BacktestEventCoordinator {
    /// Create a new backtest event coordinator.
    ///
    /// # Arguments
    /// * `timeline` - The timeline of backtest events
    /// * `timer_interval` - Interval between timer events
    pub fn new(mut timeline: BacktestTimeline, timer_interval: Duration) -> Self {
        // Determine start and end times from timeline
        let (start_time, end_time) = if let Some((start_ts, end_ts)) = timeline.time_range() {
            let start = DateTime::from_timestamp_millis(start_ts).unwrap_or_else(Utc::now);
            let end = DateTime::from_timestamp_millis(end_ts);
            (start, end)
        } else {
            (Utc::now(), None)
        };

        // Get the first tick if available
        let current_tick = timeline.next_tick().cloned();

        let state = TimelineCoordinatorState {
            timeline,
            current_tick,
            snapshot_delivered: false,
            trade_index: 0,
            spot_price_index: 0,
            enqueued_events: VecDeque::new(),
            timer_state: TimerState::new(timer_interval, start_time, end_time),
            current_time: start_time,
            exhausted: false,
        };

        Self {
            state: Mutex::new(CoordinatorState::Timeline(state)),
        }
    }

    /// Create a backtest coordinator from raw event vectors.
    ///
    /// This mode applies orderbook updates on the fly using `OrderbookTracker` so we don't
    /// need to materialize a full synthetic snapshot stream in memory.
    pub fn new_from_data(
        mut snapshots: Vec<OrderbookSnapshotEvent>,
        mut updates: Vec<OrderbookUpdateEvent>,
        mut trades: Vec<PolymarketTradeEvent>,
        mut spot_prices: Option<Vec<SpotPriceUpdate>>,
        timer_interval: Duration,
    ) -> Self {
        snapshots.sort_by_key(|s| s.timestamp);
        updates.sort_by_key(|u| u.timestamp);
        trades.sort_by_key(|t| t.timestamp);
        if let Some(ref mut spots) = spot_prices {
            spots.sort_by_key(|s| s.timestamp);
        }

        let (start_time, end_time, first_snapshot_ts) = if let Some(first) = snapshots.first() {
            let start = DateTime::from_timestamp_millis(first.timestamp).unwrap_or_else(Utc::now);
            let last_snapshot_ts = snapshots
                .last()
                .map(|s| s.timestamp)
                .unwrap_or(first.timestamp);
            let last_update_ts = updates
                .last()
                .map(|u| u.timestamp)
                .unwrap_or(last_snapshot_ts);
            let end_ts = last_snapshot_ts.max(last_update_ts);
            let end = DateTime::from_timestamp_millis(end_ts);
            (start, end, first.timestamp)
        } else {
            (Utc::now(), None, i64::MAX)
        };

        let state = StreamingCoordinatorState {
            snapshots: snapshots.into_iter().peekable(),
            updates: updates.into_iter().peekable(),
            trades: trades.into_iter().peekable(),
            spot_prices: spot_prices.map(|v| v.into_iter().peekable()),
            tracker: OrderbookTracker::new(),
            enqueued_events: VecDeque::new(),
            timer_state: TimerState::new(timer_interval, start_time, end_time),
            current_time: start_time,
            first_snapshot_ts,
            exhausted: false,
        };

        Self {
            state: Mutex::new(CoordinatorState::Streaming(state)),
        }
    }

    fn peek_next_orderbook_ts(stream: &mut StreamingCoordinatorState) -> Option<i64> {
        let s_ts = stream.snapshots.peek().map(|s| s.timestamp);
        let u_ts = stream.updates.peek().map(|u| u.timestamp);
        match (s_ts, u_ts) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    fn pop_next_orderbook_input(stream: &mut StreamingCoordinatorState) -> Option<OrderbookInput> {
        let s_ts = stream.snapshots.peek().map(|s| s.timestamp);
        let u_ts = stream.updates.peek().map(|u| u.timestamp);
        match (s_ts, u_ts) {
            (Some(_), Some(_)) if s_ts <= u_ts => {
                stream.snapshots.next().map(OrderbookInput::Snapshot)
            }
            (Some(_), Some(_)) => stream.updates.next().map(OrderbookInput::Update),
            (Some(_), None) => stream.snapshots.next().map(OrderbookInput::Snapshot),
            (None, Some(_)) => stream.updates.next().map(OrderbookInput::Update),
            (None, None) => None,
        }
    }

    /// Get the number of enqueued events (for testing).
    pub fn enqueued_count(&self) -> usize {
        match &*self.state.lock().unwrap() {
            CoordinatorState::Timeline(s) => s.enqueued_events.len(),
            CoordinatorState::Streaming(s) => s.enqueued_events.len(),
        }
    }

    /// Enqueue multiple limit order events.
    ///
    /// This is a convenience method for enqueuing fill events from the executor.
    /// Each LimitOrderEvent is wrapped in a SystemEvent and added to the queue.
    pub fn enqueue_limit_order_events(&self, events: Vec<crate::execution::LimitOrderEvent>) {
        let mut state = self.state.lock().unwrap();
        for event in events {
            trace!(event_type = "LimitOrder", "Enqueueing fill event");
            match &mut *state {
                CoordinatorState::Timeline(s) => {
                    s.enqueued_events.push_back(SystemEvent::LimitOrder(event));
                }
                CoordinatorState::Streaming(s) => {
                    s.enqueued_events.push_back(SystemEvent::LimitOrder(event));
                }
            }
        }
    }

    /// Get the current simulation time.
    pub fn current_time(&self) -> DateTime<Utc> {
        match &*self.state.lock().unwrap() {
            CoordinatorState::Timeline(s) => s.current_time,
            CoordinatorState::Streaming(s) => s.current_time,
        }
    }

    /// Check if the timeline has been exhausted.
    pub fn is_exhausted(&self) -> bool {
        match &*self.state.lock().unwrap() {
            CoordinatorState::Timeline(s) => s.exhausted,
            CoordinatorState::Streaming(s) => s.exhausted,
        }
    }
}

#[async_trait]
impl EventCoordinator for BacktestEventCoordinator {
    async fn enqueue_event(&self, event: SystemEvent) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        trace!(event_type = event.event_type(), "Enqueueing event");
        match &mut *state {
            CoordinatorState::Timeline(s) => s.enqueued_events.push_back(event),
            CoordinatorState::Streaming(s) => s.enqueued_events.push_back(event),
        }
        Ok(())
    }

    async fn next_event(&self) -> Result<SystemEvent> {
        loop {
            let mut state = self.state.lock().unwrap();
            match &mut *state {
                CoordinatorState::Timeline(state) => {
                    // Check if exhausted
                    if state.exhausted {
                        return Err(EventCoordinatorError::NoMoreEvents.into());
                    }

                    // Priority 1: Enqueued events (fill events, position events)
                    if let Some(event) = state.enqueued_events.pop_front() {
                        trace!(event_type = event.event_type(), "Returning enqueued event");
                        return Ok(event);
                    }

                    // Priority 2: Buffered trades for current tick (processed BEFORE snapshot)
                    if let Some(ref tick) = state.current_tick {
                        if state.trade_index < tick.trades.len() {
                            let trade = tick.trades[state.trade_index].clone();
                            state.trade_index += 1;

                            // Update current time to trade timestamp
                            if let Some(ts) = DateTime::from_timestamp_millis(trade.timestamp) {
                                state.current_time = ts;
                            }

                            trace!(
                                asset_id = %trade.asset_id,
                                price = trade.price,
                                size = trade.size,
                                "Returning trade event"
                            );
                            return Ok(SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(
                                trade,
                            )));
                        }
                    }

                    // Priority 2.5: Spot price updates for current tick (processed BEFORE snapshot)
                    if let Some(ref tick) = state.current_tick {
                        if state.spot_price_index < tick.spot_prices.len() {
                            let spot_price = tick.spot_prices[state.spot_price_index].clone();
                            state.spot_price_index += 1;

                            // Update current time to spot price timestamp
                            state.current_time = spot_price.timestamp;

                            trace!(
                                symbol = %spot_price.symbol,
                                price = spot_price.price,
                                "Returning spot price event"
                            );

                            return Ok(SystemEvent::MarketData(MarketDataEvent::SpotPrice(
                                spot_price,
                            )));
                        }
                    }

                    // Priority 3: Deliver snapshot for current tick (after trades are processed)
                    if !state.snapshot_delivered {
                        if let Some(ref tick) = state.current_tick {
                            let snapshot = tick.snapshot.clone();

                            // Update current time
                            if let Some(ts) = DateTime::from_timestamp_millis(snapshot.timestamp) {
                                state.current_time = ts;
                            }

                            state.snapshot_delivered = true;

                            debug!(
                                asset_id = %snapshot.asset_id,
                                timestamp = snapshot.timestamp,
                                "Returning orderbook snapshot"
                            );
                            return Ok(SystemEvent::MarketData(
                                MarketDataEvent::OrderbookSnapshot(snapshot),
                            ));
                        }
                    }

                    // Priority 4: Timer events (between current and next tick boundary)
                    let timer_info: Option<(DateTime<Utc>, DateTime<Utc>)> =
                        state.current_tick.as_ref().map(|tick| {
                            let next_tick_time = state
                                .timeline
                                .peek()
                                .and_then(|t| DateTime::from_timestamp_millis(t.timestamp()));
                            let tick_start = tick.datetime().unwrap_or(state.current_time);
                            let current_tick_end = next_tick_time
                                .unwrap_or_else(|| tick_start + chrono::Duration::seconds(1));
                            (tick_start, current_tick_end)
                        });

                    if let Some((_tick_start, current_tick_end)) = timer_info {
                        while state.timer_state.should_emit(current_tick_end) {
                            let timer_ts = state.timer_state.emit();

                            if timer_ts > state.current_time && timer_ts < current_tick_end {
                                debug!(timestamp = %timer_ts, "Returning timer event");
                                state.current_time = timer_ts;
                                return Ok(SystemEvent::Timer(TimerEvent {
                                    timestamp: timer_ts,
                                }));
                            }
                            if timer_ts >= current_tick_end {
                                break;
                            }
                        }
                    }

                    // Priority 5: Advance to next tick
                    if state.current_tick.is_some() {
                        state.current_tick = state.timeline.next_tick().cloned();
                        state.snapshot_delivered = false;
                        state.trade_index = 0;
                        state.spot_price_index = 0;

                        if state.current_tick.is_some() {
                            continue;
                        }
                    }

                    // No more events - timeline exhausted
                    state.exhausted = true;
                    return Err(EventCoordinatorError::NoMoreEvents.into());
                }
                CoordinatorState::Streaming(stream) => {
                    if stream.exhausted {
                        return Err(EventCoordinatorError::NoMoreEvents.into());
                    }

                    // Priority 1: Enqueued events (fill events, position events)
                    if let Some(event) = stream.enqueued_events.pop_front() {
                        trace!(event_type = event.event_type(), "Returning enqueued event");
                        return Ok(event);
                    }

                    // Determine the next market-data timestamp (used to schedule timers)
                    let next_trade_ts = stream.trades.peek().map(|t| t.timestamp);
                    let next_spot_ts = match stream.spot_prices.as_mut() {
                        Some(it) => it.peek().map(|s| s.timestamp.timestamp_millis()),
                        None => None,
                    };
                    let next_ob_ts = Self::peek_next_orderbook_ts(stream);

                    let mut next_data_ts = next_ob_ts;
                    if let Some(ts) = next_trade_ts {
                        next_data_ts = Some(next_data_ts.map(|x| x.min(ts)).unwrap_or(ts));
                    }
                    if let Some(ts) = next_spot_ts {
                        next_data_ts = Some(next_data_ts.map(|x| x.min(ts)).unwrap_or(ts));
                    }

                    // No more market data
                    let Some(next_data_ts) = next_data_ts else {
                        stream.exhausted = true;
                        return Err(EventCoordinatorError::NoMoreEvents.into());
                    };

                    // Emit timers in chronological order up to the next market-data timestamp
                    if let Some(next_dt) = DateTime::from_timestamp_millis(next_data_ts) {
                        if stream.timer_state.should_emit(next_dt) {
                            let timer_ts = stream.timer_state.emit();
                            stream.current_time = timer_ts;
                            return Ok(SystemEvent::Timer(TimerEvent {
                                timestamp: timer_ts,
                            }));
                        }
                    }

                    // Discard trades before the first snapshot (requires an initial book)
                    if let Some(trade) = stream.trades.peek() {
                        if trade.timestamp < stream.first_snapshot_ts {
                            let _ = stream.trades.next();
                            continue;
                        }
                    }

                    // Priority 2: Trades (chronological)
                    if let Some(trade) = stream.trades.peek() {
                        if trade.timestamp == next_data_ts {
                            let trade = stream.trades.next().expect("peeked trade exists");
                            if let Some(ts) = DateTime::from_timestamp_millis(trade.timestamp) {
                                stream.current_time = ts;
                            }
                            return Ok(SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(
                                trade,
                            )));
                        }
                    }

                    // Priority 2.5: Spot prices (chronological)
                    if let Some(spot_iter) = stream.spot_prices.as_mut() {
                        if let Some(spot) = spot_iter.peek() {
                            if spot.timestamp.timestamp_millis() == next_data_ts {
                                let spot = spot_iter.next().expect("peeked spot exists");
                                stream.current_time = spot.timestamp;
                                return Ok(SystemEvent::MarketData(MarketDataEvent::SpotPrice(
                                    spot,
                                )));
                            }
                        }
                    }

                    // Priority 3: Orderbook snapshots (raw snapshots + updates applied on the fly)
                    let Some(input) = Self::pop_next_orderbook_input(stream) else {
                        stream.exhausted = true;
                        return Err(EventCoordinatorError::NoMoreEvents.into());
                    };

                    let snapshot_opt = match input {
                        OrderbookInput::Snapshot(snapshot) => {
                            Some(stream.tracker.apply_snapshot(&snapshot))
                        }
                        OrderbookInput::Update(update) => stream.tracker.apply_update(&update),
                    };

                    let Some(snapshot) = snapshot_opt else {
                        // Update without prior snapshot; ignore.
                        continue;
                    };

                    if let Some(ts) = DateTime::from_timestamp_millis(snapshot.timestamp) {
                        stream.current_time = ts;
                    }

                    debug!(
                        asset_id = %snapshot.asset_id,
                        timestamp = snapshot.timestamp,
                        "Returning orderbook snapshot (streaming)"
                    );
                    return Ok(SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(
                        snapshot,
                    )));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use popeyes_trading_types::{
        OrderSummary, OrderbookSnapshotEvent, OrderbookSource, PolymarketTradeEvent, TradeSide,
    };

    use super::*;

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

    fn create_trade(timestamp: i64, asset_id: &str, price: f64) -> PolymarketTradeEvent {
        PolymarketTradeEvent {
            asset_id: asset_id.to_string(),
            market: "test-market".to_string(),
            price,
            size: 10.0,
            side: TradeSide::Buy,
            timestamp,
            fee_rate_bps: 0,
            market_metadata: None,
            observed_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_coordinator_delivers_snapshot_first() {
        let snapshots = vec![create_snapshot(1000, "asset1")];
        let trades = vec![];
        let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        let coordinator = BacktestEventCoordinator::new(timeline, Duration::from_secs(60));

        let event = coordinator.next_event().await.unwrap();
        assert!(matches!(
            event,
            SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(_))
        ));
    }

    #[tokio::test]
    async fn test_coordinator_delivers_trades_before_snapshot() {
        let snapshots = vec![create_snapshot(1000, "asset1")];
        let trades = vec![
            create_trade(1100, "asset1", 0.51),
            create_trade(1200, "asset1", 0.52),
        ];
        let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        let coordinator = BacktestEventCoordinator::new(timeline, Duration::from_secs(3600)); // Long interval to avoid timer events

        // First: first trade (trades are delivered before snapshot to allow filling orders from previous tick)
        let event = coordinator.next_event().await.unwrap();
        if let SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(trade)) = event {
            assert_eq!(trade.timestamp, 1100);
        } else {
            panic!(
                "Expected PolymarketTrade event, got: {:?}",
                event.event_type()
            );
        }

        // Second: second trade
        let event = coordinator.next_event().await.unwrap();
        if let SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(trade)) = event {
            assert_eq!(trade.timestamp, 1200);
        } else {
            panic!("Expected PolymarketTrade event");
        }

        // Third: snapshot
        let event = coordinator.next_event().await.unwrap();
        assert!(matches!(
            event,
            SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(_))
        ));
    }

    #[tokio::test]
    async fn test_coordinator_enqueued_events_have_priority() {
        let snapshots = vec![create_snapshot(1000, "asset1")];
        let trades = vec![create_trade(1100, "asset1", 0.51)];
        let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        let coordinator = BacktestEventCoordinator::new(timeline, Duration::from_secs(3600));

        // Enqueue a timer event
        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        coordinator.enqueue_event(timer_event).await.unwrap();

        // First: enqueued timer event (priority 1)
        let event = coordinator.next_event().await.unwrap();
        assert!(matches!(event, SystemEvent::Timer(_)));

        // Second: trade (priority 2)
        let event = coordinator.next_event().await.unwrap();
        assert!(matches!(
            event,
            SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(_))
        ));

        // Third: snapshot (priority 3)
        let event = coordinator.next_event().await.unwrap();
        assert!(matches!(
            event,
            SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(_))
        ));
    }

    #[tokio::test]
    async fn test_coordinator_no_more_events() {
        let snapshots = vec![create_snapshot(1000, "asset1")];
        let trades = vec![];
        let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        let coordinator = BacktestEventCoordinator::new(timeline, Duration::from_secs(3600));

        // Get the snapshot
        let _ = coordinator.next_event().await.unwrap();

        // Should return NoMoreEvents
        let result = coordinator.next_event().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_coordinator_multiple_ticks() {
        let snapshots = vec![
            create_snapshot(1000, "asset1"),
            create_snapshot(2000, "asset1"),
        ];
        let trades = vec![
            create_trade(1100, "asset1", 0.51),
            create_trade(2100, "asset1", 0.52),
        ];
        let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        let coordinator = BacktestEventCoordinator::new(timeline, Duration::from_secs(3600));

        // Tick 1: trade first (allows filling orders from previous periods)
        let event = coordinator.next_event().await.unwrap();
        if let SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(trade)) = event {
            assert_eq!(trade.timestamp, 1100);
        } else {
            panic!(
                "Expected PolymarketTrade event, got: {:?}",
                event.event_type()
            );
        }

        // Tick 1: snapshot
        let event = coordinator.next_event().await.unwrap();
        assert!(
            matches!(
                event,
                SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(_))
            ),
            "Expected snapshot, got: {:?}",
            event.event_type()
        );

        // Tick 2: trade first
        let event = coordinator.next_event().await.unwrap();
        if let SystemEvent::MarketData(MarketDataEvent::PolymarketTrade(trade)) = event {
            assert_eq!(trade.timestamp, 2100);
        } else {
            panic!(
                "Expected PolymarketTrade event for tick 2, got: {:?}",
                event.event_type()
            );
        }

        // Tick 2: snapshot
        let event = coordinator.next_event().await.unwrap();
        assert!(
            matches!(
                event,
                SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(_))
            ),
            "Expected snapshot for tick 2, got: {:?}",
            event.event_type()
        );
    }

    #[tokio::test]
    async fn test_coordinator_timer_events() {
        // Create timeline spanning 5 seconds
        let snapshots = vec![
            create_snapshot(1000, "asset1"), // t=1s
            create_snapshot(5000, "asset1"), // t=5s
        ];
        let trades = vec![];
        let timeline = BacktestTimeline::new(snapshots, trades, None).unwrap();

        // Timer every 1 second
        let coordinator = BacktestEventCoordinator::new(timeline, Duration::from_secs(1));

        // First: snapshot at t=1s
        let event = coordinator.next_event().await.unwrap();
        assert!(matches!(
            event,
            SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(_))
        ));

        // Next events could be timers or the second snapshot
        // The exact behavior depends on timer logic - we verify we don't crash
        let mut got_second_snapshot = false;
        for _ in 0..10 {
            let result = coordinator.next_event().await;
            if result.is_err() {
                break;
            }
            let event = result.unwrap();
            if let SystemEvent::MarketData(MarketDataEvent::OrderbookSnapshot(s)) = event {
                if s.timestamp == 5000 {
                    got_second_snapshot = true;
                }
            }
        }
        assert!(got_second_snapshot, "Should have received second snapshot");
    }
}
