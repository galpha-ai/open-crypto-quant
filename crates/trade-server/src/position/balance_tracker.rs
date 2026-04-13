//! Balance and trade statistics tracking module.
//!
//! This module handles quote currency balance management, cash flow tracking,
//! and aggregate trade statistics.

use chrono::{DateTime, Utc};
use tracing::{debug, info};

use crate::position::Position;
use crate::position::state::PositionManagerState;

/// Updates the quote balance and cash flow tracking based on a trade execution.
///
/// # Arguments
///
/// * `state` - The position manager state to update
/// * `token_amount_change` - Positive for buys, negative for sells
/// * `quote_amount_change` - Optional direct quote amount change from execution
/// * `price` - Token price (used if quote_amount_change is None)
pub fn update_quote_balance(
    state: &mut PositionManagerState,
    token_amount_change: f64,
    quote_amount_change: &Option<f64>,
    price: Option<f64>,
) {
    // Calculate quote change
    let quote_change = match quote_amount_change {
        Some(amount) => amount.abs(),
        None => {
            if price.is_none() {
                tracing::error!(
                    "Cannot calculate quote change - both quote_amount_change and price are None"
                );
                0.0
            } else {
                token_amount_change.abs() * price.unwrap()
            }
        }
    };

    // Update balance and cash flow tracking based on trade direction
    if token_amount_change > 0.0 {
        // Buy - quote flows out
        state.available_quote -= quote_change;
        state.total_quote_spent += quote_change;
        debug!(
            quote_spent = quote_change,
            total_quote_spent = state.total_quote_spent,
            available_quote = state.available_quote,
            "Updated quote balance after buy"
        );
    } else {
        // Sell - quote flows in
        state.available_quote += quote_change;
        state.total_quote_received += quote_change;
        debug!(
            quote_received = quote_change,
            total_quote_received = state.total_quote_received,
            available_quote = state.available_quote,
            "Updated quote balance after sell"
        );
    }
}

/// Updates trade statistics after a position is closed.
///
/// # Arguments
///
/// * `state` - The position manager state to update
/// * `position` - The closed position
/// * `timestamp` - When the position was closed
pub fn update_trade_statistics(
    state: &mut PositionManagerState,
    position: &Position,
    timestamp: DateTime<Utc>,
) {
    state.total_closed_positions += 1;

    if position.pnl_pct > Some(0.0) {
        state.winning_trades += 1;
    }

    // Calculate net cash flow for logging
    let net_cash_flow = state.total_quote_received - state.total_quote_spent;

    info!(
        mint = position.mint,
        entry_price = position.entry_price,
        exit_price = position.current_price,
        pnl_pct = position.pnl_pct,
        holding_period = %timestamp.signed_duration_since(position.entry_time),
        "Trade closed"
    );

    info!(
        mint = position.mint,
        net_cash_flow = net_cash_flow,
        total_quote_received = state.total_quote_received,
        total_quote_spent = state.total_quote_spent,
        total_trades = state.total_closed_positions,
        winning_trades = state.winning_trades,
        win_rate_pct = (state.winning_trades as f64 / state.total_closed_positions as f64) * 100.0,
        "Strategy performance update"
    );
}

/// Calculate PnL percentage for a position.
///
/// # Arguments
///
/// * `position` - The position to calculate PnL for
///
/// # Returns
///
/// PnL percentage, or None if entry or current price is unavailable
pub fn calculate_pnl_pct(position: &Position) -> Option<f64> {
    match (position.current_price, position.entry_price) {
        (Some(current), Some(entry)) if entry != 0.0 => Some(((current - entry) / entry) * 100.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::ExitMode;
    use solana_sdk::signature::Signature;

    fn create_test_position(mint: &str, entry_price: f64, current_price: f64) -> Position {
        Position {
            mint: mint.to_string(),
            amount: 100.0,
            entry_price: Some(entry_price),
            current_price: Some(current_price),
            current_price_updated_time: Utc::now(),
            pnl_pct: Some(((current_price - entry_price) / entry_price) * 100.0),
            entry_time: Utc::now(),
            entry_slot: 0,
            entry_signature: Signature::default(),
            signal_id: None,
            sell_failure_count: 0,
            active_exit_order: None,
            exit_mode: ExitMode::Automatic,
        }
    }

    #[test]
    fn test_update_quote_balance_buy() {
        let mut state = PositionManagerState::new(10.0);

        // Buy 100 tokens at price 0.05 each = 5 quote spent
        update_quote_balance(&mut state, 100.0, &None, Some(0.05));

        assert!((state.available_quote - 5.0).abs() < 1e-9);
        assert!((state.total_quote_spent - 5.0).abs() < 1e-9);
        assert_eq!(state.total_quote_received, 0.0);
    }

    #[test]
    fn test_update_quote_balance_sell() {
        let mut state = PositionManagerState::new(5.0);

        // Sell 100 tokens at price 0.05 each = 5 quote received
        update_quote_balance(&mut state, -100.0, &None, Some(0.05));

        assert!((state.available_quote - 10.0).abs() < 1e-9);
        assert!((state.total_quote_received - 5.0).abs() < 1e-9);
        assert_eq!(state.total_quote_spent, 0.0);
    }

    #[test]
    fn test_update_quote_balance_with_quote_amount() {
        let mut state = PositionManagerState::new(10.0);

        // Buy with explicit quote amount of 3
        update_quote_balance(&mut state, 100.0, &Some(3.0), Some(0.05));

        assert!((state.available_quote - 7.0).abs() < 1e-9);
        assert!((state.total_quote_spent - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_cash_flow_tracking_round_trip() {
        let mut state = PositionManagerState::new(10.0);

        // Buy 100 tokens at 1.0 each = 100 spent
        update_quote_balance(&mut state, 100.0, &Some(100.0), None);
        assert!((state.available_quote - (-90.0)).abs() < 1e-9);
        assert!((state.total_quote_spent - 100.0).abs() < 1e-9);
        assert_eq!(state.total_quote_received, 0.0);

        // Sell 100 tokens at 1.2 each = 120 received
        update_quote_balance(&mut state, -100.0, &Some(120.0), None);
        assert!((state.available_quote - 30.0).abs() < 1e-9);
        assert!((state.total_quote_received - 120.0).abs() < 1e-9);
        assert!((state.total_quote_spent - 100.0).abs() < 1e-9);

        // Net cash flow should be 120 - 100 = 20 profit
        let net_cash_flow = state.total_quote_received - state.total_quote_spent;
        assert!((net_cash_flow - 20.0).abs() < 1e-9);
    }

    #[test]
    fn test_calculate_pnl_pct_profit() {
        let position = create_test_position("mint1", 0.05, 0.06);
        let pnl_pct = calculate_pnl_pct(&position);
        // (0.06 - 0.05) / 0.05 * 100 = 20%
        assert!((pnl_pct.unwrap() - 20.0).abs() < 1e-9);
    }

    #[test]
    fn test_calculate_pnl_pct_loss() {
        let position = create_test_position("mint1", 0.10, 0.08);
        let pnl_pct = calculate_pnl_pct(&position);
        // (0.08 - 0.10) / 0.10 * 100 = -20%
        assert!((pnl_pct.unwrap() - (-20.0)).abs() < 1e-9);
    }

    #[test]
    fn test_update_trade_statistics() {
        let mut state = PositionManagerState::new(10.0);
        let position = create_test_position("mint1", 0.05, 0.06);
        let timestamp = Utc::now();

        update_trade_statistics(&mut state, &position, timestamp);

        assert_eq!(state.total_closed_positions, 1);
        assert_eq!(state.winning_trades, 1);
    }

    #[test]
    fn test_update_trade_statistics_losing_trade() {
        let mut state = PositionManagerState::new(10.0);
        let position = create_test_position("mint1", 0.10, 0.08); // Loss position
        let timestamp = Utc::now();

        update_trade_statistics(&mut state, &position, timestamp);

        assert_eq!(state.total_closed_positions, 1);
        assert_eq!(state.winning_trades, 0); // Not a winner
    }

    #[test]
    fn test_market_maker_scenario() {
        // Simulate market maker: continuous buys and sells at different prices
        let mut state = PositionManagerState::new(100.0);

        // Round 1: Buy at 1.00, sell at 1.02 (spread capture)
        update_quote_balance(&mut state, 10.0, &Some(10.0), None); // Buy 10 tokens for 10.00
        update_quote_balance(&mut state, -10.0, &Some(10.2), None); // Sell 10 tokens for 10.20

        // Round 2: Buy at 1.01, sell at 1.03
        update_quote_balance(&mut state, 10.0, &Some(10.1), None); // Buy 10 tokens for 10.10
        update_quote_balance(&mut state, -10.0, &Some(10.3), None); // Sell 10 tokens for 10.30

        // Net cash flow = (10.2 + 10.3) - (10.0 + 10.1) = 20.5 - 20.1 = 0.4
        let net_cash_flow = state.total_quote_received - state.total_quote_spent;
        assert!((net_cash_flow - 0.4).abs() < 1e-9);
        assert!((state.total_quote_received - 20.5).abs() < 1e-9);
        assert!((state.total_quote_spent - 20.1).abs() < 1e-9);
    }
}
