use tracing::info;

use crate::position::balance_tracker::calculate_pnl_pct;
use crate::position::errors::PositionError;
use crate::position::position_query::ExchangePosition;
use crate::position::state::PositionManagerState;
use crate::position::{Position, PositionEvent, PositionUpdateSource};
use solana_sdk::signature::Signature;

pub(crate) fn reconcile_position(
    state: &mut PositionManagerState,
    exchange_pos: &ExchangePosition,
) -> Result<PositionEvent, PositionError> {
    let previous_amount = state
        .positions
        .get(&exchange_pos.asset_id)
        .map(|p| p.amount)
        .unwrap_or(0.0);

    let drift = exchange_pos.amount - previous_amount;

    let position_clone = if let Some(position) = state.positions.get_mut(&exchange_pos.asset_id) {
        position.amount = exchange_pos.amount;
        if let Some(price) = exchange_pos.entry_price {
            position.entry_price = Some(price);
        }
        position.current_price_updated_time = exchange_pos.timestamp;
        position.pnl_pct = calculate_pnl_pct(position);
        position.clone()
    } else {
        let new_position = Position {
            mint: exchange_pos.asset_id.clone(),
            amount: exchange_pos.amount,
            entry_price: exchange_pos.entry_price,
            current_price: exchange_pos.entry_price,
            current_price_updated_time: exchange_pos.timestamp,
            pnl_pct: None,
            entry_time: exchange_pos.timestamp,
            entry_slot: 0,
            entry_signature: Signature::default(),
            signal_id: None,
            sell_failure_count: 0,
            active_exit_order: None,
            exit_mode: crate::position::ExitMode::default(),
        };
        state
            .positions
            .insert(exchange_pos.asset_id.clone(), new_position.clone());
        new_position
    };

    info!(
        asset_id = %exchange_pos.asset_id,
        previous_amount = previous_amount,
        new_amount = exchange_pos.amount,
        drift = drift,
        "Position reconciled with exchange data"
    );

    let available_quote = state.available_quote;
    Ok(PositionEvent::PositionUpdated {
        position: position_clone,
        source: PositionUpdateSource::Reconciliation {
            previous_amount,
            drift,
        },
        available_quote,
    })
}
