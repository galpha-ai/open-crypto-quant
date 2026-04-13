use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use tracing::debug;

use crate::execution::{
    events::{LimitOrderEvent, OrderSide, TimeInForce},
    lifecycle::{
        LifecycleCancelConfirmed, LifecycleCancelRequest, LifecycleFillEvidence, LifecycleOrderRef,
        LifecyclePlaceRequest, LifecyclePlaceSuccess, LifecycleState, TerminalReason,
    },
    order::{Order, OrderType},
};
use crate::position::ExitMode;

use super::types::{QuoteLaneKey, QuoteLaneState, QuoteRequest, ScheduledQuote};
use super::{BacktestOrderExecutor, PendingBacktestOrder};

impl BacktestOrderExecutor {
    const PRICE_MATCH_ABS_EPSILON: f64 = 1e-9;
    const PRICE_MATCH_REL_EPSILON: f64 = 1e-9;
    const SIZE_MATCH_ABS_EPSILON: f64 = 1e-6;
    const SIZE_MATCH_REL_EPSILON: f64 = 1e-9;

    fn quote_lane_key(mint: String, market: Option<String>, side: OrderSide) -> QuoteLaneKey {
        QuoteLaneKey { market, mint, side }
    }

    fn quote_from_order(
        mint: String,
        market: Option<String>,
        side: OrderSide,
        price: f64,
        size: f64,
        time_in_force: TimeInForce,
        signal_id: Option<String>,
        exit_mode: Option<ExitMode>,
        context: Option<serde_json::Value>,
    ) -> QuoteRequest {
        QuoteRequest {
            mint,
            market,
            side,
            price,
            size,
            time_in_force,
            signal_id,
            exit_mode,
            context,
        }
    }

    fn lifecycle_order_ref(order_id: &str) -> LifecycleOrderRef {
        LifecycleOrderRef::ClientOrderId(order_id.to_string())
    }

    fn pending_order_from_scheduled(
        scheduled: &ScheduledQuote,
        placed_at: DateTime<Utc>,
        eligible_for_fills_at: DateTime<Utc>,
        active_until: Option<DateTime<Utc>>,
    ) -> PendingBacktestOrder {
        PendingBacktestOrder {
            order_id: scheduled.order_id.clone(),
            mint: scheduled.quote.mint.clone(),
            market: scheduled.quote.market.clone(),
            side: scheduled.quote.side,
            price: scheduled.quote.price,
            original_size: scheduled.quote.size,
            remaining_size: scheduled.quote.size,
            placed_at,
            time_in_force: scheduled.quote.time_in_force,
            signal_id: scheduled.quote.signal_id.clone(),
            exit_mode: scheduled.quote.exit_mode,
            context: scheduled.quote.context.clone(),
            eligible_for_fills_at,
            active_until,
        }
    }

    fn lifecycle_record_place_request(&self, order_id: &str, quote: &QuoteRequest) -> Result<()> {
        let mut lifecycle = self
            .lifecycle_engine
            .lock()
            .expect("backtest lifecycle mutex poisoned");
        lifecycle
            .record_place_request(LifecyclePlaceRequest {
                lifecycle_id: Some(format!("backtest-{}", order_id)),
                client_order_id: order_id.to_string(),
                mint: quote.mint.clone(),
                market: quote.market.clone(),
                side: quote.side,
                price: quote.price,
                size: quote.size,
                time_in_force: quote.time_in_force,
                signal_id: quote.signal_id.clone(),
            })
            .map_err(|err| anyhow!("failed to record lifecycle place request: {err}"))?;
        Ok(())
    }

    pub(super) fn lifecycle_record_place_success(&self, order_id: &str) -> Result<LifecycleState> {
        let mut lifecycle = self
            .lifecycle_engine
            .lock()
            .expect("backtest lifecycle mutex poisoned");
        let order = lifecycle
            .record_place_success(LifecyclePlaceSuccess {
                order_ref: Self::lifecycle_order_ref(order_id),
                venue_order_id: order_id.to_string(),
            })
            .map_err(|err| anyhow!("failed to record lifecycle place success: {err}"))?;
        Ok(order.state)
    }

    fn lifecycle_record_cancel_request(&self, order_id: &str) -> Result<()> {
        let mut lifecycle = self
            .lifecycle_engine
            .lock()
            .expect("backtest lifecycle mutex poisoned");
        lifecycle
            .record_cancel_request(LifecycleCancelRequest {
                order_ref: Self::lifecycle_order_ref(order_id),
            })
            .map_err(|err| anyhow!("failed to record lifecycle cancel request: {err}"))?;
        Ok(())
    }

    pub(super) fn lifecycle_record_cancel_confirmed(&self, order_id: &str) -> Result<bool> {
        let mut lifecycle = self
            .lifecycle_engine
            .lock()
            .expect("backtest lifecycle mutex poisoned");
        let order_ref = Self::lifecycle_order_ref(order_id);
        let previous_state = lifecycle.get(&order_ref).map(|order| order.state);
        let updated = lifecycle
            .record_cancel_confirmed(LifecycleCancelConfirmed {
                order_ref,
                venue_order_id: Some(order_id.to_string()),
            })
            .map_err(|err| anyhow!("failed to record lifecycle cancel confirmation: {err}"))?;
        Ok(!matches!(
            previous_state,
            Some(LifecycleState::Terminal(TerminalReason::Cancelled))
        ) && matches!(
            updated.state,
            LifecycleState::Terminal(TerminalReason::Cancelled)
        ))
    }

    pub(super) fn lifecycle_record_fill_evidence(
        &self,
        order_id: &str,
        filled_size: f64,
    ) -> Result<(LifecycleState, f64)> {
        let mut lifecycle = self
            .lifecycle_engine
            .lock()
            .expect("backtest lifecycle mutex poisoned");
        let order = lifecycle
            .record_fill_evidence(LifecycleFillEvidence {
                order_ref: Self::lifecycle_order_ref(order_id),
                filled_size,
            })
            .map_err(|err| anyhow!("failed to record lifecycle fill evidence: {err}"))?;
        Ok((order.state, order.remaining_size))
    }

    fn nearly_equal(lhs: f64, rhs: f64, abs_epsilon: f64, rel_epsilon: f64) -> bool {
        let diff = (lhs - rhs).abs();
        let scale = lhs.abs().max(rhs.abs());
        diff <= abs_epsilon.max(rel_epsilon * scale)
    }

    fn quote_matches_live_order(quote: &QuoteRequest, live_order: &PendingBacktestOrder) -> bool {
        quote.mint == live_order.mint
            && quote.market == live_order.market
            && quote.side == live_order.side
            && quote.time_in_force == live_order.time_in_force
            && Self::nearly_equal(
                quote.price,
                live_order.price,
                Self::PRICE_MATCH_ABS_EPSILON,
                Self::PRICE_MATCH_REL_EPSILON,
            )
            && Self::nearly_equal(
                quote.size,
                live_order.remaining_size,
                Self::SIZE_MATCH_ABS_EPSILON,
                Self::SIZE_MATCH_REL_EPSILON,
            )
    }

    pub(super) async fn advance_time(&self, now: DateTime<Utc>) -> Result<Vec<LimitOrderEvent>> {
        self.set_current_time(now).await;

        let mut events = Vec::new();
        let mut lanes = self.quote_lanes.lock().await;
        let mut pending_orders = self.pending_orders.lock().await;
        let mut order_id_to_lane = self.order_id_to_lane.lock().await;

        let lane_keys: Vec<QuoteLaneKey> = lanes.keys().cloned().collect();

        for key in lane_keys {
            let state = match lanes.get_mut(&key) {
                Some(state) => state,
                None => continue,
            };

            match state {
                QuoteLaneState::Placing {
                    scheduled,
                    live_at,
                    cancel_at,
                } => {
                    if now < *live_at {
                        continue;
                    }

                    let lifecycle_state =
                        self.lifecycle_record_place_success(&scheduled.order_id)?;

                    match lifecycle_state {
                        LifecycleState::Open => {
                            let pending =
                                Self::pending_order_from_scheduled(scheduled, now, *live_at, None);
                            pending_orders.insert(scheduled.order_id.clone(), pending);
                            order_id_to_lane.insert(scheduled.order_id.clone(), key.clone());

                            events.push(LimitOrderEvent::OrderPlaced {
                                order_id: scheduled.order_id.clone(),
                                mint: scheduled.quote.mint.clone(),
                                market: scheduled.quote.market.clone(),
                                price: scheduled.quote.price,
                                size: scheduled.quote.size,
                                side: scheduled.quote.side,
                                timestamp: now,
                                signal_id: scheduled.quote.signal_id.clone(),
                                context: scheduled.quote.context.clone(),
                            });

                            if let Some(cancel_deadline) = *cancel_at {
                                if now >= cancel_deadline {
                                    pending_orders.remove(&scheduled.order_id);
                                    order_id_to_lane.remove(&scheduled.order_id);
                                    if self
                                        .lifecycle_record_cancel_confirmed(&scheduled.order_id)?
                                    {
                                        events.push(LimitOrderEvent::OrderCancelled {
                                            order_id: scheduled.order_id.clone(),
                                            reason: Some("User cancelled".to_string()),
                                            timestamp: now,
                                        });
                                    }
                                    *state = QuoteLaneState::Idle;
                                } else {
                                    if let Some(pending) =
                                        pending_orders.get_mut(&scheduled.order_id)
                                    {
                                        pending.active_until = Some(cancel_deadline);
                                    }
                                    *state = QuoteLaneState::Canceling {
                                        order_id: scheduled.order_id.clone(),
                                        remove_at: cancel_deadline,
                                        pending_replacement: None,
                                    };
                                }
                            } else {
                                *state = QuoteLaneState::Live {
                                    order_id: scheduled.order_id.clone(),
                                };
                            }
                        }
                        LifecycleState::CancelPending => {
                            let remove_at = cancel_at.unwrap_or_else(|| {
                                now + Duration::milliseconds(self.sample_cancel_latency_ms() as i64)
                            });
                            if now >= remove_at {
                                order_id_to_lane.remove(&scheduled.order_id);
                                if self.lifecycle_record_cancel_confirmed(&scheduled.order_id)? {
                                    events.push(LimitOrderEvent::OrderCancelled {
                                        order_id: scheduled.order_id.clone(),
                                        reason: Some("User cancelled".to_string()),
                                        timestamp: now,
                                    });
                                }
                                *state = QuoteLaneState::Idle;
                            } else {
                                let pending = Self::pending_order_from_scheduled(
                                    scheduled,
                                    now,
                                    *live_at,
                                    Some(remove_at),
                                );
                                pending_orders.insert(scheduled.order_id.clone(), pending);
                                order_id_to_lane.insert(scheduled.order_id.clone(), key.clone());
                                *state = QuoteLaneState::Canceling {
                                    order_id: scheduled.order_id.clone(),
                                    remove_at,
                                    pending_replacement: None,
                                };
                            }
                        }
                        LifecycleState::Terminal(_) => {
                            order_id_to_lane.remove(&scheduled.order_id);
                            *state = QuoteLaneState::Idle;
                        }
                        LifecycleState::SubmitPending => {
                            return Err(anyhow!(
                                "lifecycle order {} stayed submit-pending after place success",
                                scheduled.order_id
                            ));
                        }
                    }
                }
                QuoteLaneState::Canceling {
                    order_id,
                    remove_at,
                    pending_replacement,
                } => {
                    if now >= *remove_at {
                        pending_orders.remove(order_id);
                        order_id_to_lane.remove(order_id);

                        if self.lifecycle_record_cancel_confirmed(order_id)? {
                            events.push(LimitOrderEvent::OrderCancelled {
                                order_id: order_id.clone(),
                                reason: Some("User cancelled".to_string()),
                                timestamp: now,
                            });
                        }

                        if let Some(replacement) = pending_replacement.take() {
                            let live_at =
                                now + Duration::milliseconds(self.sample_place_latency_ms() as i64);
                            if now >= live_at {
                                let replacement_state =
                                    self.lifecycle_record_place_success(&replacement.order_id)?;
                                match replacement_state {
                                    LifecycleState::Open => {
                                        let pending = Self::pending_order_from_scheduled(
                                            &replacement,
                                            now,
                                            live_at,
                                            None,
                                        );
                                        pending_orders
                                            .insert(replacement.order_id.clone(), pending);
                                        order_id_to_lane
                                            .insert(replacement.order_id.clone(), key.clone());
                                        events.push(LimitOrderEvent::OrderPlaced {
                                            order_id: replacement.order_id.clone(),
                                            mint: replacement.quote.mint.clone(),
                                            market: replacement.quote.market.clone(),
                                            price: replacement.quote.price,
                                            size: replacement.quote.size,
                                            side: replacement.quote.side,
                                            timestamp: now,
                                            signal_id: replacement.quote.signal_id.clone(),
                                            context: replacement.quote.context.clone(),
                                        });
                                        *state = QuoteLaneState::Live {
                                            order_id: replacement.order_id.clone(),
                                        };
                                    }
                                    LifecycleState::CancelPending => {
                                        let replacement_remove_at = now
                                            + Duration::milliseconds(
                                                self.sample_cancel_latency_ms() as i64,
                                            );
                                        if now >= replacement_remove_at {
                                            if self.lifecycle_record_cancel_confirmed(
                                                &replacement.order_id,
                                            )? {
                                                events.push(LimitOrderEvent::OrderCancelled {
                                                    order_id: replacement.order_id.clone(),
                                                    reason: Some("User cancelled".to_string()),
                                                    timestamp: now,
                                                });
                                            }
                                            *state = QuoteLaneState::Idle;
                                        } else {
                                            let pending = Self::pending_order_from_scheduled(
                                                &replacement,
                                                now,
                                                live_at,
                                                Some(replacement_remove_at),
                                            );
                                            pending_orders
                                                .insert(replacement.order_id.clone(), pending);
                                            order_id_to_lane
                                                .insert(replacement.order_id.clone(), key.clone());
                                            *state = QuoteLaneState::Canceling {
                                                order_id: replacement.order_id.clone(),
                                                remove_at: replacement_remove_at,
                                                pending_replacement: None,
                                            };
                                        }
                                    }
                                    LifecycleState::Terminal(_) => {
                                        *state = QuoteLaneState::Idle;
                                    }
                                    LifecycleState::SubmitPending => {
                                        return Err(anyhow!(
                                            "replacement order {} stayed submit-pending after place success",
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
                    } else if let Some(pending) = pending_orders.get_mut(order_id) {
                        pending.active_until = Some(*remove_at);
                    }
                }
                QuoteLaneState::Live { .. } | QuoteLaneState::Idle => {}
            }
        }

        Ok(events)
    }

    /// Execute a limit order by adding it to the pending orders map.
    ///
    /// For ImmediateOrCancel (IOC) orders, this implementation does not attempt
    /// immediate fill checking - it simply places the order and relies on
    /// subsequent trade events to trigger fills.
    pub(super) async fn execute_limit_order_with_lifecycle(
        &self,
        order: Order,
    ) -> Result<LimitOrderEvent> {
        let (size, limit_price, side, time_in_force) = match &order.order_type {
            OrderType::LimitBuy {
                quote_amount,
                limit_price,
                time_in_force,
            } => {
                // Convert quote_amount to base units (token amount) for consistent tracking.
                // Intent signals express size in base units, so reconciliation comparisons
                // need the same unit to avoid spurious cancel/replace cycles.
                let base_size = *quote_amount / *limit_price;
                (base_size, *limit_price, OrderSide::Buy, *time_in_force)
            }
            OrderType::LimitSell {
                token_amount,
                limit_price,
                time_in_force,
                ..
            } => (*token_amount, *limit_price, OrderSide::Sell, *time_in_force),
            _ => {
                return Err(anyhow!(
                    "execute_limit_order called with non-limit order type: {:?}",
                    order.order_type
                ));
            }
        };

        self.set_current_time(order.timestamp).await;

        let quote = Self::quote_from_order(
            order.mint.clone(),
            order.market.clone(),
            side,
            limit_price,
            size,
            time_in_force,
            order.signal_id.clone(),
            order.exit_mode,
            order.context.clone(),
        );

        let mut order_id = self.generate_order_id();

        if self.latency_config.is_some() {
            let live_at = self.compute_eligibility_time(order.timestamp);
            let lane_key = Self::quote_lane_key(order.mint.clone(), order.market.clone(), side);

            let mut lanes = self.quote_lanes.lock().await;
            let mut pending_orders = self.pending_orders.lock().await;
            let mut order_id_to_lane = self.order_id_to_lane.lock().await;

            let state = lanes
                .entry(lane_key.clone())
                .or_insert(QuoteLaneState::Idle);

            match state {
                QuoteLaneState::Idle => {
                    self.lifecycle_record_place_request(&order_id, &quote)?;
                    *state = QuoteLaneState::Placing {
                        scheduled: ScheduledQuote {
                            order_id: order_id.clone(),
                            quote,
                        },
                        live_at,
                        cancel_at: None,
                    };
                    order_id_to_lane.insert(order_id.clone(), lane_key);
                }
                QuoteLaneState::Placing {
                    scheduled,
                    live_at: _,
                    cancel_at: _,
                } => {
                    scheduled.quote = quote;
                    order_id = scheduled.order_id.clone();
                }
                QuoteLaneState::Live {
                    order_id: live_order_id,
                    ..
                } => {
                    let is_noop_replacement = pending_orders
                        .get(live_order_id)
                        .is_some_and(|pending| Self::quote_matches_live_order(&quote, pending));

                    if is_noop_replacement {
                        // Reuse the existing live quote/order when the replacement is equivalent.
                        order_id = live_order_id.clone();
                    } else {
                        self.lifecycle_record_place_request(&order_id, &quote)?;
                        let remove_at = order.timestamp
                            + Duration::milliseconds(self.sample_cancel_latency_ms() as i64);
                        if let Some(pending) = pending_orders.get_mut(live_order_id) {
                            pending.active_until = Some(remove_at);
                        }
                        let replacement = ScheduledQuote {
                            order_id: order_id.clone(),
                            quote,
                        };
                        *state = QuoteLaneState::Canceling {
                            order_id: live_order_id.clone(),
                            remove_at,
                            pending_replacement: Some(replacement),
                        };
                        order_id_to_lane.insert(order_id.clone(), lane_key);
                    }
                }
                QuoteLaneState::Canceling {
                    pending_replacement,
                    ..
                } => {
                    if let Some(replacement) = pending_replacement {
                        // Once canceling is armed, keep the cancel schedule and only update
                        // which quote should be placed after cancellation completes.
                        replacement.quote = quote;
                        order_id = replacement.order_id.clone();
                    } else {
                        self.lifecycle_record_place_request(&order_id, &quote)?;
                        *pending_replacement = Some(ScheduledQuote {
                            order_id: order_id.clone(),
                            quote,
                        });
                        order_id_to_lane.insert(order_id.clone(), lane_key);
                    }
                }
            }
        } else {
            // Compute when this order becomes eligible for fills (latency simulation)
            let eligible_for_fills_at = self.compute_eligibility_time(order.timestamp);

            self.lifecycle_record_place_request(&order_id, &quote)?;
            let lifecycle_state = self.lifecycle_record_place_success(&order_id)?;
            if !matches!(lifecycle_state, LifecycleState::Open) {
                return Err(anyhow!(
                    "unexpected lifecycle state after immediate place success for {}: {:?}",
                    order_id,
                    lifecycle_state
                ));
            }

            // Create pending order
            let pending = PendingBacktestOrder {
                order_id: order_id.clone(),
                mint: order.mint.clone(),
                market: order.market.clone(),
                side,
                price: limit_price,
                original_size: size,
                remaining_size: size,
                placed_at: order.timestamp,
                time_in_force,
                signal_id: order.signal_id.clone(),
                exit_mode: order.exit_mode,
                context: order.context.clone(),
                eligible_for_fills_at,
                active_until: None,
            };

            // Add to pending orders
            {
                let mut orders = self.pending_orders.lock().await;
                orders.insert(order_id.clone(), pending);
            }
        }

        debug!(
            order_id = %order_id,
            mint = %order.mint,
            side = ?side,
            price = limit_price,
            size = size,
            "Limit order placed"
        );

        Ok(LimitOrderEvent::OrderPlaced {
            order_id,
            mint: order.mint,
            market: order.market,
            price: limit_price,
            size,
            side,
            timestamp: order.timestamp,
            signal_id: order.signal_id,
            context: order.context,
        })
    }

    /// Cancel a pending limit order by its order ID.
    pub(super) async fn cancel_order_with_lifecycle(
        &self,
        order_id: &str,
    ) -> Result<LimitOrderEvent> {
        let now = self.current_time().await;

        if self.latency_config.is_some() {
            let remove_at = now + Duration::milliseconds(self.sample_cancel_latency_ms() as i64);

            let mut lanes = self.quote_lanes.lock().await;
            let mut pending_orders = self.pending_orders.lock().await;
            let order_id_to_lane = self.order_id_to_lane.lock().await;

            if let Some(lane_key) = order_id_to_lane.get(order_id).cloned() {
                if let Some(state) = lanes.get_mut(&lane_key) {
                    match state {
                        QuoteLaneState::Placing {
                            scheduled,
                            live_at: _,
                            cancel_at,
                        } if scheduled.order_id == order_id => {
                            let next_remove_at = match *cancel_at {
                                Some(existing) => existing.min(remove_at),
                                None => remove_at,
                            };
                            *cancel_at = Some(next_remove_at);
                        }
                        QuoteLaneState::Live {
                            order_id: live_id, ..
                        } if live_id == order_id => {
                            if let Some(pending) = pending_orders.get_mut(order_id) {
                                pending.active_until = Some(remove_at);
                            }
                            *state = QuoteLaneState::Canceling {
                                order_id: order_id.to_string(),
                                remove_at,
                                pending_replacement: None,
                            };
                        }
                        QuoteLaneState::Canceling { .. } => {}
                        _ => {}
                    }
                }
                self.lifecycle_record_cancel_request(order_id)?;

                return Ok(LimitOrderEvent::OrderCancelled {
                    order_id: order_id.to_string(),
                    reason: Some("User cancelled".to_string()),
                    timestamp: remove_at,
                });
            }
        }

        self.lifecycle_record_cancel_request(order_id)?;
        let mut orders = self.pending_orders.lock().await;

        match orders.remove(order_id) {
            Some(pending) => {
                let _ = self.lifecycle_record_cancel_confirmed(order_id)?;
                debug!(
                    order_id = %order_id,
                    remaining_size = pending.remaining_size,
                    "Limit order cancelled"
                );
                Ok(LimitOrderEvent::OrderCancelled {
                    order_id: order_id.to_string(),
                    reason: Some("User cancelled".to_string()),
                    timestamp: now,
                })
            }
            None => {
                // Order not found - it was likely already filled or cancelled.
                // Treat as success since the goal (order no longer active) is achieved.
                debug!(
                    order_id = %order_id,
                    "Order not found during cancel (likely already filled or cancelled)"
                );
                Ok(LimitOrderEvent::OrderCancelled {
                    order_id: order_id.to_string(),
                    reason: Some("Order already removed (filled or cancelled)".to_string()),
                    timestamp: now,
                })
            }
        }
    }
}
