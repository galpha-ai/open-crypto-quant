use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::position::Position;

#[derive(Debug, Clone, PartialEq)]
pub enum PositionEvent {
    PositionCreated {
        position: Position,
        /// Available quote currency balance after this position was created
        available_quote: f64,
    },
    PositionUpdated {
        position: Position,
        source: PositionUpdateSource,
        /// Available quote currency balance after this update
        available_quote: f64,
    },
    PositionClosed {
        position: Position,
        realized_pnl_sol: Option<f64>,
        pnl_pct: Option<f64>,
        holding_period: chrono::Duration,
        signal_id: Option<String>, // ID of the signal that initially opened the position
        exit_reason: ExitReason,   // Reason why the position was closed
        /// Available quote currency balance after this position was closed
        available_quote: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionClosedEvent {
    pub position_id: String,
    pub signal_id: String,
    pub mint: String,
    pub opened_at: DateTime<Utc>,
    pub closed_at: DateTime<Utc>,
    pub buy_price: f64,
    pub sell_price: f64,
    pub quantity: f64,
    pub pnl_sol: f64,
    pub pnl_percentage: f64,
    pub exit_reason: ExitReason,
    pub holding_duration_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExitReason {
    TakeProfit,
    StopLoss,
    ManualClose,
    MaxSellFailures,
    Timeout,
    /// Position closed via pair redemption (binary market UP+DOWN)
    Redemption,
    Other(String),
}

/// Source of a position update
///
/// Tracks why and how a position was updated, enabling strategies
/// to handle different update types appropriately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PositionUpdateSource {
    /// Position updated due to an order fill
    Fill {
        /// Price at which the fill occurred
        fill_price: f64,
        /// Size of the fill (always positive)
        fill_size: f64,
        /// Side of the order that was filled
        side: OrderSide,
        /// Venue-assigned order ID if available
        order_id: Option<String>,
    },
    /// Position updated from periodic reconciliation with exchange
    Reconciliation {
        /// Position amount before reconciliation
        previous_amount: f64,
        /// Difference between expected and actual (can be negative)
        drift: f64,
    },
    /// Position updated due to pair redemption (binary market UP+DOWN)
    Redemption {
        /// Position amount before redemption
        previous_amount: f64,
        /// Quantity redeemed from this side
        redeemed_quantity: f64,
    },
    /// Manual adjustment (e.g., from API call)
    Manual,
    /// Price update only (no position size change)
    PriceUpdate,
}

/// Side of an order (buy or sell)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn create_test_position_closed_event(exit_reason: ExitReason) -> PositionClosedEvent {
        let opened_at = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let closed_at = Utc.with_ymd_and_hms(2024, 1, 1, 12, 5, 30).unwrap();

        PositionClosedEvent {
            position_id: "pos_123".to_string(),
            signal_id: "sig_456".to_string(),
            mint: "test_mint_address".to_string(),
            opened_at,
            closed_at,
            buy_price: 0.00001,
            sell_price: 0.000015,
            quantity: 1000.0,
            pnl_sol: 0.5,
            pnl_percentage: 50.0,
            exit_reason,
            holding_duration_seconds: 330, // 5 minutes 30 seconds
        }
    }

    #[test]
    fn test_position_closed_event_serialization() {
        let event = create_test_position_closed_event(ExitReason::TakeProfit);
        let json = serde_json::to_value(&event).unwrap();

        // Verify all fields are present
        assert_eq!(json.get("position_id").unwrap(), "pos_123");
        assert_eq!(json.get("signal_id").unwrap(), "sig_456");
        assert_eq!(json.get("mint").unwrap(), "test_mint_address");
        assert!(json.get("opened_at").is_some());
        assert!(json.get("closed_at").is_some());
        assert_eq!(json.get("buy_price").unwrap(), 0.00001);
        assert_eq!(json.get("sell_price").unwrap(), 0.000015);
        assert_eq!(json.get("quantity").unwrap(), 1000.0);
        assert_eq!(json.get("pnl_sol").unwrap(), 0.5);
        assert_eq!(json.get("pnl_percentage").unwrap(), 50.0);
        assert_eq!(json.get("exit_reason").unwrap(), "take_profit");
        assert_eq!(json.get("holding_duration_seconds").unwrap(), 330);
    }

    #[test]
    fn test_position_closed_event_deserialization() {
        let json = serde_json::json!({
            "position_id": "pos_123",
            "signal_id": "sig_456",
            "mint": "test_mint_address",
            "opened_at": "2024-01-01T12:00:00Z",
            "closed_at": "2024-01-01T12:05:30Z",
            "buy_price": 0.00001,
            "sell_price": 0.000015,
            "quantity": 1000.0,
            "pnl_sol": 0.5,
            "pnl_percentage": 50.0,
            "exit_reason": "stop_loss",
            "holding_duration_seconds": 330
        });

        let event: PositionClosedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(event.position_id, "pos_123");
        assert_eq!(event.signal_id, "sig_456");
        assert!(matches!(event.exit_reason, ExitReason::StopLoss));
    }

    #[test]
    fn test_exit_reason_serialization() {
        // Test all exit reason variants
        let test_cases = vec![
            (ExitReason::TakeProfit, "\"take_profit\""),
            (ExitReason::StopLoss, "\"stop_loss\""),
            (ExitReason::ManualClose, "\"manual_close\""),
            (ExitReason::MaxSellFailures, "\"max_sell_failures\""),
            (ExitReason::Timeout, "\"timeout\""),
            (ExitReason::Redemption, "\"redemption\""),
            (
                ExitReason::Other("custom_reason".to_string()),
                "{\"other\":\"custom_reason\"}",
            ),
        ];

        for (exit_reason, expected) in test_cases {
            let json = serde_json::to_string(&exit_reason).unwrap();
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn test_exit_reason_deserialization() {
        // Test deserialization of all variants
        assert!(matches!(
            serde_json::from_str::<ExitReason>("\"take_profit\"").unwrap(),
            ExitReason::TakeProfit
        ));
        assert!(matches!(
            serde_json::from_str::<ExitReason>("\"stop_loss\"").unwrap(),
            ExitReason::StopLoss
        ));
        assert!(matches!(
            serde_json::from_str::<ExitReason>("\"manual_close\"").unwrap(),
            ExitReason::ManualClose
        ));
        assert!(matches!(
            serde_json::from_str::<ExitReason>("\"max_sell_failures\"").unwrap(),
            ExitReason::MaxSellFailures
        ));
        assert!(matches!(
            serde_json::from_str::<ExitReason>("\"timeout\"").unwrap(),
            ExitReason::Timeout
        ));
        assert!(matches!(
            serde_json::from_str::<ExitReason>("\"redemption\"").unwrap(),
            ExitReason::Redemption
        ));

        // Test Other variant
        let other = serde_json::from_str::<ExitReason>("{\"other\":\"custom\"}").unwrap();
        match other {
            ExitReason::Other(reason) => assert_eq!(reason, "custom"),
            _ => panic!("Expected Other variant"),
        }
    }

    #[test]
    fn test_position_closed_event_datetime_format() {
        let event = create_test_position_closed_event(ExitReason::TakeProfit);
        let json = serde_json::to_value(&event).unwrap();

        // Check that DateTime fields are serialized in ISO 8601 format
        let opened_at = json.get("opened_at").unwrap().as_str().unwrap();
        assert!(opened_at.ends_with("Z")); // UTC timezone indicator
        assert!(opened_at.contains("2024-01-01T12:00:00"));

        let closed_at = json.get("closed_at").unwrap().as_str().unwrap();
        assert!(closed_at.ends_with("Z"));
        assert!(closed_at.contains("2024-01-01T12:05:30"));
    }

    #[test]
    fn test_position_closed_event_with_negative_pnl() {
        let mut event = create_test_position_closed_event(ExitReason::StopLoss);
        event.sell_price = 0.000005; // Lower than buy price
        event.pnl_sol = -0.5;
        event.pnl_percentage = -50.0;

        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json.get("pnl_sol").unwrap(), -0.5);
        assert_eq!(json.get("pnl_percentage").unwrap(), -50.0);
        assert_eq!(json.get("exit_reason").unwrap(), "stop_loss");
    }

    #[test]
    fn test_position_closed_event_roundtrip() {
        // Test that serialization and deserialization preserve all data
        let original =
            create_test_position_closed_event(ExitReason::Other("market_crash".to_string()));
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: PositionClosedEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(original.position_id, deserialized.position_id);
        assert_eq!(original.signal_id, deserialized.signal_id);
        assert_eq!(original.mint, deserialized.mint);
        assert_eq!(original.opened_at, deserialized.opened_at);
        assert_eq!(original.closed_at, deserialized.closed_at);
        assert_eq!(original.buy_price, deserialized.buy_price);
        assert_eq!(original.sell_price, deserialized.sell_price);
        assert_eq!(original.quantity, deserialized.quantity);
        assert_eq!(original.pnl_sol, deserialized.pnl_sol);
        assert_eq!(original.pnl_percentage, deserialized.pnl_percentage);
        assert_eq!(
            original.holding_duration_seconds,
            deserialized.holding_duration_seconds
        );

        match (&original.exit_reason, &deserialized.exit_reason) {
            (ExitReason::Other(orig), ExitReason::Other(deser)) => assert_eq!(orig, deser),
            _ => panic!("Exit reason mismatch"),
        }
    }
}
