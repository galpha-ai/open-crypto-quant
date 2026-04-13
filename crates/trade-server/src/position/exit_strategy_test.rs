#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::position::ExitMode;
    use chrono::{DateTime, Duration, Utc};
    use solana_sdk::signature::Signature;

    fn create_test_position(
        pnl_pct: Option<f64>,
        entry_time: DateTime<Utc>,
        sell_failure_count: u32,
    ) -> Position {
        Position {
            mint: "TEST".to_string(),
            amount: 100.0,
            entry_price: Some(1.0),
            current_price: Some(1.0),
            current_price_updated_time: entry_time,
            pnl_pct,
            entry_time,
            entry_slot: 0,
            entry_signature: Signature::default(),
            sell_failure_count,
            signal_id: None,
            active_exit_order: None,
            exit_mode: ExitMode::Automatic,
        }
    }

    #[test]
    fn test_take_profit_threshold() {
        let strategy = ConfigurableExitStrategy::new(
            0.2, // 20% take profit
            0.1, // 10% stop loss
            Duration::hours(24),
            10,
        );

        let now = Utc::now();

        // Position with 25% profit should exit
        let position = create_test_position(Some(25.0), now - Duration::hours(1), 0);
        assert!(strategy.should_exit(&position, now));
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::TakeProfit
        );

        // Position with 19% profit should not exit
        let position = create_test_position(Some(19.0), now - Duration::hours(1), 0);
        assert!(!strategy.should_exit(&position, now));

        // Position with exactly 20% profit should exit
        let position = create_test_position(Some(20.0), now - Duration::hours(1), 0);
        assert!(strategy.should_exit(&position, now));
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::TakeProfit
        );
    }

    #[test]
    fn test_stop_loss_threshold() {
        let strategy = ConfigurableExitStrategy::new(
            0.2, // 20% take profit
            0.1, // 10% stop loss
            Duration::hours(24),
            10,
        );

        let now = Utc::now();

        // Position with -15% loss should exit
        let position = create_test_position(Some(-15.0), now - Duration::hours(1), 0);
        assert!(strategy.should_exit(&position, now));
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::StopLoss
        );

        // Position with -9% loss should not exit
        let position = create_test_position(Some(-9.0), now - Duration::hours(1), 0);
        assert!(!strategy.should_exit(&position, now));

        // Position with exactly -10% loss should exit
        let position = create_test_position(Some(-10.0), now - Duration::hours(1), 0);
        assert!(strategy.should_exit(&position, now));
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::StopLoss
        );
    }

    #[test]
    fn test_holding_period_timeout() {
        let strategy = ConfigurableExitStrategy::new(0.2, 0.1, Duration::hours(24), 10);

        let now = Utc::now();

        // Position held for 25 hours should exit
        let position = create_test_position(Some(5.0), now - Duration::hours(25), 0);
        assert!(strategy.should_exit(&position, now));
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::Timeout
        );

        // Position held for 23 hours should not exit
        let position = create_test_position(Some(5.0), now - Duration::hours(23), 0);
        assert!(!strategy.should_exit(&position, now));

        // Position held for exactly 24 hours should exit
        let position = create_test_position(Some(5.0), now - Duration::hours(24), 0);
        assert!(strategy.should_exit(&position, now));
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::Timeout
        );
    }

    #[test]
    fn test_sell_failure_threshold() {
        let strategy = ConfigurableExitStrategy::new(
            0.2,
            0.1,
            Duration::hours(24),
            3, // Max 3 sell failures
        );

        let now = Utc::now();

        // Position with 4 sell failures should exit
        let position = create_test_position(Some(5.0), now - Duration::hours(1), 4);
        assert!(strategy.should_exit(&position, now));
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::MaxSellFailures
        );

        // Position with 2 sell failures should not exit
        let position = create_test_position(Some(5.0), now - Duration::hours(1), 2);
        assert!(!strategy.should_exit(&position, now));

        // Position with exactly 3 sell failures should exit
        let position = create_test_position(Some(5.0), now - Duration::hours(1), 3);
        assert!(strategy.should_exit(&position, now));
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::MaxSellFailures
        );
    }

    #[test]
    fn test_exit_reason_priority() {
        let strategy = ConfigurableExitStrategy::new(0.2, 0.1, Duration::hours(24), 3);

        let now = Utc::now();

        // Position with multiple exit conditions - take profit takes precedence
        let position = create_test_position(Some(25.0), now - Duration::hours(25), 5);
        assert!(strategy.should_exit(&position, now));
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::TakeProfit
        );

        // Position with stop loss and timeout - stop loss takes precedence
        let position = create_test_position(Some(-15.0), now - Duration::hours(25), 5);
        assert!(strategy.should_exit(&position, now));
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::StopLoss
        );

        // Position with timeout and sell failures - timeout takes precedence
        let position = create_test_position(Some(5.0), now - Duration::hours(25), 5);
        assert!(strategy.should_exit(&position, now));
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::Timeout
        );
    }

    #[test]
    fn test_no_exit_conditions() {
        let strategy = ConfigurableExitStrategy::new(0.2, 0.1, Duration::hours(24), 10);

        let now = Utc::now();

        // Position with no exit conditions
        let position = create_test_position(Some(5.0), now - Duration::hours(1), 0);
        assert!(!strategy.should_exit(&position, now));
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::ManualClose
        );
    }

    #[test]
    fn test_position_with_no_pnl() {
        let strategy = ConfigurableExitStrategy::new(0.2, 0.1, Duration::hours(24), 10);

        let now = Utc::now();

        // Position with no PnL data should only check other conditions
        let position = create_test_position(None, now - Duration::hours(1), 0);
        assert!(!strategy.should_exit(&position, now));

        // Position with no PnL but timeout should exit
        let position = create_test_position(None, now - Duration::hours(25), 0);
        assert!(strategy.should_exit(&position, now));
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::Timeout
        );
    }

    #[test]
    fn test_noop_strategy_never_exits() {
        let strategy = NoopExitStrategy::new();
        let now = Utc::now();

        // Test with profitable position
        let position = create_test_position(Some(100.0), now - Duration::hours(100), 0);
        assert!(!strategy.should_exit(&position, now));

        // Test with losing position
        let position = create_test_position(Some(-90.0), now - Duration::hours(100), 0);
        assert!(!strategy.should_exit(&position, now));

        // Test with very old position
        let position = create_test_position(Some(5.0), now - Duration::days(365), 0);
        assert!(!strategy.should_exit(&position, now));

        // Test with max sell failures
        let position = create_test_position(Some(5.0), now - Duration::hours(1), 999);
        assert!(!strategy.should_exit(&position, now));

        // get_exit_reason should always return ManualClose
        assert_eq!(
            strategy.get_exit_reason(&position, now),
            ExitReason::ManualClose
        );
    }

    #[test]
    fn test_noop_strategy_default() {
        let strategy = NoopExitStrategy::default();
        let now = Utc::now();
        let position = create_test_position(Some(50.0), now, 0);
        assert!(!strategy.should_exit(&position, now));
    }
}
