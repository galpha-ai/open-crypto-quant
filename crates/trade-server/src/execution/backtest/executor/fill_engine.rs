use std::collections::HashMap;

use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use tracing::debug;

use popeyes_trading_types::{PolymarketTradeEvent, TradeSide};

use crate::execution::{
    events::{LimitOrderEvent, OrderSide},
    lifecycle::{LifecycleState, TerminalReason},
};

use super::types::QuoteLaneState;
use super::{BacktestOrderExecutor, PendingBacktestOrder};

impl BacktestOrderExecutor {
    pub(super) async fn handle_polymarket_trade_with_fill_engine(
        &self,
        trade: &PolymarketTradeEvent,
        inventory: &HashMap<String, f64>,
        available_quote: f64,
    ) -> Result<Vec<LimitOrderEvent>> {
        // Track market->assets mapping as early as possible so complement fills become available
        // even if snapshots arrive after trades within the same tick.
        self.record_market_asset(&trade.market, &trade.asset_id)
            .await;

        let mirrored_asset_id = self.complement_asset_id(&trade.asset_id).await;
        let mut events = Vec::new();

        // Track inventory changes within this call to handle multiple fills correctly
        let mut inventory_changes: HashMap<String, f64> = HashMap::new();
        // Track quote changes within this call to handle multiple buy fills correctly
        let mut quote_changes: f64 = 0.0;

        // Collect order IDs to remove (fully filled orders)
        let mut orders_to_remove = Vec::new();

        // Get trade timestamp for eligibility checks
        let trade_timestamp =
            DateTime::from_timestamp_millis(trade.timestamp).unwrap_or_else(Utc::now);

        if self.latency_config.is_some() {
            events.extend(self.advance_time(trade_timestamp).await?);
        } else {
            self.set_current_time(trade_timestamp).await;
        }

        let mut orders = self.pending_orders.lock().await;

        // Ensure deterministic fill processing regardless of `HashMap` iteration order.
        // This matters because we update `inventory_changes` / `quote_changes` as we walk orders.
        let mut order_ids: Vec<String> = orders.keys().cloned().collect();
        order_ids.sort();

        // Check each pending order for potential fill
        for order_id in order_ids {
            let Some(pending) = orders.get_mut(&order_id) else {
                continue;
            };
            // Only process orders for the traded asset OR its complementary asset (CTF mirroring).
            let is_direct_asset = pending.mint == trade.asset_id;
            let is_complement_asset = mirrored_asset_id
                .as_ref()
                .is_some_and(|mirror| pending.mint == *mirror);

            if !is_direct_asset && !is_complement_asset {
                continue;
            }

            // Skip orders that weren't on the book yet when this trade happened (latency simulation)
            if trade_timestamp < pending.eligible_for_fills_at {
                debug!(
                    order_id = %order_id,
                    trade_ts = %trade_timestamp,
                    eligible_at = %pending.eligible_for_fills_at,
                    "Skipping fill check - order not yet eligible"
                );
                continue;
            }

            if let Some(active_until) = pending.active_until {
                if trade_timestamp >= active_until {
                    continue;
                }
            }

            // Trade-based fill logic (with Polymarket CTF mirroring):
            //
            // Standard orderbooks:
            // - Our BID (buy) fills when someone SELLs at or below our bid price
            // - Our ASK (sell) fills when someone BUYs at or above our ask price
            //
            // Polymarket CTF mirroring (binary markets):
            // - A trade on asset A at price p corresponds to a mirrored trade on asset B at price
            //   (1 - p) with opposite side (BUY <-> SELL).
            // - This matters for dual-orderbook strategies that may express an "ask" on A as a bid
            //   on B (or vice versa).
            let (effective_trade_side, effective_trade_price) = if is_direct_asset {
                (trade.side.clone(), trade.price)
            } else {
                (
                    Self::flip_trade_side(&trade.side),
                    (1.0 - trade.price).clamp(0.0, 1.0),
                )
            };

            let should_fill = match (&pending.side, &effective_trade_side) {
                // We're bidding (buying), trade is a sell hitting bids
                (OrderSide::Buy, TradeSide::Sell) => effective_trade_price <= pending.price,
                // We're asking (selling), trade is a buy lifting asks
                (OrderSide::Sell, TradeSide::Buy) => effective_trade_price >= pending.price,
                // Other combinations don't result in a fill
                _ => false,
            };

            if should_fill {
                // For sell orders, check inventory constraint if enabled
                let available_inventory =
                    if self.enforce_inventory_constraints && pending.side == OrderSide::Sell {
                        let base_inventory = inventory.get(&pending.mint).copied().unwrap_or(0.0);
                        let changes = inventory_changes.get(&pending.mint).copied().unwrap_or(0.0);
                        let available = base_inventory + changes;

                        if available <= 0.0 {
                            // No inventory to sell - skip this fill
                            debug!(
                                order_id = %order_id,
                                mint = %pending.mint,
                                available_inventory = available,
                                "Skipping sell fill - no inventory available"
                            );
                            continue;
                        }
                        Some(available)
                    } else {
                        None
                    };

                // Calculate fill size (capped by order remaining, trade size, and available inventory)
                let mut fill_size = pending.remaining_size.min(trade.size);

                // Cap by available inventory for sell orders if constraint is enabled
                if let Some(available) = available_inventory {
                    fill_size = fill_size.min(available);
                }

                // For buy orders, check quote balance constraint.
                // This is Layer 2 of the quote balance defense - it catches race conditions
                // where multiple markets independently generated buy signals.
                if pending.side == OrderSide::Buy {
                    let quote_required = fill_size * pending.price;
                    let effective_available = available_quote + quote_changes;

                    if effective_available < quote_required {
                        // Check if we can do a partial fill
                        let max_fill_by_quote = effective_available / pending.price;
                        if max_fill_by_quote <= 0.0 {
                            debug!(
                                order_id = %order_id,
                                mint = %pending.mint,
                                quote_required = quote_required,
                                available_quote = effective_available,
                                "Skipping buy fill - insufficient quote balance"
                            );
                            continue;
                        }
                        // Reduce fill size to what we can afford
                        fill_size = fill_size.min(max_fill_by_quote);
                        debug!(
                            order_id = %order_id,
                            mint = %pending.mint,
                            original_fill_size = pending.remaining_size.min(trade.size),
                            reduced_fill_size = fill_size,
                            available_quote = effective_available,
                            "Reducing buy fill size due to quote balance constraint"
                        );
                    }
                }

                // Skip if fill size is effectively zero
                if fill_size <= 0.0 {
                    continue;
                }

                let previous_remaining_size = pending.remaining_size;
                let (lifecycle_state, lifecycle_remaining_size) =
                    self.lifecycle_record_fill_evidence(&order_id, fill_size)?;
                let applied_fill_size =
                    (previous_remaining_size - lifecycle_remaining_size).max(0.0);
                if applied_fill_size <= 0.0 {
                    continue;
                }

                // If lifecycle is already terminal for a non-fill reason, suppress duplicate fill effects.
                if matches!(
                    lifecycle_state,
                    LifecycleState::Terminal(reason) if reason != TerminalReason::Filled
                ) {
                    pending.remaining_size = lifecycle_remaining_size;
                    continue;
                }
                fill_size = applied_fill_size;
                pending.remaining_size = lifecycle_remaining_size;

                // Track inventory and quote changes
                if pending.side == OrderSide::Sell {
                    *inventory_changes.entry(pending.mint.clone()).or_insert(0.0) -= fill_size;
                    // Selling increases quote balance
                    quote_changes += fill_size * pending.price;
                } else {
                    // Buy orders increase inventory and decrease quote balance
                    *inventory_changes.entry(pending.mint.clone()).or_insert(0.0) += fill_size;
                    quote_changes -= fill_size * pending.price;
                }

                if matches!(
                    lifecycle_state,
                    LifecycleState::Terminal(TerminalReason::Filled)
                ) {
                    // Fully filled - emit OrderFilled event (as OrderPartiallyFilled with remaining=0)
                    debug!(
                        order_id = %order_id,
                        fill_price = pending.price,
                        fill_size = fill_size,
                        "Limit order fully filled"
                    );
                    events.push(LimitOrderEvent::OrderPartiallyFilled {
                        order_id: order_id.clone(),
                        mint: pending.mint.clone(),
                        side: pending.side,
                        filled_size: fill_size,
                        remaining_size: 0.0,
                        fill_price: pending.price, // Fill at our order price, not trade price
                        timestamp: trade_timestamp,
                        signal_id: pending.signal_id.clone(),
                        exit_mode: pending.exit_mode,
                    });
                    orders_to_remove.push(order_id.clone());
                } else {
                    // Partially filled
                    debug!(
                        order_id = %order_id,
                        fill_price = pending.price,
                        fill_size = fill_size,
                        remaining_size = pending.remaining_size,
                        "Limit order partially filled"
                    );
                    events.push(LimitOrderEvent::OrderPartiallyFilled {
                        order_id: order_id.clone(),
                        mint: pending.mint.clone(),
                        side: pending.side,
                        filled_size: fill_size,
                        remaining_size: pending.remaining_size,
                        fill_price: pending.price, // Fill at our order price, not trade price
                        timestamp: trade_timestamp,
                        signal_id: pending.signal_id.clone(),
                        exit_mode: pending.exit_mode,
                    });
                }
            }
        }

        // Remove fully filled orders
        for order_id in &orders_to_remove {
            orders.remove(order_id);
        }

        drop(orders);

        if !orders_to_remove.is_empty() {
            let mut lanes = self.quote_lanes.lock().await;
            let mut pending_orders = self.pending_orders.lock().await;
            let mut order_id_to_lane = self.order_id_to_lane.lock().await;

            for order_id in orders_to_remove {
                if let Some(key) = order_id_to_lane.remove(&order_id) {
                    if let Some(state) = lanes.get_mut(&key) {
                        match state {
                            QuoteLaneState::Live { .. } => {
                                *state = QuoteLaneState::Idle;
                            }
                            QuoteLaneState::Canceling {
                                pending_replacement,
                                ..
                            } => {
                                if let Some(replacement) = pending_replacement.take() {
                                    let live_at = trade_timestamp
                                        + Duration::milliseconds(
                                            self.sample_place_latency_ms() as i64
                                        );
                                    if trade_timestamp >= live_at {
                                        let replacement_state = self
                                            .lifecycle_record_place_success(
                                                &replacement.order_id,
                                            )?;
                                        match replacement_state {
                                            LifecycleState::Open => {
                                                let pending = PendingBacktestOrder {
                                                    order_id: replacement.order_id.clone(),
                                                    mint: replacement.quote.mint.clone(),
                                                    market: replacement.quote.market.clone(),
                                                    side: replacement.quote.side,
                                                    price: replacement.quote.price,
                                                    original_size: replacement.quote.size,
                                                    remaining_size: replacement.quote.size,
                                                    placed_at: trade_timestamp,
                                                    time_in_force: replacement.quote.time_in_force,
                                                    signal_id: replacement.quote.signal_id.clone(),
                                                    exit_mode: replacement.quote.exit_mode,
                                                    context: replacement.quote.context.clone(),
                                                    eligible_for_fills_at: live_at,
                                                    active_until: None,
                                                };
                                                pending_orders
                                                    .insert(replacement.order_id.clone(), pending);
                                                order_id_to_lane.insert(
                                                    replacement.order_id.clone(),
                                                    key.clone(),
                                                );
                                                events.push(LimitOrderEvent::OrderPlaced {
                                                    order_id: replacement.order_id.clone(),
                                                    mint: replacement.quote.mint.clone(),
                                                    market: replacement.quote.market.clone(),
                                                    price: replacement.quote.price,
                                                    size: replacement.quote.size,
                                                    side: replacement.quote.side,
                                                    timestamp: trade_timestamp,
                                                    signal_id: replacement.quote.signal_id.clone(),
                                                    context: replacement.quote.context.clone(),
                                                });
                                                *state = QuoteLaneState::Live {
                                                    order_id: replacement.order_id.clone(),
                                                };
                                            }
                                            LifecycleState::CancelPending => {
                                                let remove_at = trade_timestamp
                                                    + Duration::milliseconds(
                                                        self.sample_cancel_latency_ms() as i64,
                                                    );
                                                if trade_timestamp >= remove_at {
                                                    if self.lifecycle_record_cancel_confirmed(
                                                        &replacement.order_id,
                                                    )? {
                                                        events.push(
                                                            LimitOrderEvent::OrderCancelled {
                                                                order_id: replacement
                                                                    .order_id
                                                                    .clone(),
                                                                reason: Some(
                                                                    "User cancelled".to_string(),
                                                                ),
                                                                timestamp: trade_timestamp,
                                                            },
                                                        );
                                                    }
                                                    *state = QuoteLaneState::Idle;
                                                } else {
                                                    let pending = PendingBacktestOrder {
                                                        order_id: replacement.order_id.clone(),
                                                        mint: replacement.quote.mint.clone(),
                                                        market: replacement.quote.market.clone(),
                                                        side: replacement.quote.side,
                                                        price: replacement.quote.price,
                                                        original_size: replacement.quote.size,
                                                        remaining_size: replacement.quote.size,
                                                        placed_at: trade_timestamp,
                                                        time_in_force: replacement
                                                            .quote
                                                            .time_in_force,
                                                        signal_id: replacement
                                                            .quote
                                                            .signal_id
                                                            .clone(),
                                                        exit_mode: replacement.quote.exit_mode,
                                                        context: replacement.quote.context.clone(),
                                                        eligible_for_fills_at: live_at,
                                                        active_until: Some(remove_at),
                                                    };
                                                    pending_orders.insert(
                                                        replacement.order_id.clone(),
                                                        pending,
                                                    );
                                                    order_id_to_lane.insert(
                                                        replacement.order_id.clone(),
                                                        key.clone(),
                                                    );
                                                    *state = QuoteLaneState::Canceling {
                                                        order_id: replacement.order_id.clone(),
                                                        remove_at,
                                                        pending_replacement: None,
                                                    };
                                                }
                                            }
                                            LifecycleState::Terminal(_) => {
                                                *state = QuoteLaneState::Idle;
                                            }
                                            LifecycleState::SubmitPending => {
                                                return Err(anyhow!(
                                                    "replacement order {} stayed submit-pending after place success in fill path",
                                                    replacement.order_id
                                                ));
                                            }
                                        }
                                    } else {
                                        *state = QuoteLaneState::Placing {
                                            scheduled: replacement,
                                            live_at,
                                            cancel_at: None,
                                        };
                                    }
                                } else {
                                    *state = QuoteLaneState::Idle;
                                }
                            }
                            QuoteLaneState::Placing { .. } | QuoteLaneState::Idle => {}
                        }
                    }
                }
            }
        }

        Ok(events)
    }
}
