use tracing::debug;

use crate::execution::LimitOrderEvent;
use crate::position::constants::ORDER_DUST_THRESHOLD;
use crate::position::errors::PositionError;
use crate::position::pending_order::PendingLimitOrder;
use crate::position::state::PositionManagerState;

pub(crate) fn handle_limit_order_event(
    state: &mut PositionManagerState,
    event: &LimitOrderEvent,
) -> Result<(), PositionError> {
    match event {
        LimitOrderEvent::OrderPlaced {
            order_id,
            mint,
            market,
            price,
            size,
            side,
            timestamp,
            signal_id,
            context: _,
        } => {
            let pending_order = PendingLimitOrder::new(
                order_id.clone(),
                mint.clone(),
                market.clone(),
                *side,
                *price,
                *size,
                *timestamp,
                signal_id.clone(),
            );

            if let Some(in_flight) =
                state.remove_in_flight_order_by_level(mint, *side, *price, *size)
            {
                debug!(
                    in_flight_id = %in_flight.id,
                    order_id = %order_id,
                    mint = %mint,
                    "Transitioned in-flight order to pending"
                );
            }

            let mint_orders = state.pending_limit_orders.entry(mint.clone()).or_default();
            mint_orders.insert(order_id.clone(), pending_order);

            debug!(
                order_id = %order_id,
                mint = %mint,
                side = ?side,
                price = price,
                size = size,
                "Added pending limit order"
            );
        }

        LimitOrderEvent::OrderPartiallyFilled {
            order_id,
            filled_size,
            remaining_size,
            ..
        } => {
            for (_mint, orders) in state.pending_limit_orders.iter_mut() {
                if orders.contains_key(order_id) {
                    if *remaining_size < ORDER_DUST_THRESHOLD {
                        orders.remove(order_id);
                        debug!(
                            order_id = %order_id,
                            filled_size = filled_size,
                            "Removed fully filled order from pending state"
                        );
                    } else if let Some(order) = orders.get_mut(order_id) {
                        order.remaining_size = *remaining_size;
                        debug!(
                            order_id = %order_id,
                            filled_size = filled_size,
                            remaining_size = remaining_size,
                            "Updated pending order after partial fill"
                        );
                    }
                    return Ok(());
                }
            }

            debug!(
                order_id = %order_id,
                "Received partial fill for unknown order"
            );
        }

        LimitOrderEvent::OrderCancelled {
            order_id,
            reason,
            timestamp: _,
        } => {
            if reason
                .as_deref()
                .is_some_and(|value| value.contains("awaiting confirmation"))
            {
                debug!(
                    order_id = %order_id,
                    reason = ?reason,
                    "Received cancel command acknowledgement; keeping order pending"
                );
                return Ok(());
            }
            let _ = state.remove_pending_order_by_id(order_id);
            debug!(
                order_id = %order_id,
                reason = ?reason,
                "Removed cancelled order from pending state"
            );
        }

        LimitOrderEvent::OrderExpired {
            order_id,
            timestamp: _,
        } => {
            let _ = state.remove_pending_order_by_id(order_id);
            debug!(
                order_id = %order_id,
                "Removed expired order from pending state"
            );
        }

        LimitOrderEvent::OrderRejected { reason } => {
            debug!(
                reason = %reason,
                "Order rejected (not in pending state)"
            );
        }
    }

    Ok(())
}
