use crate::position::{ExitReason, Position};
use chrono::{DateTime, Duration, Utc};
use tracing::info;

/// Trait for determining when positions should be exited
pub trait ExitStrategy: Send + Sync {
    /// Check if a position should be exited based on current conditions
    fn should_exit(&self, position: &Position, current_time: DateTime<Utc>) -> bool;

    /// Get the exit reason for a position
    fn get_exit_reason(&self, position: &Position, current_time: DateTime<Utc>) -> ExitReason;
}

/// Configurable exit strategy with thresholds
pub struct ConfigurableExitStrategy {
    /// Take profit threshold as a percentage (e.g., 0.2 for 20%)
    pub take_profit_threshold: f64,
    /// Stop loss threshold as a percentage (e.g., 0.1 for 10%)
    pub stop_loss_threshold: f64,
    /// Maximum holding period before forced exit
    pub max_holding_period: Duration,
    /// Maximum number of sell failures before forced exit
    pub max_sell_failures: u32,
}

impl ConfigurableExitStrategy {
    pub fn new(
        take_profit_threshold: f64,
        stop_loss_threshold: f64,
        max_holding_period: Duration,
        max_sell_failures: u32,
    ) -> Self {
        info!(
            take_profit_threshold = take_profit_threshold,
            stop_loss_threshold = stop_loss_threshold,
            max_holding_period_secs = max_holding_period.num_seconds(),
            max_sell_failures = max_sell_failures,
            "Creating ConfigurableExitStrategy"
        );

        Self {
            take_profit_threshold,
            stop_loss_threshold,
            max_holding_period,
            max_sell_failures,
        }
    }
}

impl ExitStrategy for ConfigurableExitStrategy {
    fn should_exit(&self, position: &Position, current_time: DateTime<Utc>) -> bool {
        // Check PnL thresholds
        if let Some(pnl_pct) = position.pnl_pct {
            let pnl_percent = pnl_pct / 100.0; // Convert from percentage to decimal
            if pnl_percent >= self.take_profit_threshold {
                info!(
                    mint = %position.mint,
                    entry_signature = %position.entry_signature,
                    signal_id = ?position.signal_id,
                    current_pnl_pct = pnl_pct,
                    take_profit_threshold = self.take_profit_threshold,
                    "Position should exit: take profit threshold reached"
                );
                return true;
            }
            if pnl_percent <= -self.stop_loss_threshold {
                info!(
                    mint = %position.mint,
                    entry_signature = %position.entry_signature,
                    signal_id = ?position.signal_id,
                    current_pnl_pct = pnl_pct,
                    stop_loss_threshold = self.stop_loss_threshold,
                    "Position should exit: stop loss threshold reached"
                );
                return true;
            }
        }

        // Check holding period
        let holding_duration = current_time - position.entry_time;
        if holding_duration >= self.max_holding_period {
            info!(
                mint = %position.mint,
                entry_signature = %position.entry_signature,
                signal_id = ?position.signal_id,
                holding_duration_secs = holding_duration.num_seconds(),
                max_holding_period_secs = self.max_holding_period.num_seconds(),
                "Position should exit: maximum holding period reached"
            );
            return true;
        }

        // Check sell failures
        if position.sell_failure_count >= self.max_sell_failures {
            info!(
                mint = %position.mint,
                entry_signature = %position.entry_signature,
                signal_id = ?position.signal_id,
                sell_failure_count = position.sell_failure_count,
                max_sell_failures = self.max_sell_failures,
                "Position should exit: maximum sell failures reached"
            );
            return true;
        }

        false
    }

    fn get_exit_reason(&self, position: &Position, current_time: DateTime<Utc>) -> ExitReason {
        // Check PnL thresholds
        if let Some(pnl_pct) = position.pnl_pct {
            let pnl_percent = pnl_pct / 100.0; // Convert from percentage to decimal

            // Check take profit first
            if pnl_percent >= self.take_profit_threshold {
                return ExitReason::TakeProfit;
            }

            // Check stop loss
            if pnl_percent <= -self.stop_loss_threshold {
                return ExitReason::StopLoss;
            }
        }

        // Check holding period
        let holding_duration = current_time - position.entry_time;
        if holding_duration >= self.max_holding_period {
            return ExitReason::Timeout;
        }

        // Check sell failures
        if position.sell_failure_count >= self.max_sell_failures {
            return ExitReason::MaxSellFailures;
        }

        // Default to manual close if no other reason applies
        ExitReason::ManualClose
    }
}

/// No-operation exit strategy that never triggers automatic exits.
/// Positions using this strategy will only exit through:
/// - Manual close requests
/// - Force exit due to max sell failures (handled separately)
/// - External signals with explicit sell orders
pub struct NoopExitStrategy;

impl NoopExitStrategy {
    pub fn new() -> Self {
        info!("Creating NoopExitStrategy - positions will NEVER auto-exit");
        Self
    }
}

impl Default for NoopExitStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl ExitStrategy for NoopExitStrategy {
    fn should_exit(&self, _position: &Position, _current_time: DateTime<Utc>) -> bool {
        // Never trigger automatic exit
        false
    }

    fn get_exit_reason(&self, _position: &Position, _current_time: DateTime<Utc>) -> ExitReason {
        // This should never be called since should_exit always returns false
        // But provide a reasonable default for safety
        ExitReason::ManualClose
    }
}

#[cfg(test)]
#[path = "exit_strategy_test.rs"]
mod exit_strategy_test;
