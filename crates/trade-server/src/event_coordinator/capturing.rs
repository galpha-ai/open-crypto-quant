//! Capturing event coordinator decorator.
//!
//! The `CapturingEventCoordinator` wraps any `EventCoordinator` implementation
//! and captures delivered events flowing through it for later analysis.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::domain::SystemEvent;

use super::{EventCollector, EventCoordinator};

/// A decorator that wraps any `EventCoordinator` and captures all events.
///
/// This provides a unified event capture mechanism that works with any coordinator
/// backend (Redis, Backtest, Noop). Events are captured on egress only:
/// - Events delivered via `next_event()`
///
/// `enqueue_event()` is forwarded without recording so enqueued events that are
/// later emitted by `next_event()` are captured exactly once.
///
/// The inner coordinator is wrapped in an `Arc` to allow sharing the same coordinator
/// instance with other components that may need direct access (e.g., for backtest-specific
/// methods like `enqueue_limit_order_events`).
///
/// # Example
///
/// ```ignore
/// let collector = Arc::new(InMemoryEventCollector::new());
/// let base_coordinator = Arc::new(BacktestEventCoordinator::new(timeline, timer_interval));
/// let coordinator = CapturingEventCoordinator::new(base_coordinator.clone(), collector.clone());
///
/// // Events are captured automatically as they flow through
/// let event = coordinator.next_event().await?;
///
/// // Export captured events at the end
/// collector.export(&output_path)?;
/// ```
pub struct CapturingEventCoordinator<C: EventCoordinator> {
    inner: Arc<C>,
    collector: Arc<dyn EventCollector>,
    logical_time_fn: Option<Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>>,
    capture_filter: Option<Arc<dyn Fn(&SystemEvent) -> bool + Send + Sync>>,
}

impl<C: EventCoordinator> CapturingEventCoordinator<C> {
    /// Create a new capturing coordinator that wraps the given inner coordinator.
    ///
    /// # Arguments
    /// * `inner` - The underlying event coordinator to wrap (as Arc for shared ownership)
    /// * `collector` - The event collector to record events to
    pub fn new(inner: Arc<C>, collector: Arc<dyn EventCollector>) -> Self {
        Self {
            inner,
            collector,
            logical_time_fn: None,
            capture_filter: None,
        }
    }

    /// Create a new capturing coordinator with a logical time function.
    ///
    /// The logical time function is called when recording events to capture
    /// the simulation time (for backtests) alongside the wall-clock timestamp.
    ///
    /// # Arguments
    /// * `inner` - The underlying event coordinator to wrap (as Arc for shared ownership)
    /// * `collector` - The event collector to record events to
    /// * `logical_time_fn` - Function that returns the current logical/simulation time
    pub fn with_logical_time_fn(
        inner: Arc<C>,
        collector: Arc<dyn EventCollector>,
        logical_time_fn: Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>,
    ) -> Self {
        Self {
            inner,
            collector,
            logical_time_fn: Some(logical_time_fn),
            capture_filter: None,
        }
    }

    /// Create a new capturing coordinator with a logical time function and a capture filter.
    ///
    /// The capture filter controls which events are recorded; events are always delivered.
    pub fn with_logical_time_fn_and_filter(
        inner: Arc<C>,
        collector: Arc<dyn EventCollector>,
        logical_time_fn: Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>,
        capture_filter: Arc<dyn Fn(&SystemEvent) -> bool + Send + Sync>,
    ) -> Self {
        Self {
            inner,
            collector,
            logical_time_fn: Some(logical_time_fn),
            capture_filter: Some(capture_filter),
        }
    }

    /// Get a reference to the event collector.
    pub fn collector(&self) -> &Arc<dyn EventCollector> {
        &self.collector
    }

    /// Get a reference to the inner coordinator.
    pub fn inner(&self) -> &Arc<C> {
        &self.inner
    }

    /// Get the current logical time, if a logical time function is set.
    fn get_logical_time(&self) -> Option<DateTime<Utc>> {
        self.logical_time_fn.as_ref().map(|f| f())
    }

    fn should_capture(&self, event: &SystemEvent) -> bool {
        self.capture_filter
            .as_ref()
            .map(|f| f(event))
            .unwrap_or(true)
    }
}

#[async_trait]
impl<C: EventCoordinator> EventCoordinator for CapturingEventCoordinator<C> {
    async fn next_event(&self) -> Result<SystemEvent> {
        let event = self.inner.next_event().await?;
        if self.should_capture(&event) {
            self.collector
                .record_with_logical_time(&event, self.get_logical_time());
        }
        Ok(event)
    }

    async fn enqueue_event(&self, event: SystemEvent) -> Result<()> {
        self.inner.enqueue_event(event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TimerEvent;
    use crate::event_coordinator::{InMemoryEventCollector, NoopEventCoordinator};
    use chrono::Utc;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A mock coordinator that returns a fixed number of events
    struct MockEventCoordinator {
        events: std::sync::Mutex<VecDeque<SystemEvent>>,
        enqueued: AtomicUsize,
    }

    impl MockEventCoordinator {
        fn new(events: Vec<SystemEvent>) -> Self {
            Self {
                events: std::sync::Mutex::new(VecDeque::from(events)),
                enqueued: AtomicUsize::new(0),
            }
        }

        fn enqueue_count(&self) -> usize {
            self.enqueued.load(Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl EventCoordinator for MockEventCoordinator {
        async fn next_event(&self) -> Result<SystemEvent> {
            let mut events = self.events.lock().unwrap();
            events.pop_front().ok_or_else(|| anyhow::anyhow!("No more events"))
        }

        async fn enqueue_event(&self, event: SystemEvent) -> Result<()> {
            self.enqueued.fetch_add(1, Ordering::Relaxed);
            self.events.lock().unwrap().push_back(event);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_capturing_next_event() {
        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        let mock = Arc::new(MockEventCoordinator::new(vec![timer_event]));
        let collector = Arc::new(InMemoryEventCollector::new());
        let capturing = CapturingEventCoordinator::new(mock, collector.clone());

        // Get the event
        let event = capturing.next_event().await.unwrap();
        assert!(matches!(event, SystemEvent::Timer(_)));

        // Verify it was recorded
        assert_eq!(collector.len(), 1);
        assert_eq!(collector.events()[0].event_type, "Timer");
    }

    #[tokio::test]
    async fn test_capturing_enqueue_event() {
        let mock = Arc::new(MockEventCoordinator::new(vec![]));
        let collector = Arc::new(InMemoryEventCollector::new());
        let capturing = CapturingEventCoordinator::new(mock, collector.clone());

        // Enqueue an event
        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        capturing.enqueue_event(timer_event).await.unwrap();

        // Enqueue should not record; recording happens on delivery via next_event().
        assert_eq!(collector.len(), 0);

        let event = capturing.next_event().await.unwrap();
        assert!(matches!(event, SystemEvent::Timer(_)));

        // Verify it was recorded exactly once on egress.
        assert_eq!(collector.len(), 1);
        assert_eq!(collector.events()[0].event_type, "Timer");

        // Verify it was passed to inner coordinator
        assert_eq!(capturing.inner().enqueue_count(), 1);
    }

    #[tokio::test]
    async fn test_capturing_with_noop_coordinator() {
        let noop = Arc::new(NoopEventCoordinator {});
        let collector = Arc::new(InMemoryEventCollector::new());
        let capturing = CapturingEventCoordinator::new(noop, collector.clone());

        // Enqueue should work
        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        capturing.enqueue_event(timer_event).await.unwrap();

        // Noop coordinator drops enqueued events and never emits them.
        // Egress-only capture therefore records nothing.
        assert_eq!(collector.len(), 0);
    }

    #[tokio::test]
    async fn test_collector_accessor() {
        let noop = Arc::new(NoopEventCoordinator {});
        let collector = Arc::new(InMemoryEventCollector::new());
        let capturing = CapturingEventCoordinator::new(noop, collector.clone());

        // Record directly to collector
        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        capturing.collector().record(&timer_event);

        // Verify via accessor
        assert_eq!(capturing.collector().len(), 1);
    }

    #[tokio::test]
    async fn test_capturing_with_logical_time_fn() {
        use std::sync::atomic::{AtomicI64, Ordering};

        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        let mock = Arc::new(MockEventCoordinator::new(vec![timer_event]));
        let collector = Arc::new(InMemoryEventCollector::new());

        // Logical time starts at a specific point
        let logical_millis = AtomicI64::new(
            DateTime::parse_from_rfc3339("2025-11-24T14:30:00Z")
                .unwrap()
                .timestamp_millis(),
        );
        let logical_time_fn = Arc::new(move || {
            DateTime::from_timestamp_millis(logical_millis.load(Ordering::Relaxed))
                .unwrap_or_else(Utc::now)
        });

        let capturing = CapturingEventCoordinator::with_logical_time_fn(
            mock,
            collector.clone(),
            logical_time_fn,
        );

        // Get the event
        let event = capturing.next_event().await.unwrap();
        assert!(matches!(event, SystemEvent::Timer(_)));

        // Verify it was recorded with logical time
        let events = collector.events();
        assert_eq!(events.len(), 1);
        assert!(events[0].logical_time.is_some());
        assert!(
            events[0]
                .logical_time
                .as_ref()
                .unwrap()
                .contains("2025-11-24")
        );
    }

    #[tokio::test]
    async fn test_capturing_without_logical_time_fn() {
        let timer_event = SystemEvent::Timer(TimerEvent {
            timestamp: Utc::now(),
        });
        let mock = Arc::new(MockEventCoordinator::new(vec![timer_event]));
        let collector = Arc::new(InMemoryEventCollector::new());

        let capturing = CapturingEventCoordinator::new(mock, collector.clone());

        // Get the event
        capturing.next_event().await.unwrap();

        // Verify it was recorded without logical time
        let events = collector.events();
        assert_eq!(events.len(), 1);
        assert!(events[0].logical_time.is_none());
    }
}
