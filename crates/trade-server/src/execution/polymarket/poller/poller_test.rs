//! Tests for OrderStatusPoller.

use std::time::Duration;

use crate::execution::events::OrderSide;

use super::config::PollerConfig;
use super::types::MonitoredOrder;

#[tokio::test]
async fn test_monitored_order_lifecycle() {
    // Test the MonitoredOrder structure
    let order = MonitoredOrder::new(
        "order-123".to_string(),
        "token-abc".to_string(),
        Some("market-xyz".to_string()),
        OrderSide::Buy,
        0.5,
        100.0,
        Some("signal-1".to_string()),
    );

    assert_eq!(order.remaining_size(), 100.0);
    assert!(!order.is_fully_filled());
    assert_eq!(order.poll_count, 0);
}

#[tokio::test]
async fn test_config_backoff() {
    let config = PollerConfig::default()
        .with_poll_interval(Duration::from_millis(100))
        .with_failure_backoff_multiplier(2.0);

    assert_eq!(config.calculate_backoff(0), Duration::from_millis(100));
    assert_eq!(config.calculate_backoff(1), Duration::from_millis(200));
    assert_eq!(config.calculate_backoff(2), Duration::from_millis(400));
    assert_eq!(config.calculate_backoff(3), Duration::from_millis(800));
}

#[tokio::test]
async fn test_poll_priority() {
    let order1 = MonitoredOrder::new(
        "order-1".to_string(),
        "token".to_string(),
        None,
        OrderSide::Buy,
        0.5,
        100.0,
        None,
    );

    // Simulate some time passing
    tokio::time::sleep(Duration::from_millis(10)).await;

    let order2 = MonitoredOrder::new(
        "order-2".to_string(),
        "token".to_string(),
        None,
        OrderSide::Buy,
        0.5,
        100.0,
        None,
    );

    // order1 was added first, so should have higher priority (earlier timestamp)
    let p1 = order1.poll_priority();
    let p2 = order2.poll_priority();

    // order1.added_at < order2.added_at
    assert!(p1.0 < p2.0);
}

#[tokio::test]
async fn test_monitored_order_fill_tracking() {
    let mut order = MonitoredOrder::new(
        "order-123".to_string(),
        "token".to_string(),
        None,
        OrderSide::Buy,
        0.5,
        100.0,
        None,
    );

    // Initial state
    assert_eq!(order.last_known_filled, 0.0);
    assert_eq!(order.remaining_size(), 100.0);

    // Record partial fill
    order.record_poll_success(25.0);
    assert_eq!(order.last_known_filled, 25.0);
    assert_eq!(order.remaining_size(), 75.0);
    assert!(!order.is_fully_filled());

    // Record more fill
    order.record_poll_success(75.0);
    assert_eq!(order.last_known_filled, 75.0);
    assert_eq!(order.remaining_size(), 25.0);

    // Record full fill
    order.record_poll_success(100.0);
    assert_eq!(order.last_known_filled, 100.0);
    assert_eq!(order.remaining_size(), 0.0);
    assert!(order.is_fully_filled());
}

#[tokio::test]
async fn test_consecutive_failure_tracking() {
    let mut order = MonitoredOrder::new(
        "order-123".to_string(),
        "token".to_string(),
        None,
        OrderSide::Buy,
        0.5,
        100.0,
        None,
    );

    assert_eq!(order.consecutive_failures, 0);

    // Record failures
    order.record_poll_failure();
    assert_eq!(order.consecutive_failures, 1);

    order.record_poll_failure();
    assert_eq!(order.consecutive_failures, 2);

    // Success resets failures
    order.record_poll_success(0.0);
    assert_eq!(order.consecutive_failures, 0);

    // Can fail again
    order.record_poll_failure();
    assert_eq!(order.consecutive_failures, 1);
}
