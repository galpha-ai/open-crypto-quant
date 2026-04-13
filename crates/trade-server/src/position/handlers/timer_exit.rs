use tracing::{debug, info};

use crate::domain::TimerEvent;
use crate::execution::{Order, OrderStatus, OrderType};
use crate::position::errors::PositionError;
use crate::position::exit_strategy::ExitStrategy;
use crate::position::state::PositionManagerState;

pub(crate) fn collect_positions_to_sell(
    state: &PositionManagerState,
    exit_strategy: &dyn ExitStrategy,
    event: &TimerEvent,
) -> Vec<(String, crate::position::Position)> {
    state
        .positions
        .iter()
        .filter(|(mint, position)| {
            // Skip if we already have a pending sell for this position
            if state.pending_sells.contains(*mint) {
                debug!("Skipping position {} - pending sell already exists", mint);
                return false;
            }

            // Handle exit based on exit_mode
            match position.exit_mode {
                crate::position::ExitMode::Automatic => {
                    let should_exit = exit_strategy.should_exit(position, event.timestamp);

                    if should_exit {
                        let exit_reason = exit_strategy.get_exit_reason(position, event.timestamp);
                        tracing::info!(
                            mint = mint,
                            exit_reason = ?exit_reason,
                            pnl_pct = ?position.pnl_pct,
                            position_age = ?(event.timestamp.signed_duration_since(position.entry_time)),
                            "Position should be exited based on exit strategy (Automatic mode)"
                        );
                    }

                    should_exit
                }
                crate::position::ExitMode::StrategyManaged => false,
            }
        })
        .map(|(mint, position)| (mint.clone(), position.clone()))
        .collect()
}

pub(crate) fn build_exit_orders(
    positions_to_sell: Vec<(String, crate::position::Position)>,
    exit_strategy: &dyn ExitStrategy,
    event: &TimerEvent,
) -> Result<Vec<Order>, PositionError> {
    let mut orders = Vec::new();

    for (mint, position) in positions_to_sell {
        let exit_reason = exit_strategy.get_exit_reason(&position, event.timestamp);
        info!(
            mint,
            ?exit_reason,
            exit_mode = ?position.exit_mode,
            "Exit triggered - creating sell order",
        );

        let order = Order {
            mint: mint.clone(),
            market: None,
            order_type: OrderType::MarketSell {
                token_amount: position.amount,
                clear_position: true,
            },
            price: position.current_price,
            status: OrderStatus::Pending,
            timestamp: event.timestamp,
            signal_slot: None,
            signal_id: None,
            dex_type: None,
            venue_order_id: None,
            exit_mode: None,
            context: None,
        };

        orders.push(order);
    }

    Ok(orders)
}
