//! Polymarket order executor implementation.

use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use polyfill_rs::{ClobClient, OrderArgs, OrderType, Side, types::ExtraOrderArgs};
use popeyes_trading_types::TokenTradeEvent;
use rust_decimal::Decimal;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::{
    client::polymarket::SafeClient,
    event_coordinator::EventCoordinator,
    execution::{
        OrderExecutor,
        events::{ExecutionEvent, LimitOrderEvent, OrderSide, RedemptionEvent, TimeInForce},
        order::{Order, OrderType as TradeOrderType},
    },
    position::PositionManager,
    signal::RedemptionAction,
};

use super::{
    config::PolymarketConfig,
    types::{PendingOrder, PolymarketMetrics},
};

/// Polymarket order executor for CLOB trading.
///
/// Implements the `OrderExecutor` trait for Polymarket's Central Limit Order Book.
/// Uses polyfill-rs for high-performance orderbook operations.
///
/// # Architecture
///
/// This executor is a pure execution layer - it only handles REST API calls for
/// order placement and cancellation. WebSocket connections for fill detection
/// should be managed externally and events enqueued to the EventCoordinator.
///
/// # Fill Detection
///
/// Polymarket's public trade events are anonymous (no order_id). Fill detection
/// should use an external User WebSocket manager that enqueues `LimitOrderEvent`
/// to the EventCoordinator. A polling fallback is available via `poll_pending_orders()`.
///
/// # Pair Redemption
///
/// Supports merging Up + Down token pairs back to USDC via Safe wallet.
/// Requires `safe_rpc_url` and `safe_address` to be configured.
pub struct PolymarketOrderExecutor {
    /// Polymarket CLOB client for REST API calls
    client: Arc<ClobClient>,

    /// Event coordinator for enqueueing events
    event_coordinator: Arc<dyn EventCoordinator>,

    /// Configuration
    config: PolymarketConfig,

    /// Metrics
    metrics: Arc<RwLock<PolymarketMetrics>>,

    /// Safe wallet client for redemption operations
    safe_client: SafeClient,

    /// Order status poller for centralized fill detection
    poller: Arc<super::poller::OrderStatusPoller>,

    /// Merge executor for redemption operations
    merge_executor: Arc<super::poller::MergeExecutor>,

    /// Position manager for drift detection
    position_manager: Arc<dyn PositionManager>,
}

impl PolymarketOrderExecutor {
    /// Create a new executor with the given client, config, event coordinator, safe client, poller, merge executor, and position manager
    pub(crate) fn new(
        client: ClobClient,
        config: PolymarketConfig,
        event_coordinator: Arc<dyn EventCoordinator>,
        safe_client: SafeClient,
        poller: Arc<super::poller::OrderStatusPoller>,
        merge_executor: Arc<super::poller::MergeExecutor>,
        position_manager: Arc<dyn PositionManager>,
    ) -> Self {
        Self {
            client: Arc::new(client),
            event_coordinator,
            config,
            metrics: Arc::new(RwLock::new(PolymarketMetrics::default())),
            safe_client,
            poller,
            merge_executor,
            position_manager,
        }
    }

    /// Get a reference to the order status poller
    pub fn poller(&self) -> Arc<super::poller::OrderStatusPoller> {
        self.poller.clone()
    }

    /// Get a reference to the merge executor
    pub fn merge_executor(&self) -> Arc<super::poller::MergeExecutor> {
        self.merge_executor.clone()
    }

    /// Get event coordinator reference
    pub fn event_coordinator(&self) -> Arc<dyn EventCoordinator> {
        self.event_coordinator.clone()
    }

    /// Get a reference to the config
    pub fn config(&self) -> &PolymarketConfig {
        &self.config
    }

    /// Get a reference to the CLOB client (for testing/advanced use)
    pub fn client(&self) -> &ClobClient {
        &self.client
    }

    /// Get a reference to the Safe client
    pub fn safe_client(&self) -> &SafeClient {
        &self.safe_client
    }

    /// Get a reference to the position manager
    pub fn position_manager(&self) -> Arc<dyn PositionManager> {
        self.position_manager.clone()
    }

    /// Get the number of pending orders from poller.
    pub async fn pending_order_count(&self) -> usize {
        self.poller.monitored_count().await
    }

    /// Get all pending orders from poller (for testing/inspection).
    pub async fn get_pending_orders(&self) -> Vec<PendingOrder> {
        self.poller
            .get_monitored_orders()
            .await
            .into_iter()
            .map(|m| {
                PendingOrder::new(
                    m.order_id,
                    m.mint,
                    m.market,
                    m.side,
                    m.price,
                    m.original_size,
                    TimeInForce::GoodTilCancelled,
                    m.signal_id,
                )
            })
            .collect()
    }

    /// Get a pending order by ID from poller.
    pub async fn get_pending_order(&self, order_id: &str) -> Option<PendingOrder> {
        self.poller
            .get_monitored_orders()
            .await
            .into_iter()
            .find(|m| m.order_id == order_id)
            .map(|m| {
                PendingOrder::new(
                    m.order_id,
                    m.mint,
                    m.market,
                    m.side,
                    m.price,
                    m.original_size,
                    TimeInForce::GoodTilCancelled,
                    m.signal_id,
                )
            })
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> PolymarketMetrics {
        self.metrics.read().await.clone()
    }

    /// Convert trade server OrderSide to polyfill-rs Side
    fn to_polyfill_side(side: OrderSide) -> Side {
        match side {
            OrderSide::Buy => Side::BUY,
            OrderSide::Sell => Side::SELL,
        }
    }

    /// Convert trade server TimeInForce to polyfill-rs OrderType
    fn to_polyfill_order_type(tif: TimeInForce) -> OrderType {
        match tif {
            TimeInForce::GoodTilCancelled => OrderType::GTC,
            TimeInForce::FillOrKill => OrderType::FOK,
            TimeInForce::ImmediateOrCancel => OrderType::FOK, // Use FOK as fallback (FAK not in enum)
        }
    }
}

#[async_trait]
impl OrderExecutor for PolymarketOrderExecutor {
    /// Execute a market order by using FOK limit order with aggressive pricing.
    ///
    /// Polymarket CLOB doesn't have native market orders, so we simulate them:
    /// - Buy: Use FOK with price = 0.99 (near max) to sweep asks
    /// - Sell: Use FOK with price = 0.01 (near min) to sweep bids
    ///
    /// The FOK order type ensures the order is either fully filled immediately
    /// or cancelled entirely, providing market-order-like behavior.
    async fn execute_market_order(&self, order: Order) -> Result<ExecutionEvent> {
        // Determine side and amount from order type
        let (side, size, aggressive_price) = match &order.order_type {
            TradeOrderType::MarketBuy { quote_amount } => {
                // For market buy, we want to sweep asks at high price
                // Use 0.99 as the aggressive price to ensure fill
                let aggressive_price = 0.99;
                // Convert quote amount to shares: shares = quote_amount / price
                let shares = *quote_amount / aggressive_price;
                (OrderSide::Buy, shares, aggressive_price)
            }
            TradeOrderType::MarketSell { token_amount, .. } => {
                // For market sell, we want to sweep bids at low price
                // Use 0.01 as the aggressive price to ensure fill
                let aggressive_price = 0.01;
                (OrderSide::Sell, *token_amount, aggressive_price)
            }
            _ => {
                return Err(anyhow!(
                    "Invalid order type for market order: {:?}",
                    order.order_type
                ));
            }
        };

        // Dry run mode
        if self.config.is_dry_run() {
            let order_id = format!("dry-run-market-{}", uuid::Uuid::new_v4());
            info!(
                dry_run = true,
                order_id = %order_id,
                mint = %order.mint,
                side = ?side,
                size = size,
                aggressive_price = aggressive_price,
                signal_id = ?order.signal_id,
                "[DRY RUN] Would place market order (as FOK)"
            );

            // For buys, token_amount_change is positive; for sells, negative
            let token_amount_change = if matches!(side, OrderSide::Buy) {
                size
            } else {
                -size
            };

            return Ok(ExecutionEvent::OrderFilled {
                mint: order.mint.clone(),
                token_amount_change,
                quote_amount_change: Some(size * aggressive_price),
                price: Some(aggressive_price),
                timestamp: Utc::now(),
                slippage: None,
                clear_position: false,
                force_position_clear: false,
                execution_latency_in_slots: None,
                confirmed_slot: None,
                confirmed_signature: None,
                signal_id: order.signal_id.clone(),
                exit_mode: order.exit_mode,
            });
        }

        info!(
            mint = %order.mint,
            market = ?order.market,
            side = ?side,
            size = size,
            aggressive_price = aggressive_price,
            "Executing market order as FOK limit order"
        );

        // Create order args with aggressive pricing
        let order_args = OrderArgs::new(
            &order.mint,
            Decimal::try_from(aggressive_price).unwrap_or_default(),
            Decimal::try_from(size).unwrap_or_default(),
            Self::to_polyfill_side(side),
        );

        // Create and post FOK order with retry logic
        let max_retries = 3;
        let mut last_error = None;

        for attempt in 1..=max_retries {
            debug!(
                attempt = attempt,
                mint = %order.mint,
                side = ?side,
                size = size,
                aggressive_price = aggressive_price,
                "Creating signed FOK order for market execution"
            );

            // Create signed order
            let signed_order = match self
                .client
                .create_order(&order_args, None, None, None)
                .await
            {
                Ok(order) => order,
                Err(e) => {
                    error!(attempt = attempt, error = %e, "Failed to create market order");
                    last_error = Some(anyhow!("Failed to create order: {}", e));
                    if attempt < max_retries {
                        tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64))
                            .await;
                    }
                    continue;
                }
            };

            // Post as FOK order
            let post_start = std::time::Instant::now();
            match self.client.post_order(signed_order, OrderType::FOK).await {
                Ok(result) => {
                    let post_latency_ms = post_start.elapsed().as_millis();
                    info!(
                        response = %result,
                        mint = %order.mint,
                        side = ?side,
                        size = size,
                        latency_ms = post_latency_ms,
                        "Market order FOK response"
                    );

                    let status = result["status"].as_str().unwrap_or("");

                    if status == "matched" {
                        // FOK order was fully filled
                        let order_id = result["orderID"].as_str().unwrap_or("unknown").to_string();
                        let fill_price = result
                            .get("matchedPrice")
                            .and_then(|p| p.as_str())
                            .and_then(|p| p.parse::<f64>().ok())
                            .unwrap_or(aggressive_price);

                        self.metrics.write().await.orders_placed += 1;
                        self.metrics.write().await.orders_filled += 1;

                        info!(
                            order_id = %order_id,
                            mint = %order.mint,
                            side = ?side,
                            filled_size = size,
                            fill_price = fill_price,
                            "Market order filled"
                        );

                        // For buys, token_amount_change is positive; for sells, negative
                        let token_amount_change = if matches!(side, OrderSide::Buy) {
                            size
                        } else {
                            -size
                        };

                        return Ok(ExecutionEvent::OrderFilled {
                            mint: order.mint.clone(),
                            token_amount_change,
                            quote_amount_change: Some(size * fill_price),
                            price: Some(fill_price),
                            timestamp: Utc::now(),
                            slippage: None,
                            clear_position: false,
                            force_position_clear: false,
                            execution_latency_in_slots: None,
                            confirmed_slot: None,
                            confirmed_signature: None,
                            signal_id: order.signal_id.clone(),
                            exit_mode: order.exit_mode.clone(),
                        });
                    } else {
                        // FOK order was not filled (rejected or expired)
                        warn!(
                            status = status,
                            mint = %order.mint,
                            side = ?side,
                            size = size,
                            "Market order FOK not filled"
                        );
                        return Err(anyhow!(
                            "Market order not filled: status={}, insufficient liquidity at price {}",
                            status,
                            aggressive_price
                        ));
                    }
                }
                Err(e) => {
                    error!(attempt = attempt, error = %e, "Failed to post market order");
                    last_error = Some(anyhow!("Failed to post order: {}", e));
                }
            }

            if attempt < max_retries {
                tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| {
            anyhow!(
                "Failed to execute market order after {} retries",
                max_retries
            )
        }))
    }

    /// Update internal state from trade events.
    /// For Polymarket, this only logs the trade.
    /// Fill detection is done via WebSocket User Channel or polling.
    async fn handle_token_trade(&self, trade: &TokenTradeEvent) -> Result<()> {
        if let TokenTradeEvent::Polymarket(pm_trade) = trade {
            // Note: The OrderBook doesn't track last trade price,
            // but we log for debugging purposes
            debug!(
                asset_id = %pm_trade.asset_id,
                price = pm_trade.price,
                side = ?pm_trade.side,
                size = pm_trade.size,
                "Received trade event"
            );
        }
        // NOTE: Do NOT attempt fill detection here - Polymarket trades are anonymous!
        Ok(())
    }

    /// Execute a limit order on Polymarket CLOB
    async fn execute_limit_order(&self, order: Order) -> Result<LimitOrderEvent> {
        let (side, size, price, time_in_force) = match &order.order_type {
            TradeOrderType::LimitBuy {
                quote_amount,
                limit_price,
                time_in_force,
            } => {
                // Convert quote amount (USDC) to share count: shares = quote_amount / price
                let shares = *quote_amount / *limit_price;
                (OrderSide::Buy, shares, *limit_price, *time_in_force)
            }
            TradeOrderType::LimitSell {
                token_amount,
                limit_price,
                time_in_force,
                ..
            } => {
                // For sells, token_amount is already in shares
                (OrderSide::Sell, *token_amount, *limit_price, *time_in_force)
            }
            _ => {
                return Err(anyhow!("Invalid order type for limit order"));
            }
        };

        // Dry run mode: log and return simulated response
        if self.config.is_dry_run() {
            let order_id = format!("dry-run-{}", uuid::Uuid::new_v4());
            info!(
                dry_run = true,
                order_id = %order_id,
                mint = %order.mint,
                market = ?order.market,
                side = ?side,
                size = size,
                price = price,
                time_in_force = ?time_in_force,
                signal_id = ?order.signal_id,
                "[DRY RUN] Would place limit order"
            );

            return Ok(LimitOrderEvent::OrderPlaced {
                order_id,
                mint: order.mint.clone(),
                market: order.market.clone(),
                price,
                size,
                side,
                timestamp: Utc::now(),
                signal_id: order.signal_id.clone(),
                context: order.context.clone(),
            });
        }

        debug!(
            mint = %order.mint,
            side = ?side,
            size = size,
            price = price,
            "Executing limit order"
        );

        // Create order args (size = number of shares, price = price per share)
        let order_args = OrderArgs::new(
            &order.mint,
            Decimal::try_from(price).unwrap_or_default(),
            Decimal::try_from(size).unwrap_or_default(),
            Self::to_polyfill_side(side),
        );

        // Create and post order with retry logic
        let order_type = Self::to_polyfill_order_type(time_in_force);
        let max_retries = 3;
        let mut last_error = None;

        let order_id = 'retry: {
            for attempt in 1..=max_retries {
                debug!(
                    attempt = attempt,
                    mint = %order.mint,
                    side = ?side,
                    size = size,
                    price = price,
                    order_value = size * price,
                    "Creating signed order"
                );

                // Create signed order (needs fresh signature each attempt)
                // Polymarket requires fee_rate_bps to match market's maker fee (1000 = 0.1%)
                let extras = ExtraOrderArgs {
                    fee_rate_bps: 1000,
                    ..Default::default()
                };
                let signed_order = match self
                    .client
                    .create_order(&order_args, None, Some(extras), None)
                    .await
                {
                    Ok(order) => {
                        debug!(
                            order_salt = %order.salt,
                            order_maker = %order.maker,
                            order_signer = %order.signer,
                            "Signed order created"
                        );
                        order
                    }
                    Err(e) => {
                        error!(
                            attempt = attempt,
                            error = %e,
                            error_debug = ?e,
                            mint = %order.mint,
                            market = ?order.market,
                            side = ?side,
                            size = size,
                            price = price,
                            "Failed to create order"
                        );
                        last_error = Some(anyhow!("Failed to create order: {}", e));
                        if attempt < max_retries {
                            tokio::time::sleep(std::time::Duration::from_millis(
                                500 * attempt as u64,
                            ))
                            .await;
                        }
                        continue;
                    }
                };

                // Post order
                let post_start = std::time::Instant::now();
                match self.client.post_order(signed_order, order_type).await {
                    Ok(result) => {
                        let post_latency_ms = post_start.elapsed().as_millis();
                        // Log the full API response for debugging
                        info!(
                            response = %result,
                            mint = %order.mint,
                            side = ?side,
                            size = size,
                            price = price,
                            latency_ms = post_latency_ms,
                            "Received post_order response"
                        );

                        if let Some(id) = result["orderID"].as_str() {
                            // Check if order was immediately matched
                            let was_matched = result["status"].as_str() == Some("matched");

                            break 'retry (id.to_string(), was_matched);
                        } else {
                            last_error = Some(anyhow!("No order ID in response: {}", result));
                            warn!(attempt = attempt, response = %result, "No order ID in response, retrying...");
                        }
                    }
                    Err(e) => {
                        // Log the full error for debugging
                        error!(
                            attempt = attempt,
                            error = %e,
                            error_debug = ?e,
                            mint = %order.mint,
                            market = ?order.market,
                            side = ?side,
                            size = size,
                            price = price,
                            order_value = size * price,
                            "Failed to post order"
                        );
                        last_error = Some(anyhow!("Failed to post order: {}", e));
                    }
                }

                if attempt < max_retries {
                    tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64))
                        .await;
                }
            }
            return Err(last_error.unwrap_or_else(|| {
                anyhow!("Failed to place order after {} retries", max_retries)
            }));
        };

        // Destructure result from retry block
        let (order_id, was_matched) = order_id;

        self.metrics.write().await.orders_placed += 1;

        // Handle immediate match: return fill event directly without registering with poller
        // The caller will enqueue the event
        if was_matched {
            // status="matched" means fully filled, use original order size
            let filled_size = size;

            info!(
                order_id = %order_id,
                mint = %order.mint,
                side = ?side,
                filled_size = filled_size,
                price = price,
                "Order immediately matched"
            );

            self.metrics.write().await.orders_filled += 1;

            // Return fill event - caller will enqueue it
            return Ok(LimitOrderEvent::OrderPartiallyFilled {
                order_id,
                mint: order.mint,
                side,
                filled_size,
                remaining_size: 0.0,
                fill_price: price,
                timestamp: Utc::now(),
                signal_id: order.signal_id,
                exit_mode: None,
            });
        }

        // Register with poller for fill detection (non-immediate orders only)
        let monitored = super::poller::MonitoredOrder::new(
            order_id.clone(),
            order.mint.clone(),
            order.market.clone(),
            side,
            price,
            size,
            order.signal_id.clone(),
        );
        self.poller.monitor_order(monitored).await;

        info!(
            order_id = %order_id,
            mint = %order.mint,
            side = ?side,
            size = size,
            price = price,
            "Limit order placed"
        );

        let event = LimitOrderEvent::OrderPlaced {
            order_id,
            mint: order.mint,
            market: order.market,
            price,
            size,
            side,
            timestamp: Utc::now(),
            signal_id: order.signal_id,
            context: None,
        };

        // Note: Event is NOT enqueued here - the caller (order_executor.rs) handles enqueuing
        // to avoid double-enqueue issues
        Ok(event)
    }

    /// Cancel an existing order
    async fn cancel_order(&self, order_id: &str) -> Result<LimitOrderEvent> {
        // Dry run mode: log and return simulated response
        if self.config.is_dry_run() {
            info!(
                dry_run = true,
                order_id = %order_id,
                "[DRY RUN] Would cancel order"
            );

            return Ok(LimitOrderEvent::OrderCancelled {
                order_id: order_id.to_string(),
                reason: Some("Dry run cancel".to_string()),
                timestamp: Utc::now(),
            });
        }

        debug!(order_id = %order_id, "Cancelling order");

        // Don't unmonitor here - let poller detect CANCELED status and handle any fills
        // before removing from monitoring

        self.client
            .cancel(order_id)
            .await
            .map_err(|e| anyhow!("Failed to cancel order: {}", e))?;

        self.poller.record_cancel_request(order_id).await;

        self.metrics.write().await.orders_cancelled += 1;

        info!(
            order_id = %order_id,
            "Cancel command acknowledged; awaiting poller/websocket terminal confirmation"
        );

        let event = LimitOrderEvent::OrderCancelled {
            order_id: order_id.to_string(),
            reason: Some("Cancel requested (awaiting confirmation)".to_string()),
            timestamp: Utc::now(),
        };

        // Note: Event is NOT enqueued here - the caller (order_executor.rs) handles enqueuing
        // to avoid double-enqueue issues
        Ok(event)
    }

    /// Returns true - Polymarket supports limit orders
    fn supports_limit_orders(&self) -> bool {
        true
    }

    /// Queue a redemption request for the merge executor to process.
    ///
    /// This method does NOT execute the merge directly. Instead, it queues
    /// the request to the MergeExecutor, which will execute the merge
    /// in its next poll cycle after checking current position quantities.
    ///
    /// Always returns RedemptionFailed with a "queued" message to indicate
    /// that the actual execution is pending.
    async fn execute_redemption(&self, action: &RedemptionAction) -> Result<RedemptionEvent> {
        info!(
            market = %action.market,
            up_asset = %action.up_asset_id,
            down_asset = %action.down_asset_id,
            quantity = action.quantity,
            "Queueing redemption request to merge executor"
        );

        // Queue the merge request to the merge executor
        self.merge_executor.request_merge(action.clone()).await;

        // Return "pending" status - actual execution happens in merge executor
        Ok(RedemptionEvent::RedemptionFailed {
            market: action.market.clone(),
            reason: "Queued for execution by merge executor".to_string(),
            timestamp: Utc::now(),
        })
    }

    /// Returns true - redemption is supported via merge executor
    fn supports_redemption(&self) -> bool {
        true
    }

    fn defers_cancel_order_events(&self) -> bool {
        true
    }
}
