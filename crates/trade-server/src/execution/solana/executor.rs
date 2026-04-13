use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::leader_monitor::LeaderMonitorService;
use anyhow::{Context, Result};
use async_trait::async_trait;
use popeyes_trading_types::TokenTradeEvent;
use solana_client::rpc_client::RpcClient;
use solana_sdk::message::VersionedMessage;
use solana_signature::Signature;
use solana_sub::redis_tx_retriever::RedisTxRetriever;
use tokio_retry2::{Retry, strategy::FixedInterval};
use tracing::{info, instrument};

use super::{
    confirmation::GrpcConfirmationMonitor,
    submission::TransactionService,
    tx_constructor::TransactionConstructor,
    utils::{TokenTransactionError, TransactionDetails, extract_token_transaction_details},
};
use crate::{
    execution::{
        OrderExecutor,
        events::{ExecutionEvent, SimulationMode},
        order::{Order, OrderType},
    },
    signal::TradableSignal,
};

#[async_trait]
impl OrderExecutor for SolanaOrderExecutor {
    async fn handle_signal(&self, signal: &dyn TradableSignal) -> Result<()> {
        // Pass the signal to the transaction constructor for caching
        self.transaction_constructor.cache_signal(signal);
        Ok(())
    }

    #[instrument(skip(self), level = "info", field(signal_id=order.signal_id.as_deref().unwrap_or("")))]
    async fn execute_market_order(&self, order: Order) -> Result<ExecutionEvent> {
        // Check if this is a buy order and we've reached the trade limit for real trades
        let real_trade_mode = self.simulation_mode == SimulationMode::Disabled;
        if real_trade_mode && order.order_type.is_buy() {
            if let Some(max_trades) = self.max_real_trades {
                let reached_limit = {
                    let state = self.state.lock().unwrap();
                    state.executed_trades_count >= max_trades
                };

                if reached_limit {
                    info!(
                        mint = order.mint.as_str(),
                        max_trades = max_trades,
                        "Buy trade limit reached. Rejecting order"
                    );
                    return Ok(ExecutionEvent::OrderRejected {
                        mint: order.mint,
                        reason: format!("Buy trade limit of {} reached", max_trades),
                    });
                }
            }
        }

        match self.simulation_mode {
            SimulationMode::Pure => self.create_simulated_execution_event(&order),
            SimulationMode::RpcBased => self.execute_simulated_order_via_rpc(&order).await,
            SimulationMode::Disabled => {
                // --- Leader Check Start ---
                // Only check for bad leaders on buy orders since sells can't be sandwich attacked
                if order.order_type.is_buy() {
                    if let Some(monitor) = &self.leader_monitor_service {
                        if let (Some(start_slot), Some(latency)) =
                            (order.signal_slot, self.max_slot_latency)
                        {
                            let end_slot = start_slot + latency;
                            tracing::debug!(
                                start_slot,
                                end_slot,
                                "Checking for potential bad leader in slot range"
                            );
                            match monitor
                                .is_target_validator_scheduled_leader(start_slot, end_slot)
                                .await
                            {
                                Ok(true) => {
                                    info!(
                                        mint = order.mint.as_str(),
                                        start_slot,
                                        end_slot,
                                        "Potential bad leader detected in schedule. Rejecting order."
                                    );
                                    return Ok(ExecutionEvent::OrderRejected {
                                        mint: order.mint.clone(),
                                        reason: "Potential bad leader in execution window"
                                            .to_string(),
                                    });
                                }
                                Ok(false) => {
                                    tracing::debug!(
                                        start_slot,
                                        end_slot,
                                        "No bad leader detected in range."
                                    );
                                }
                                Err(e) => {
                                    tracing::error!(error = ?e, "Leader check failed. Rejecting order as safe default.");
                                    return Ok(ExecutionEvent::OrderRejected {
                                        mint: order.mint.clone(),
                                        reason: format!("Leader check failed: {}", e),
                                    });
                                }
                            }
                        } else {
                            info!(
                                "Skipping leader check: signal_slot or max_slot_latency not available."
                            );
                        }
                    }
                }
                // --- Leader Check End ---
                let result = self.execute_real_order(&order).await;

                // Update the counter only if the operation was successful and it was a buy order
                #[allow(deprecated)]
                match (&result, &order.order_type) {
                    (Ok(_), order_type) if order_type.is_buy() => {
                        let mut state = self.state.lock().unwrap();
                        state.executed_trades_count += 1;
                        info!(
                            mint = order.mint.as_str(),
                            executed_trades_count = state.executed_trades_count,
                            max_real_trades = match self.max_real_trades {
                                Some(max) => max.to_string(),
                                None => "unlimited".to_string(),
                            },
                            "Executed real buy trade"
                        );
                    }
                    (Ok(_), order_type) if order_type.is_sell() => {
                        info!(mint = order.mint.as_str(), "Executed real sell trade");
                    }
                    (Err(e), order_type) if order_type.is_buy() => {
                        tracing::error!(
                            mint = order.mint.as_str(),
                            error = ?e,
                            "Failed to execute real buy trade"
                        );
                    }
                    (Err(e), order_type) if order_type.is_sell() => {
                        tracing::error!(
                            mint = order.mint.as_str(),
                            error = ?e,
                            "Failed to execute real sell trade"
                        );
                    }
                    _ => {} // Handle other cases (limit orders, etc.)
                }

                result
            }
        }
    }

    async fn handle_token_trade(&self, trade: &TokenTradeEvent) -> Result<()> {
        // Forward event to the central gRPC confirmation monitor
        self.grpc_confirmer.handle_token_trade(trade).await;
        Ok(())
    }
}

#[derive(Debug)]
struct ExecutorState {
    executed_trades_count: u32,
}

pub struct SolanaOrderExecutor {
    sim_buy_slippage: f64,
    sim_sell_slippage: f64,
    simulation_mode: SimulationMode,
    state: Mutex<ExecutorState>,
    rpc_client: RpcClient,
    transaction_service: TransactionService,
    transaction_constructor: Box<dyn TransactionConstructor>,
    max_real_trades: Option<u32>,
    skip_simulation_when_sell: bool,
    skip_simulation_for_buy: bool,
    max_slot_latency: Option<u64>,
    redis_tx_retriever: Option<RedisTxRetriever>,
    grpc_confirmer: GrpcConfirmationMonitor,
    leader_monitor_service: Option<Arc<LeaderMonitorService>>,
}

impl SolanaOrderExecutor {
    pub fn new(
        transaction_service: TransactionService,
        transaction_constructor: Box<dyn TransactionConstructor>,
        rpc_url: String,
        sim_buy_slippage: f64,  // Simulation buy slippage
        sim_sell_slippage: f64, // Simulation sell slippage
        simulation_mode: SimulationMode,
        max_real_trades: Option<u32>,
        skip_simulation_when_sell: bool,
        skip_simulation_for_buy: bool,
        max_slot_latency: Option<u64>,
        redis_tx_retriever: Option<RedisTxRetriever>,
        grpc_confirmer: GrpcConfirmationMonitor,
        leader_monitor_service: Option<Arc<LeaderMonitorService>>,
    ) -> Self {
        tracing::info!(
            rpc_url = rpc_url.as_str(),
            sim_buy_slippage,
            sim_sell_slippage,
            simulation_mode = ?simulation_mode,
            max_real_trades = max_real_trades.map(|max| max.to_string()),
            skip_simulation_when_sell,
            skip_simulation_for_buy,
            max_slot_latency = max_slot_latency.map(|latency| latency.to_string()),
            has_leader_monitor = leader_monitor_service.is_some(),
            "Creating SolanaOrderExecutor"
        );
        Self {
            sim_buy_slippage,
            sim_sell_slippage,
            simulation_mode,
            state: Mutex::new(ExecutorState {
                executed_trades_count: 0,
            }),
            rpc_client: RpcClient::new(rpc_url),
            transaction_service,
            transaction_constructor,
            max_real_trades,
            skip_simulation_when_sell,
            skip_simulation_for_buy,
            max_slot_latency,
            redis_tx_retriever,
            grpc_confirmer,
            leader_monitor_service,
        }
    }

    #[instrument(skip(self), fields(signal_id=order.signal_id.as_deref().unwrap_or("")))]
    async fn execute_real_order(&self, order: &Order) -> Result<ExecutionEvent> {
        // Log the order details
        let transaction_type = if order.order_type.is_buy() {
            "Buy"
        } else {
            "Sell"
        };
        let amount = if let Some(quote_amount) = order.order_type.quote_amount() {
            format!("{} SOL", quote_amount)
        } else if order.order_type.clear_position().unwrap_or(false) {
            "100%".to_string()
        } else if let Some(token_amount) = order.order_type.token_amount() {
            format!("{} tokens", token_amount)
        } else {
            "unknown".to_string()
        };
        info!(
            signer = self
                .transaction_service
                .wallet_pubkey()
                .to_string()
                .as_str(),
            transaction_type,
            mint = order.mint.as_str(),
            amount,
            "Executing order"
        );

        // Use the transaction constructor to build the transaction message
        let no_later_than_slot = if order.order_type.is_buy() {
            // Calculate the 'noLaterThan' slot only for buy orders if applicable
            match (order.signal_slot, self.max_slot_latency) {
                (Some(signal_slot), Some(max_latency)) => {
                    let target_slot = signal_slot + max_latency;
                    info!(
                        signal_slot,
                        max_latency, target_slot, "Calculated noLaterThan slot for BUY order"
                    );
                    Some(target_slot)
                }
                _ => None,
            }
        } else {
            None
        };

        let log_str = order.signal_id.as_ref().map(|id| format!("s={}", id));

        let message = self
            .transaction_constructor
            .construct_transaction(order, no_later_than_slot, log_str)
            .await?;

        // Determine if we should skip simulation
        let skip_simulation = if order.order_type.is_sell() {
            self.skip_simulation_when_sell
        } else {
            self.skip_simulation_for_buy
        };

        // Sign and submit the transaction
        let signature = self
            .transaction_service
            .submit(&message, skip_simulation)
            .await?;

        // Track tx status
        let confirmation_result = self
            .transaction_service
            .confirm_transaction(signature)
            .await;

        match confirmation_result {
            Ok(_) => self.handle_successful_execution(signature, order).await,
            Err(e) => {
                tracing::info!(
                    ?order,
                    error = ?e,
                    "Failed to confirm transaction",
                );
                // Handle confirmation failure differently for buy and sell orders
                if order.order_type.is_buy() {
                    self.handle_buy_confirmation_failure(order, signature)
                } else {
                    self.handle_sell_confirmation_failure(&message, order, &e)
                        .await
                }
            }
        }
    }

    #[instrument(skip(self))]
    #[allow(dead_code)]
    async fn handle_real_execution(&self, data: Vec<u8>, order: &Order) -> Result<ExecutionEvent> {
        // Deserialize the transaction message
        let message = bincode::deserialize::<VersionedMessage>(&data)
            .context("Failed to deserialize transaction message")?;

        // Determine if we should skip simulation
        let skip_simulation = if order.order_type.is_sell() {
            self.skip_simulation_when_sell
        } else {
            self.skip_simulation_for_buy
        };

        // Sign and submit the transaction
        let result = self
            .transaction_service
            .submit(&message, skip_simulation)
            .await;

        match result {
            Ok(signature) => {
                // Track tx status.
                let confirmation_result = self
                    .transaction_service
                    .confirm_transaction(signature)
                    .await;

                match confirmation_result {
                    Ok(_) => self.handle_successful_execution(signature, order).await,
                    Err(e) => {
                        tracing::info!(
                            ?order,
                            error = ?e,
                            "Failed to confirm transaction",
                        );
                        // Handle confirmation failure differently for buy and sell orders
                        if order.order_type.is_buy() {
                            self.handle_buy_confirmation_failure(order, signature)
                        } else {
                            self.handle_sell_confirmation_failure(&message, order, &e)
                                .await
                        }
                    }
                }
            }
            Err(e) => Err(e),
        }
    }

    #[instrument(skip(self))]
    fn handle_buy_confirmation_failure(
        &self,
        order: &Order,
        signature: Signature,
    ) -> Result<ExecutionEvent> {
        info!(
            mint = order.mint.as_str(),
            signature = signature.to_string(),
            "Buy transaction failed to confirm, likely due to network congestion. Ignoring."
        );
        // For buy orders, return a rejection with explanation
        Ok(ExecutionEvent::OrderRejected {
            mint: order.mint.clone(),
            reason: "Transaction was submitted but failed to confirm".to_string(),
        })
    }

    #[instrument(skip(self, message))]
    async fn handle_sell_confirmation_failure(
        &self,
        message: &VersionedMessage,
        order: &Order,
        error: &anyhow::Error,
    ) -> Result<ExecutionEvent> {
        // First check if this is an insufficient position error
        if error.to_string().contains("custom program error: 6023") {
            info!(
                mint = order.mint.as_str(),
                "Sell failed due to insufficient position, forcing position clear"
            );
            return Ok(ExecutionEvent::OrderFilled {
                mint: order.mint.clone(),
                token_amount_change: 0.0,
                quote_amount_change: None,
                price: None,
                timestamp: order.timestamp,
                slippage: None,
                clear_position: true,
                force_position_clear: true,
                execution_latency_in_slots: None,
                signal_id: order.signal_id.clone(),
                confirmed_slot: Some(0), // Placeholder: No specific tx confirmed in this path
                confirmed_signature: Some(Signature::default()), // Placeholder
                exit_mode: order.exit_mode,
            });
        }

        info!(
            mint = order.mint.as_str(),
            "Sell transaction failed to confirm. Attempting additional retries."
        );

        let retry_strategy = FixedInterval::from_millis(1000).take(5);

        let retry_result = Retry::spawn_notify(
            retry_strategy,
            || async {
                // Determine if we should skip simulation
                let skip_simulation = if order.order_type.is_sell() {
                    self.skip_simulation_when_sell
                } else {
                    false // Always simulate for buy orders
                };

                // Sign and submit the transaction
                let signature = self
                    .transaction_service
                    .submit(message, skip_simulation)
                    .await?;

                // Try to confirm this transaction
                self.transaction_service
                    .confirm_transaction(signature)
                    .await?;

                Ok(signature)
            },
            |error: &anyhow::Error, duration: Duration| {
                info!(
                    error = ?error,
                    retry_in = ?duration,
                    "Retrying sell transaction after confirmation failure"
                );
            },
        )
        .await;

        match retry_result {
            Ok(new_signature) => self.handle_successful_execution(new_signature, order).await,
            Err(e) => Err(e.context("Failed to confirm sell transaction after multiple retries")),
        }
    }

    #[instrument(skip(self))]
    async fn handle_successful_execution(
        &self,
        signature: Signature,
        order: &Order,
    ) -> Result<ExecutionEvent> {
        let transaction_type = if order.order_type.is_buy() {
            "buy"
        } else {
            "sell"
        };
        let clear_position = order.order_type.clear_position().unwrap_or(false);

        match self
            .get_execution_details(signature, &order.mint, &order.order_type)
            .await
        {
            Ok(exec_details) => {
                info!(
                    transaction_type,
                    mint = order.mint.as_str(),
                    details = ?exec_details,
                    "Executed order"
                );

                let execution_latency_in_slots = order.signal_slot.map(|slot| {
                    let latency = exec_details.slot.saturating_sub(slot);
                    info!(
                        transaction_type,
                        mint = order.mint.as_str(),
                        latency_in_slots = ?latency,
                        "Calcuated execution latency in executor (signal slot -> filled slot)",
                    );
                    latency
                });

                Ok(ExecutionEvent::OrderFilled {
                    mint: order.mint.clone(),
                    token_amount_change: exec_details.token_amount,
                    quote_amount_change: Some(exec_details.sol_amount),
                    price: Some(exec_details.price.abs()),
                    timestamp: order.timestamp,
                    slippage: None,
                    clear_position,
                    force_position_clear: false,
                    execution_latency_in_slots,
                    signal_id: order.signal_id.clone(),
                    confirmed_slot: Some(exec_details.slot),
                    confirmed_signature: Some(signature),
                    exit_mode: order.exit_mode,
                })
            }
            Err(TokenTransactionError::PositionAlreadyCleared) => {
                info!(
                    mint = order.mint.as_str(),
                    "Position already cleared, forcing position update"
                );
                Ok(ExecutionEvent::OrderFilled {
                    mint: order.mint.clone(),
                    token_amount_change: 0.0,
                    quote_amount_change: None,
                    price: None,
                    timestamp: order.timestamp,
                    slippage: None,
                    clear_position,
                    force_position_clear: true,
                    execution_latency_in_slots: None,
                    signal_id: order.signal_id.clone(),
                    confirmed_slot: Some(0), // Placeholder: Position already cleared, no specific confirmation details relevant here
                    confirmed_signature: Some(Signature::default()), // Placeholder
                    exit_mode: order.exit_mode,
                })
            }
            Err(e) => {
                tracing::error!(
                    mint = order.mint.as_str(),
                    error = ?e,
                    "Failed to get execution details"
                );
                Err(e.into())
            }
        }
    }

    #[instrument(skip(self))]
    async fn get_execution_details(
        &self,
        signature: Signature,
        mint: &str,
        order_type: &OrderType,
    ) -> Result<TransactionDetails, TokenTransactionError> {
        let wallet_pubkey = self.transaction_service.wallet_pubkey().to_string();

        // Try Redis first if available
        if let Some(redis_retriever) = &self.redis_tx_retriever {
            match super::utils::extract_token_transaction_details_from_redis(
                redis_retriever,
                &signature,
                mint,
                &wallet_pubkey,
                order_type,
            )
            .await
            {
                Ok(details) => {
                    tracing::info!(
                        source = "redis",
                        signature = signature.to_string(),
                        "Successfully retrieved transaction details from Redis"
                    );
                    return Ok(details);
                }
                Err(e) => {
                    tracing::debug!(
                        error = ?e,
                        signature = signature.to_string(),
                        "Failed to retrieve transaction details from Redis, falling back to RPC"
                    );
                    // Fall through to RPC method
                }
            }
        }

        // Fall back to RPC method
        tracing::debug!(
            signature = signature.to_string(),
            "Retrieving transaction details from RPC"
        );

        let retry_strategy = FixedInterval::from_millis(500).take(3);

        let retry_result = Retry::spawn_notify(
            retry_strategy,
            || async {
                let result = extract_token_transaction_details(
                    &self.rpc_client,
                    &signature,
                    mint,
                    &wallet_pubkey,
                    order_type,
                )
                .await?;

                Ok(result)
            },
            |error: &TokenTransactionError, duration: Duration| {
                tracing::info!(
                    error = ?error,
                    retry_in = ?duration,
                    "Retrying transaction details fetch"
                );
            },
        )
        .await;

        match retry_result {
            Ok(details) => {
                tracing::info!(
                    source = "rpc",
                    signature = signature.to_string(),
                    "Successfully retrieved transaction details from RPC"
                );
                Ok(details)
            }
            Err(e) => {
                tracing::error!(
                    mint = mint,
                    signature = signature.to_string(),
                    "Failed to retrieve transaction details from RPC after retries: {}",
                    e
                );
                Err(e)
            }
        }
    }

    #[instrument(skip(self))]
    #[allow(deprecated)]
    fn create_simulated_execution_event(&self, order: &Order) -> Result<ExecutionEvent> {
        let (amount, price, slippage, clear_position) = match &order.order_type {
            OrderType::Buy { sol_amount }
            | OrderType::MarketBuy {
                quote_amount: sol_amount,
            } => {
                let simulated_slippage = self.sim_buy_slippage; // Use sim slippage here
                let simulated_price = order.price.expect("Order price must be set for simulation")
                    * (1.0 + simulated_slippage);
                let token_amount = sol_amount / simulated_price;
                (token_amount, simulated_price, simulated_slippage, false)
            }
            OrderType::Sell {
                token_amount,
                clear_position,
            }
            | OrderType::MarketSell {
                token_amount,
                clear_position,
            } => {
                let simulated_slippage = self.sim_sell_slippage; // Use sim slippage here
                let simulated_price = order.price.expect("Order price must be set for simulation")
                    * (1.0 - simulated_slippage);
                (
                    -token_amount,
                    simulated_price,
                    simulated_slippage,
                    *clear_position,
                )
            }
            OrderType::LimitBuy {
                quote_amount,
                limit_price,
                ..
            } => {
                // For limit buy simulation, use the limit price
                let token_amount = quote_amount / limit_price;
                (token_amount, *limit_price, 0.0, false)
            }
            OrderType::LimitSell {
                token_amount,
                limit_price,
                clear_position,
                ..
            } => {
                // For limit sell simulation, use the limit price
                (-token_amount, *limit_price, 0.0, *clear_position)
            }
            OrderType::Cancel { order_id } => {
                // Cancel orders should not be passed to create_simulated_execution_event
                anyhow::bail!(
                    "Cancel orders should not be passed to create_simulated_execution_event, use cancel_order() instead: {}",
                    order_id
                );
            }
        };

        let order_filled = ExecutionEvent::OrderFilled {
            mint: order.mint.clone(),
            token_amount_change: amount,
            quote_amount_change: None,
            price: Some(price),
            timestamp: order.timestamp,
            slippage: Some(slippage),
            clear_position,
            force_position_clear: false,
            execution_latency_in_slots: None,
            signal_id: order.signal_id.clone(),
            confirmed_slot: Some(0), // Placeholder for pure simulation
            confirmed_signature: Some(Signature::default()), // Placeholder for pure simulation
            exit_mode: order.exit_mode,
        };

        info!(
            order_filled = ?order_filled,
            "Simulated order execution"
        );

        Ok(order_filled)
    }

    #[instrument(skip(self))]
    async fn execute_simulated_order_via_rpc(&self, order: &Order) -> Result<ExecutionEvent> {
        // Step 1: Construct the transaction (reuse existing logic)
        let log_str = order.signal_id.as_ref().map(|id| format!("s={}", id));
        // For RPC-based simulation, a price is critical for the simulation event generation.
        // If order.price is somehow not set (which shouldn't happen for valid orders),
        // it's better to error out than to proceed with a potentially meaningless simulation.
        if order.price.is_none() {
            // Assuming 0.0 is an invalid/unset price
            return Err(anyhow::anyhow!(
                "Order price is not set for RPC-based simulation"
            ));
        }

        let message = self
            .transaction_constructor
            .construct_transaction(order, None, log_str)
            .await?;

        // Step 2: Create a complete transaction for simulation
        let blockhash = self.rpc_client.get_latest_blockhash()?;
        let mut tx_message = message.clone();

        // Update the blockhash in the message
        match &mut tx_message {
            solana_sdk::message::VersionedMessage::Legacy(message) => {
                message.recent_blockhash = blockhash;
            }
            solana_sdk::message::VersionedMessage::V0(message) => {
                message.recent_blockhash = blockhash;
            }
        }

        let tx = self
            .transaction_service
            .signing_service()
            .sign(&tx_message)
            .await?;

        // Step 3: Simulate the transaction
        let sim_result = self.rpc_client.simulate_transaction(&tx)?;

        match sim_result.value.err {
            Some(err) => {
                info!(
                    error = ?err,
                    "Simulation failed"
                );
                return Ok(ExecutionEvent::OrderRejected {
                    mint: order.mint.clone(),
                    reason: format!("Simulation error: {:?}", err),
                });
            }
            None => {
                if let Some(units) = sim_result.value.units_consumed {
                    info!(compute_units_consumed = units, "RPC simulation successful");
                } else {
                    info!("RPC simulation successful (no compute units data)");
                }
            }
        }

        info!("Creating simulated execution event");

        let event = self.create_simulated_execution_event(order)?;

        // Ensure price is valid for OrderFilled events
        if let ExecutionEvent::OrderFilled { price, .. } = &event {
            if price.is_none() {
                return Err(anyhow::anyhow!(
                    "Simulated execution event created with no price during RPC simulation"
                ));
            }
        }

        Ok(event)
    }
}
