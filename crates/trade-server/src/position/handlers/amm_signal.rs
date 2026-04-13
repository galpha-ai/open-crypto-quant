use tracing::info;

use crate::execution::{Order, OrderStatus, OrderType};
use crate::position::errors::PositionError;
use crate::position::state::PositionManagerState;
use crate::signal::TradableSignal;

pub(crate) fn handle_signal(
    state: &PositionManagerState,
    trade_amount_sol: f64,
    max_open_positions: u32,
    signal: &dyn TradableSignal,
) -> Result<Option<Order>, PositionError> {
    // Skip signals that aren't tradable (have empty mint)
    let mint = match signal.get_mint() {
        Some(mint) if !mint.is_empty() => mint,
        _ => {
            tracing::info!("Skipping signal without mint");
            return Ok(None);
        }
    };

    // Check max open positions limit
    let current_open_positions = state.positions.len() as u32;
    if current_open_positions >= max_open_positions {
        tracing::info!(
            mint = mint,
            current_open_positions,
            max_open_positions = max_open_positions,
            "Skipping signal: Maximum open positions limit reached"
        );
        return Err(PositionError::MaxOpenPositionsReached(max_open_positions));
    }

    // Check if we have enough quote balance for the trade
    let available_quote = state.available_quote;
    if available_quote < trade_amount_sol {
        tracing::info!(
            mint = mint,
            available_quote,
            trade_amount = trade_amount_sol,
            "Insufficient quote balance",
        );
        return Err(PositionError::InsufficientSolBalance {
            available: available_quote,
            required: trade_amount_sol,
        });
    }

    // Calculate token amount based on fixed SOL trade size
    let price = match signal.get_price() {
        Some(price) => price,
        None => {
            tracing::info!(mint = mint, "Skipping signal without price");
            return Ok(None);
        }
    };

    let timestamp = match signal.get_timestamp() {
        Some(ts) => ts,
        None => {
            tracing::info!(mint = mint, "Skipping signal without timestamp");
            return Ok(None);
        }
    };

    let token_amount = trade_amount_sol / price;
    info!(
        mint = mint,
        token_amount,
        price,
        sol_amount = trade_amount_sol,
        "Creating buy order",
    );

    // Create buy order
    #[allow(deprecated)]
    let order = Order {
        mint: mint.to_string(),
        market: None,
        order_type: OrderType::Buy {
            sol_amount: trade_amount_sol,
        },
        price: Some(price),
        status: OrderStatus::Pending,
        timestamp,
        signal_slot: signal.get_slot(),
        signal_id: Some(signal.signal_id().to_string()),
        dex_type: None,
        venue_order_id: None,
        exit_mode: None,
        context: signal.get_context(),
    };

    Ok(Some(order))
}
