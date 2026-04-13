use chrono::DateTime;
use solana_sdk::signature::Signature;
use tracing::{debug, info};

use crate::execution::ExecutionEvent;
use crate::position::ExitMode;
use crate::position::balance_tracker;
use crate::position::constants::POSITION_DUST_THRESHOLD;
use crate::position::errors::PositionError;
use crate::position::events::{ExitReason, OrderSide as PositionOrderSide};
use crate::position::execution_handler;
use crate::position::exit_strategy::ExitStrategy;
use crate::position::metrics::PositionManagerMetrics;
use crate::position::state::PositionManagerState;
use crate::position::{PositionEvent, PositionUpdateSource};

pub(crate) fn handle_execution(
    state: &mut PositionManagerState,
    event: &ExecutionEvent,
    exit_strategy: &dyn ExitStrategy,
    metrics: &PositionManagerMetrics,
) -> Result<PositionEvent, PositionError> {
    debug!(?event, "Handling execution event");

    match event {
        ExecutionEvent::OrderFilled {
            mint,
            token_amount_change,
            quote_amount_change,
            price,
            timestamp,
            slippage: _,
            clear_position,
            force_position_clear,
            execution_latency_in_slots: _,
            confirmed_slot,
            confirmed_signature,
            signal_id,
            exit_mode,
        } => {
            if *force_position_clear {
                tracing::info!(
                    mint = mint,
                    "Force position clear flag set, clearing position without metrics calculation"
                );

                if let Some(position) = state.positions.remove(mint) {
                    state.pending_sells.remove(mint);

                    return Ok(PositionEvent::PositionClosed {
                        position: position.clone(),
                        realized_pnl_sol: None,
                        pnl_pct: None,
                        holding_period: timestamp.signed_duration_since(position.entry_time),
                        signal_id: position.signal_id.clone(),
                        exit_reason: ExitReason::Other("force_clear".to_string()),
                        available_quote: state.available_quote,
                    });
                }

                debug!(mint = mint, "Position already cleared or doesn't exist");
                return Err(PositionError::PositionNotFound(mint.to_string()));
            }

            handle_order_filled(
                state,
                mint,
                *token_amount_change,
                quote_amount_change,
                *price,
                *timestamp,
                *clear_position,
                signal_id.clone(),
                *confirmed_slot,
                *confirmed_signature,
                *exit_mode,
                exit_strategy,
            )
        }
        ExecutionEvent::OrderRejected { mint, reason } => {
            info!("Order rejected for mint={}, reason={}", mint, reason);
            metrics
                .order_rejected_handled
                .with_label_values(&["reason"])
                .inc();
            state.pending_sells.remove(mint);
            Err(PositionError::OrderRejected(format!(
                "Order rejected for {}: {}",
                mint, reason
            )))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_order_filled(
    state: &mut PositionManagerState,
    mint: &str,
    token_amount_change: f64,
    quote_amount_change: &Option<f64>,
    price: Option<f64>,
    timestamp: DateTime<chrono::Utc>,
    clear_position: bool,
    signal_id: Option<String>,
    confirmed_slot: Option<u64>,
    confirmed_signature: Option<Signature>,
    exit_mode: Option<ExitMode>,
    exit_strategy: &dyn ExitStrategy,
) -> Result<PositionEvent, PositionError> {
    debug!(
        mint = mint,
        token_amount_change = token_amount_change,
        quote_amount_change = ?quote_amount_change,
        price = price,
        timestamp = %timestamp,
        signal_id = ?signal_id,
        exit_mode = ?exit_mode,
        "Updating position based on OrderFilled event"
    );
    debug!(
        "Pre-execution state - available_quote={}, positions={:?}",
        state.available_quote, state.positions
    );

    // Update quote balance and cash flow tracking
    balance_tracker::update_quote_balance(state, token_amount_change, quote_amount_change, price);

    if token_amount_change > 0.0 {
        state.bought_mints.insert(mint.to_string());
    }

    let (mut position, was_new_position) = execution_handler::get_or_create_position(
        state,
        mint,
        price,
        timestamp,
        signal_id,
        confirmed_slot,
        confirmed_signature,
        exit_mode,
    );

    if token_amount_change > 0.0 {
        execution_handler::update_position_for_buy(&mut position, token_amount_change, price);
    } else {
        execution_handler::update_position_for_sell(
            &mut position,
            token_amount_change,
            clear_position,
        );
    }

    position.current_price = price;
    position.pnl_pct = crate::position::balance_tracker::calculate_pnl_pct(&position);

    let should_remove = position.amount.abs() < POSITION_DUST_THRESHOLD;
    let available_quote = state.available_quote;
    let result = if should_remove {
        balance_tracker::update_trade_statistics(state, &position, timestamp);

        state.pending_sells.remove(mint);

        let holding_period = timestamp.signed_duration_since(position.entry_time);
        let exit_reason = exit_strategy.get_exit_reason(&position, timestamp);

        let realized_pnl = match (position.current_price, position.entry_price) {
            (Some(current), Some(entry)) => Some((current - entry) * token_amount_change.abs()),
            _ => quote_amount_change.map(|q| q.abs()),
        };

        PositionEvent::PositionClosed {
            position: position.clone(),
            realized_pnl_sol: realized_pnl,
            pnl_pct: position.pnl_pct,
            holding_period,
            signal_id: position.signal_id.clone(),
            exit_reason,
            available_quote: state.available_quote,
        }
    } else if was_new_position {
        PositionEvent::PositionCreated {
            position: position.clone(),
            available_quote,
        }
    } else {
        let side = if token_amount_change > 0.0 {
            PositionOrderSide::Buy
        } else {
            PositionOrderSide::Sell
        };

        PositionEvent::PositionUpdated {
            position: position.clone(),
            source: PositionUpdateSource::Fill {
                fill_price: price.unwrap_or(0.0),
                fill_size: token_amount_change.abs(),
                side,
                order_id: None,
            },
            available_quote,
        }
    };

    if should_remove {
        state.positions.remove(mint);
    } else {
        state.positions.insert(mint.to_string(), position);
    }

    debug!(?result, "Position updated");
    debug!(
        "Post-execution state - available_quote={}, positions={:?}",
        state.available_quote, state.positions
    );

    Ok(result)
}
