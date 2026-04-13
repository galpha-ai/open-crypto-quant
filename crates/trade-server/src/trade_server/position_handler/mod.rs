//! Position handler module - coordinates position lifecycle and signal handling.
//!
//! This module provides `PositionHandler`, the main coordinator that routes
//! events and signals to specialized handlers:
//!
//! - `event_handlers`: Position/execution/timer event handling
//! - `signal_handlers`: Entry/exit/modify/cancel signal handling
//! - `intent_handlers`: Intent-based orderbook signal handling
//! - `order_executor`: Common async order execution patterns

mod event_handlers;
mod intent_handlers;
mod order_executor;
mod signal_handlers;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use tracing::info;

use crate::{
    domain::TimerEvent,
    event_coordinator::EventCoordinator,
    execution::{ExecutionEvent, OrderExecutor},
    notifier::Notifier,
    persistence::TradingEventPersistence,
    position::{ExitStrategy, PositionEvent, PositionManager},
    signal::{SignalAction, TradableSignal},
    trade_server::TradeServerMetrics,
};

use event_handlers::PositionEventHandler;
use intent_handlers::IntentHandler;
use signal_handlers::SignalHandler;

/// Coordinates position lifecycle events and signal handling.
///
/// `PositionHandler` is the main entry point for position-related operations.
/// It delegates to specialized handlers for different event/signal types:
///
/// - Position events (created, closed, updated) → `PositionEventHandler`
/// - Execution events (order filled, rejected) → `PositionEventHandler`
/// - Timer events (time-based exits) → `PositionEventHandler`
/// - Entry/Exit/Modify/Cancel signals → `SignalHandler`
/// - Intent-based signals → `IntentHandler`
#[derive(Clone)]
pub struct PositionHandler {
    event_handler: Arc<PositionEventHandler>,
    signal_handler: Arc<SignalHandler>,
    intent_handler: Arc<IntentHandler>,
    order_executor: Arc<dyn OrderExecutor + Send + Sync>,
    position_manager: Arc<dyn PositionManager>,
}

impl PositionHandler {
    pub fn new(
        position_manager: Arc<dyn PositionManager>,
        order_executor: Arc<dyn OrderExecutor + Send + Sync>,
        signal_notifier: Arc<dyn Notifier>,
        event_coordinator: Arc<dyn EventCoordinator>,
        exit_strategy: Arc<dyn ExitStrategy>,
        metrics: Option<Arc<TradeServerMetrics>>,
        max_sell_failures: u32,
    ) -> Self {
        Self::with_persistence(
            position_manager,
            order_executor,
            signal_notifier,
            event_coordinator,
            exit_strategy,
            metrics,
            max_sell_failures,
            None,
        )
    }

    pub fn with_persistence(
        position_manager: Arc<dyn PositionManager>,
        order_executor: Arc<dyn OrderExecutor + Send + Sync>,
        signal_notifier: Arc<dyn Notifier>,
        event_coordinator: Arc<dyn EventCoordinator>,
        exit_strategy: Arc<dyn ExitStrategy>,
        metrics: Option<Arc<TradeServerMetrics>>,
        max_sell_failures: u32,
        persistence: Option<Arc<dyn TradingEventPersistence>>,
    ) -> Self {
        info!(
            max_sell_failures,
            "Creating PositionHandler with exit strategy",
        );

        let event_handler = Arc::new(PositionEventHandler::new(
            Arc::clone(&position_manager),
            Arc::clone(&order_executor),
            signal_notifier,
            Arc::clone(&event_coordinator),
            exit_strategy,
            metrics,
            max_sell_failures,
            persistence,
        ));

        let signal_handler = Arc::new(SignalHandler::new(
            Arc::clone(&position_manager),
            Arc::clone(&order_executor),
            Arc::clone(&event_coordinator),
        ));

        let intent_handler = Arc::new(IntentHandler::new(
            Arc::clone(&position_manager),
            Arc::clone(&order_executor),
            Arc::clone(&event_coordinator),
        ));

        Self {
            event_handler,
            signal_handler,
            intent_handler,
            order_executor,
            position_manager,
        }
    }

    /// Handle a position event (created, closed, or updated).
    pub async fn handle_position_event(&self, event: &PositionEvent) -> Result<()> {
        self.event_handler.handle_position_event(event).await
    }

    /// Handle an execution event (order filled or rejected).
    pub async fn handle_execution_event(&self, event: &ExecutionEvent) -> Result<()> {
        self.event_handler.handle_execution_event(event).await?;

        // After position changes, check if any redemption policies are violated
        // This triggers auto-redemption when fills create paired positions
        self.intent_handler.process_redemption_actions().await;

        Ok(())
    }

    /// Handle a timer event for time-based exits.
    pub async fn handle_timer_event(&self, timer_event: &TimerEvent) -> Result<()> {
        self.event_handler.handle_timer_event(timer_event).await
    }

    /// Update price for a position and enqueue the resulting event.
    pub async fn update_price(
        &self,
        mint: &str,
        price: f64,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        self.event_handler
            .update_price(mint, price, timestamp)
            .await
    }

    /// Update position-related metrics.
    pub async fn update_position_metrics(&self) {
        self.event_handler.update_position_metrics().await
    }

    /// Handle a signal by routing based on its action.
    ///
    /// Routes signals to appropriate handlers:
    /// - Intent-based signals: Reconciles desired state with current orders
    /// - `Entry`: Creates new positions (default, backward compatible)
    /// - `Exit`: Closes existing positions
    /// - `ModifyOrder`: Modifies existing exit orders (cancel + replace)
    /// - `CancelOrder`: Cancels existing exit orders
    pub async fn handle_signal(&self, signal: &dyn TradableSignal) -> Result<()> {
        // First, let the order executor cache the signal
        if let Err(e) = self.order_executor.handle_signal(signal).await {
            tracing::error!(err = ?e, "Failed to cache signal in order executor");
        }

        // Check for intent-based signals first (orderbook market making)
        if signal.is_intent_signal() {
            return self.intent_handler.handle_intent_signal(signal).await;
        }

        // Route based on signal action
        match signal.signal_action() {
            SignalAction::Entry => self.signal_handler.handle_entry_signal(signal).await,
            SignalAction::Exit => self.signal_handler.handle_exit_signal(signal).await,
            SignalAction::ModifyOrder => {
                self.signal_handler.handle_modify_order_signal(signal).await
            }
            SignalAction::CancelOrder => {
                self.signal_handler.handle_cancel_order_signal(signal).await
            }
        }
    }

    /// Get a reference to the position manager.
    pub fn position_manager(&self) -> &Arc<dyn PositionManager> {
        &self.position_manager
    }
}
