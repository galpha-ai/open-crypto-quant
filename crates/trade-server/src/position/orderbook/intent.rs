use std::collections::HashMap;

use chrono::Duration;
use tracing::debug;

use crate::execution::{Order, OrderSide};
use crate::position::errors::PositionError;
use crate::position::pending_order::PendingLimitOrder;
use crate::position::reconciliation::ReconciliationEngine;
use crate::position::state::{IntentDebounceState, IntentLaneKey, PositionManagerState};
use crate::signal::{OrderIntent, QuoteLevel};

pub(crate) fn reconcile_intent(
    state: &mut PositionManagerState,
    reconciliation_engine: &ReconciliationEngine,
    min_quote_lifetime_ms: Option<u64>,
    intent: &OrderIntent,
) -> Result<Vec<Order>, PositionError> {
    let pending_orders = state
        .pending_limit_orders
        .get(&intent.mint)
        .cloned()
        .unwrap_or_default();
    let in_flight_orders = state
        .in_flight_orders
        .get(&intent.mint)
        .cloned()
        .unwrap_or_default();
    let available_quote = state.available_quote;

    let mut current_orders = pending_orders.clone();
    for (id, in_flight) in &in_flight_orders {
        let synthetic_pending = PendingLimitOrder::new(
            format!("in_flight:{}", id),
            in_flight.mint.clone(),
            in_flight.market.clone(),
            in_flight.side,
            in_flight.price,
            in_flight.size,
            in_flight.submitted_at,
            in_flight.signal_id.clone(),
        );
        current_orders.insert(format!("in_flight:{}", id), synthetic_pending);
    }

    let effective_intent = apply_intent_debounce(
        intent,
        &pending_orders,
        &current_orders,
        state,
        min_quote_lifetime_ms,
    );

    let mut orders =
        reconciliation_engine.compute_reconciliation(&effective_intent, &current_orders)?;

    orders.retain(|order| {
        if let Some(order_id) = order.order_type.cancel_order_id() {
            if order_id.starts_with("in_flight:") {
                debug!(
                    order_id = %order_id,
                    "Filtering out cancel for in-flight order (not yet confirmed)"
                );
                return false;
            }
        }
        true
    });

    let mut remaining_quote = available_quote;
    orders.retain(|order| {
        if order.order_type.is_cancel() {
            return true;
        }

        if let Some(quote_amount) = order.order_type.quote_amount() {
            if quote_amount > remaining_quote {
                debug!(
                    order_type = ?order.order_type,
                    quote_required = quote_amount,
                    available_quote = remaining_quote,
                    mint = %order.mint,
                    "Filtering out buy order - insufficient quote balance"
                );
                return false;
            }
            remaining_quote -= quote_amount;
        }

        true
    });

    Ok(orders)
}

fn apply_intent_debounce(
    intent: &OrderIntent,
    pending_orders: &HashMap<String, PendingLimitOrder>,
    current_orders: &HashMap<String, PendingLimitOrder>,
    state: &mut PositionManagerState,
    min_quote_lifetime_ms: Option<u64>,
) -> OrderIntent {
    let min_lifetime_ms = match min_quote_lifetime_ms {
        Some(ms) if ms > 0 => ms,
        _ => return intent.clone(),
    };

    let mut effective_intent = intent.clone();
    effective_intent.bids = debounce_side(
        intent,
        OrderSide::Buy,
        intent.bids.clone(),
        pending_orders,
        current_orders,
        state,
        min_lifetime_ms,
    );
    effective_intent.asks = debounce_side(
        intent,
        OrderSide::Sell,
        intent.asks.clone(),
        pending_orders,
        current_orders,
        state,
        min_lifetime_ms,
    );

    effective_intent
}

fn debounce_side(
    intent: &OrderIntent,
    side: OrderSide,
    desired: Option<Vec<QuoteLevel>>,
    pending_orders: &HashMap<String, PendingLimitOrder>,
    current_orders: &HashMap<String, PendingLimitOrder>,
    state: &mut PositionManagerState,
    min_lifetime_ms: u64,
) -> Option<Vec<QuoteLevel>> {
    let lane_key = IntentLaneKey {
        market: intent.market.clone(),
        mint: intent.mint.clone(),
        side,
    };
    let lane_state = state
        .intent_debounce
        .entry(lane_key)
        .or_insert_with(IntentDebounceState::default);

    let desired_is_cancel_all = matches!(desired.as_ref(), Some(levels) if levels.is_empty());
    if desired_is_cancel_all {
        if lane_state.pending_levels.is_some() {
            debug!(
                mint = %intent.mint,
                market = ?intent.market,
                side = ?side,
                signal_id = %intent.signal_id,
                "Bypassing quote-update debounce for cancel-all and clearing pending debounced levels"
            );
        }
        lane_state.pending_levels = None;
        return desired;
    }

    let pending_side: Vec<&PendingLimitOrder> = pending_orders
        .values()
        .filter(|order| order.side == side)
        .collect();
    let current_side: Vec<&PendingLimitOrder> = current_orders
        .values()
        .filter(|order| order.side == side)
        .collect();

    let debounce_until = pending_side
        .iter()
        .map(|order| order.placed_at)
        .max()
        .map(|live_since| live_since + Duration::milliseconds(min_lifetime_ms as i64));

    if desired.is_none() {
        if lane_state.pending_levels.is_some()
            && debounce_until
                .map(|deadline| intent.timestamp >= deadline)
                .unwrap_or(true)
        {
            let pending_level_count = lane_state
                .pending_levels
                .as_ref()
                .map(|levels| levels.len())
                .unwrap_or(0);
            debug!(
                mint = %intent.mint,
                market = ?intent.market,
                side = ?side,
                signal_id = %intent.signal_id,
                pending_level_count,
                debounce_until = ?debounce_until,
                "Flushing debounced quote update after debounce window"
            );
            return lane_state.pending_levels.take();
        }
        return None;
    }

    let levels = desired.expect("checked above");
    let has_change = side_has_diff(&current_side, &levels);
    let within_window = debounce_until
        .map(|deadline| intent.timestamp < deadline)
        .unwrap_or(false);

    if has_change && within_window {
        let desired_level_count = levels.len();
        debug!(
            mint = %intent.mint,
            market = ?intent.market,
            side = ?side,
            signal_id = %intent.signal_id,
            desired_level_count,
            debounce_until = ?debounce_until,
            "Debouncing quote update within minimum quote lifetime window"
        );
        lane_state.pending_levels = Some(levels);
        return None;
    }

    if has_change && lane_state.pending_levels.is_some() {
        debug!(
            mint = %intent.mint,
            market = ?intent.market,
            side = ?side,
            signal_id = %intent.signal_id,
            "Applying latest quote update after debounce window"
        );
    }
    lane_state.pending_levels = None;
    Some(levels)
}

fn side_has_diff(current: &[&PendingLimitOrder], desired: &[QuoteLevel]) -> bool {
    if current.len() != desired.len() {
        return true;
    }

    for order in current {
        if !desired
            .iter()
            .any(|level| order.matches_level(level.price, level.size))
        {
            return true;
        }
    }

    for level in desired {
        if !current
            .iter()
            .any(|order| order.matches_level(level.price, level.size))
        {
            return true;
        }
    }

    false
}
