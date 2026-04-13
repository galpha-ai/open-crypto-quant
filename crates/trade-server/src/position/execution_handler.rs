//! Execution event handling module.
//!
//! This module handles position updates based on execution events,
//! including position creation, updates, and closures.

use chrono::{DateTime, Utc};
use solana_sdk::signature::Signature;
use tracing::error;

use crate::position::state::PositionManagerState;
use crate::position::{ExitMode, Position};

/// Get or create a position for the given mint.
///
/// If the position exists, returns a clone of it. Otherwise creates a new
/// position with the provided parameters.
///
/// # Arguments
///
/// * `state` - The position manager state
/// * `mint` - Token mint address
/// * `price` - Current token price
/// * `timestamp` - Event timestamp
/// * `signal_id` - Optional signal ID that triggered this position
/// * `confirmed_slot` - Optional slot when confirmed
/// * `confirmed_signature` - Optional transaction signature
/// * `exit_mode` - Optional exit mode for the position (defaults to Automatic)
///
/// # Returns
///
/// A tuple of (Position, was_new_position)
pub fn get_or_create_position(
    state: &PositionManagerState,
    mint: &str,
    price: Option<f64>,
    timestamp: DateTime<Utc>,
    signal_id: Option<String>,
    confirmed_slot: Option<u64>,
    confirmed_signature: Option<Signature>,
    exit_mode: Option<ExitMode>,
) -> (Position, bool) {
    if let Some(pos) = state.get_position(mint) {
        (pos.clone(), false)
    } else {
        (
            Position {
                mint: mint.to_string(),
                amount: 0.0,
                entry_price: Some(0.0),
                current_price: price,
                current_price_updated_time: timestamp,
                pnl_pct: Some(0.0),
                entry_time: timestamp,
                entry_slot: confirmed_slot.unwrap_or(0),
                entry_signature: confirmed_signature.unwrap_or_default(),
                signal_id,
                sell_failure_count: 0,
                active_exit_order: None,
                exit_mode: exit_mode.unwrap_or(ExitMode::Automatic),
            },
            true,
        )
    }
}

/// Update a position after a buy.
///
/// Calculates the new average entry price based on the existing position
/// and the new purchase.
///
/// # Arguments
///
/// * `position` - The position to update
/// * `token_amount_change` - Amount of tokens purchased (positive)
/// * `price` - Price at which tokens were purchased
pub fn update_position_for_buy(
    position: &mut Position,
    token_amount_change: f64,
    price: Option<f64>,
) {
    let (price, entry_price) = match (price, position.entry_price) {
        (Some(p), Some(ep)) => (p, ep),
        _ => {
            error!(
                "Cannot update position for buy - price or entry_price is None for mint {}",
                position.mint
            );
            return;
        }
    };

    // Calculate total value using the ORIGINAL position amount
    let original_amount = position.amount;
    let total_value = original_amount * entry_price + token_amount_change * price;

    // Now update the position amount
    position.amount += token_amount_change;

    // Calculate new average price
    position.entry_price = Some(total_value / position.amount);
}

/// Update a position after a sell.
///
/// Reduces the position amount by the sold amount. If `clear_position` is true,
/// the position amount is set to zero to avoid rounding errors.
///
/// # Arguments
///
/// * `position` - The position to update
/// * `token_amount_change` - Amount of tokens sold (negative)
/// * `clear_position` - If true, clear position entirely to avoid dust
pub fn update_position_for_sell(
    position: &mut Position,
    token_amount_change: f64,
    clear_position: bool,
) {
    if clear_position {
        // If clear_position is true, set amount to zero to avoid rounding errors
        position.amount = 0.0;
    } else {
        // Otherwise just reduce the amount as normal
        position.amount += token_amount_change;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_position(mint: &str, amount: f64, entry_price: f64) -> Position {
        Position {
            mint: mint.to_string(),
            amount,
            entry_price: Some(entry_price),
            current_price: Some(entry_price),
            current_price_updated_time: Utc::now(),
            pnl_pct: Some(0.0),
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
    fn test_get_or_create_position_existing() {
        let mut state = PositionManagerState::new(10.0);
        let existing = create_test_position("mint1", 100.0, 0.05);
        state
            .positions
            .insert("mint1".to_string(), existing.clone());

        let (pos, is_new) = get_or_create_position(
            &state,
            "mint1",
            Some(0.06),
            Utc::now(),
            None,
            None,
            None,
            None,
        );

        assert!(!is_new);
        assert_eq!(pos.mint, "mint1");
        assert_eq!(pos.amount, 100.0);
    }

    #[test]
    fn test_get_or_create_position_new() {
        let state = PositionManagerState::new(10.0);

        let (pos, is_new) = get_or_create_position(
            &state,
            "mint1",
            Some(0.05),
            Utc::now(),
            Some("signal1".to_string()),
            Some(12345),
            None,
            None,
        );

        assert!(is_new);
        assert_eq!(pos.mint, "mint1");
        assert_eq!(pos.amount, 0.0);
        assert_eq!(pos.current_price, Some(0.05));
        assert_eq!(pos.signal_id, Some("signal1".to_string()));
        assert_eq!(pos.entry_slot, 12345);
        assert_eq!(pos.exit_mode, ExitMode::Automatic);
    }

    #[test]
    fn test_get_or_create_position_with_strategy_managed_exit_mode() {
        let state = PositionManagerState::new(10.0);

        let (pos, is_new) = get_or_create_position(
            &state,
            "mint1",
            Some(0.05),
            Utc::now(),
            Some("signal1".to_string()),
            Some(12345),
            None,
            Some(ExitMode::StrategyManaged),
        );

        assert!(is_new);
        assert_eq!(pos.mint, "mint1");
        assert_eq!(pos.exit_mode, ExitMode::StrategyManaged);
    }

    #[test]
    fn test_update_position_for_buy_first_buy() {
        let mut position = create_test_position("mint1", 0.0, 0.0);

        update_position_for_buy(&mut position, 100.0, Some(0.05));

        assert_eq!(position.amount, 100.0);
        // entry_price should be NaN because we divided by 0, but we need to handle the initial case
        // Actually the function guards against this - the entry_price is 0.0 initially
    }

    #[test]
    fn test_update_position_for_buy_average_price() {
        let mut position = create_test_position("mint1", 100.0, 0.05);

        // Buy 100 more tokens at 0.07
        update_position_for_buy(&mut position, 100.0, Some(0.07));

        assert_eq!(position.amount, 200.0);
        // Average: (100 * 0.05 + 100 * 0.07) / 200 = 12 / 200 = 0.06
        assert!((position.entry_price.unwrap() - 0.06).abs() < 1e-9);
    }

    #[test]
    fn test_update_position_for_sell() {
        let mut position = create_test_position("mint1", 100.0, 0.05);

        update_position_for_sell(&mut position, -50.0, false);

        assert_eq!(position.amount, 50.0);
        assert_eq!(position.entry_price, Some(0.05)); // Entry price unchanged
    }

    #[test]
    fn test_update_position_for_sell_clear() {
        let mut position = create_test_position("mint1", 100.0, 0.05);

        update_position_for_sell(&mut position, -99.99, true);

        assert_eq!(position.amount, 0.0); // Cleared to 0, not -0.01
    }
}
