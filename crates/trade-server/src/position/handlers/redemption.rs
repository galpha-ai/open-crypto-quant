use chrono::{DateTime, Utc};
use tracing::{debug, error};

use crate::execution::RedemptionEvent;
use crate::position::balance_tracker::calculate_pnl_pct;
use crate::position::constants::POSITION_DUST_THRESHOLD;
use crate::position::errors::PositionError;
use crate::position::events::ExitReason;
use crate::position::state::PositionManagerState;
use crate::position::{PositionEvent, PositionUpdateSource};
use crate::signal::{RedemptionAction, RedemptionPolicy};

pub(crate) fn update_redemption_policy(state: &mut PositionManagerState, policy: RedemptionPolicy) {
    debug!(
        market = %policy.market,
        up_asset = %policy.up_asset_id,
        down_asset = %policy.down_asset_id,
        max_unredeemed = policy.max_unredeemed_pair_value,
        "Updating redemption policy"
    );
    state.set_redemption_policy(policy);
}

pub(crate) fn reconcile_redemption_policies(state: &PositionManagerState) -> Vec<RedemptionAction> {
    let mut actions = vec![];

    for policy in state.all_redemption_policies() {
        let up_qty = state
            .positions
            .get(&policy.up_asset_id)
            .map(|p| p.amount)
            .unwrap_or(0.0);
        let down_qty = state
            .positions
            .get(&policy.down_asset_id)
            .map(|p| p.amount)
            .unwrap_or(0.0);

        let to_redeem = policy.pairs_to_redeem(up_qty, down_qty);
        if to_redeem > 0.0 {
            debug!(
                market = %policy.market,
                up_qty = up_qty,
                down_qty = down_qty,
                to_redeem = to_redeem,
                max_unredeemed = policy.max_unredeemed_pair_value,
                "Redemption policy violated, generating action"
            );
            actions.push(RedemptionAction::new(
                policy.market.clone(),
                policy.up_asset_id.clone(),
                policy.down_asset_id.clone(),
                to_redeem,
                Utc::now(),
            ));
        }
    }

    actions
}

pub(crate) fn handle_redemption(
    state: &mut PositionManagerState,
    event: &RedemptionEvent,
) -> Result<Vec<PositionEvent>, PositionError> {
    match event {
        RedemptionEvent::RedemptionCompleted {
            up_asset_id,
            down_asset_id,
            quantity,
            quote_received,
            timestamp,
            ..
        } => {
            let mut events = vec![];

            state.available_quote += quote_received;
            state.total_quote_received += quote_received;

            debug!(
                quote_received = quote_received,
                total_quote_received = state.total_quote_received,
                available_quote = state.available_quote,
                "Updated quote balance after redemption"
            );

            fn update_position_for_redemption(
                state: &mut PositionManagerState,
                asset_id: &str,
                quantity: f64,
                timestamp: DateTime<Utc>,
                events: &mut Vec<PositionEvent>,
            ) {
                if let Some(position) = state.positions.get_mut(asset_id) {
                    let previous_amount = position.amount;
                    let entry_time = position.entry_time;
                    let signal_id = position.signal_id.clone();

                    position.amount -= quantity;
                    position.pnl_pct = calculate_pnl_pct(position);

                    let should_close = position.amount.abs() < POSITION_DUST_THRESHOLD;
                    let updated_position = position.clone();
                    let available_quote = state.available_quote;

                    if should_close {
                        state.positions.remove(asset_id);
                        state.pending_sells.remove(asset_id);
                        state.total_closed_positions += 1;

                        events.push(PositionEvent::PositionClosed {
                            position: updated_position,
                            realized_pnl_sol: None,
                            pnl_pct: None,
                            holding_period: timestamp.signed_duration_since(entry_time),
                            signal_id,
                            exit_reason: ExitReason::Redemption,
                            available_quote,
                        });
                    } else {
                        events.push(PositionEvent::PositionUpdated {
                            position: updated_position,
                            source: PositionUpdateSource::Redemption {
                                previous_amount,
                                redeemed_quantity: quantity,
                            },
                            available_quote,
                        });
                    }
                } else {
                    debug!(
                        asset_id = %asset_id,
                        "Position not found during redemption"
                    );
                }
            }

            update_position_for_redemption(state, up_asset_id, *quantity, *timestamp, &mut events);

            update_position_for_redemption(
                state,
                down_asset_id,
                *quantity,
                *timestamp,
                &mut events,
            );

            debug!(
                up_asset = %up_asset_id,
                down_asset = %down_asset_id,
                quantity = quantity,
                quote_received = quote_received,
                "Processed redemption completed event"
            );

            Ok(events)
        }
        RedemptionEvent::RedemptionFailed { market, reason, .. } => {
            error!(
                market = %market,
                reason = %reason,
                "Redemption failed"
            );
            Ok(vec![])
        }
    }
}
