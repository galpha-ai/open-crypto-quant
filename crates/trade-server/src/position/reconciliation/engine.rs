//! Reconciliation engine for intent-based order management.
//!
//! The engine computes the diff between current pending orders and desired order
//! state, generating the necessary cancel and placement orders.

use std::collections::HashMap;

use tracing::debug;

use crate::execution::{OrderSide, order::Order};
use crate::position::ExitMode;
use crate::position::errors::PositionError;
use crate::position::pending_order::PendingLimitOrder;
use crate::signal::{OrderIntent, QuoteLevel};

use super::validation::validate_intent;

/// Reconciliation engine for computing order diffs.
///
/// This struct provides pure computation methods that take pending order state
/// as input and return orders to execute. It does not hold any state itself.
#[derive(Debug, Default)]
pub struct ReconciliationEngine;

impl ReconciliationEngine {
    /// Create a new reconciliation engine.
    pub fn new() -> Self {
        Self
    }

    /// Compute the orders needed to reconcile current state with desired intent.
    ///
    /// # Arguments
    ///
    /// * `intent` - The desired order book state
    /// * `current_orders` - Current pending orders for the mint (order_id -> PendingLimitOrder)
    ///
    /// # Returns
    ///
    /// A vector of `Order` objects representing the actions needed, sorted with
    /// cancels before placements.
    ///
    /// # Errors
    ///
    /// Returns `PositionError::InvalidIntent` if the intent fails validation.
    pub fn compute_reconciliation(
        &self,
        intent: &OrderIntent,
        current_orders: &HashMap<String, PendingLimitOrder>,
    ) -> Result<Vec<Order>, PositionError> {
        // Validate intent
        validate_intent(intent)?;

        let mut orders = vec![];

        // Process bids if specified (None means preserve existing)
        if let Some(desired_bids) = &intent.bids {
            let current_bids: Vec<&PendingLimitOrder> = current_orders
                .values()
                .filter(|o| o.side == OrderSide::Buy)
                .collect();
            orders.extend(self.reconcile_side(
                intent,
                &current_bids,
                desired_bids,
                OrderSide::Buy,
            )?);
        }

        // Process asks if specified (None means preserve existing)
        if let Some(desired_asks) = &intent.asks {
            let current_asks: Vec<&PendingLimitOrder> = current_orders
                .values()
                .filter(|o| o.side == OrderSide::Sell)
                .collect();
            orders.extend(self.reconcile_side(
                intent,
                &current_asks,
                desired_asks,
                OrderSide::Sell,
            )?);
        }

        // Sort: cancels first, then placements
        orders.sort_by_key(|o| if o.order_type.is_cancel() { 0 } else { 1 });

        let cancel_count = orders.iter().filter(|o| o.order_type.is_cancel()).count();
        let placement_count = orders.len() - cancel_count;

        debug!(
            signal_id = %intent.signal_id,
            mint = %intent.mint,
            cancels = cancel_count,
            placements = placement_count,
            "Reconciliation complete"
        );

        Ok(orders)
    }

    /// Reconcile a single side (bids or asks) of the order book.
    fn reconcile_side(
        &self,
        intent: &OrderIntent,
        current: &[&PendingLimitOrder],
        desired: &[QuoteLevel],
        side: OrderSide,
    ) -> Result<Vec<Order>, PositionError> {
        let mut actions = vec![];

        // Find orders to cancel: current orders not matching any desired level
        for order in current {
            if !self.has_matching_level(order, desired) {
                actions.push(self.create_cancel_order(intent, &order.order_id));
            }
        }

        // Find orders to place: desired levels not matching any current order
        for level in desired {
            if !self.has_matching_order(level, current) {
                actions.push(self.create_limit_order(intent, level, side)?);
            }
        }

        Ok(actions)
    }

    /// Check if a pending order matches any desired level.
    fn has_matching_level(&self, order: &PendingLimitOrder, desired: &[QuoteLevel]) -> bool {
        desired.iter().any(|d| order.matches_level(d.price, d.size))
    }

    /// Check if a desired level matches any current pending order.
    fn has_matching_order(&self, level: &QuoteLevel, current: &[&PendingLimitOrder]) -> bool {
        current
            .iter()
            .any(|c| c.matches_level(level.price, level.size))
    }

    /// Create a cancel order for the given order_id.
    fn create_cancel_order(&self, intent: &OrderIntent, order_id: &str) -> Order {
        Order::new_cancel(
            intent.mint.clone(),
            intent.market.clone(),
            order_id.to_string(),
            intent.timestamp,
            Some(intent.signal_id.clone()),
        )
    }

    /// Create a limit order from a desired QuoteLevel.
    ///
    /// Orders from intent-based signals use StrategyManaged exit mode since
    /// market making strategies don't use position-based exit logic.
    fn create_limit_order(
        &self,
        intent: &OrderIntent,
        level: &QuoteLevel,
        side: OrderSide,
    ) -> Result<Order, PositionError> {
        let mut order = match side {
            OrderSide::Buy => Order::new_limit_buy(
                intent.mint.clone(),
                intent.market.clone(),
                level.price * level.size, // quote_amount = price * size
                level.price,
                level.time_in_force,
                intent.timestamp,
                Some(intent.signal_id.clone()),
                None, // signal_slot not available from intent
            )
            .with_exit_mode(ExitMode::StrategyManaged),
            OrderSide::Sell => Order::new_limit_sell(
                intent.mint.clone(),
                intent.market.clone(),
                level.size,
                level.price,
                false, // clear_position - let position manager decide based on size
                level.time_in_force,
                intent.timestamp,
                Some(intent.signal_id.clone()),
                None, // signal_slot not available from intent
            )
            .with_exit_mode(ExitMode::StrategyManaged),
        };

        // Propagate context from intent to order for debugging/analysis
        if let Some(ctx) = &intent.context {
            order = order.with_context(ctx.clone());
        }

        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::order::OrderType;
    use chrono::Utc;

    fn create_pending_order(
        order_id: &str,
        mint: &str,
        side: OrderSide,
        price: f64,
        size: f64,
    ) -> PendingLimitOrder {
        PendingLimitOrder::new(
            order_id.to_string(),
            mint.to_string(),
            Some("market1".to_string()),
            side,
            price,
            size,
            Utc::now(),
            None,
        )
    }

    #[test]
    fn test_empty_current_state_generates_placements() {
        let engine = ReconciliationEngine::new();
        let current_orders = HashMap::new();
        let timestamp = Utc::now();

        let intent = OrderIntent::new(
            "token123".to_string(),
            Some("market1".to_string()),
            Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
            Some(vec![QuoteLevel::gtc(0.55, 100.0)]),
            "signal1".to_string(),
            timestamp,
            None,
        );

        let orders = engine
            .compute_reconciliation(&intent, &current_orders)
            .unwrap();

        assert_eq!(orders.len(), 2);
        assert!(orders.iter().all(|o| o.order_type.is_limit()));
    }

    #[test]
    fn test_matching_state_generates_no_orders() {
        let engine = ReconciliationEngine::new();
        let mut current_orders = HashMap::new();
        current_orders.insert(
            "bid1".to_string(),
            create_pending_order("bid1", "token123", OrderSide::Buy, 0.45, 100.0),
        );
        current_orders.insert(
            "ask1".to_string(),
            create_pending_order("ask1", "token123", OrderSide::Sell, 0.55, 100.0),
        );

        let intent = OrderIntent::new(
            "token123".to_string(),
            Some("market1".to_string()),
            Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
            Some(vec![QuoteLevel::gtc(0.55, 100.0)]),
            "signal1".to_string(),
            Utc::now(),
            None,
        );

        let orders = engine
            .compute_reconciliation(&intent, &current_orders)
            .unwrap();

        assert!(orders.is_empty());
    }

    #[test]
    fn test_price_change_generates_cancel_and_placement() {
        let engine = ReconciliationEngine::new();
        let mut current_orders = HashMap::new();
        current_orders.insert(
            "bid1".to_string(),
            create_pending_order("bid1", "token123", OrderSide::Buy, 0.45, 100.0),
        );

        let intent = OrderIntent::new(
            "token123".to_string(),
            Some("market1".to_string()),
            Some(vec![QuoteLevel::gtc(0.46, 100.0)]), // Different price
            None,
            "signal1".to_string(),
            Utc::now(),
            None,
        );

        let orders = engine
            .compute_reconciliation(&intent, &current_orders)
            .unwrap();

        assert_eq!(orders.len(), 2);
        // Cancel comes first
        assert!(orders[0].order_type.is_cancel());
        assert_eq!(orders[0].order_type.cancel_order_id(), Some("bid1"));
        // Placement second
        assert!(orders[1].order_type.is_limit());
        assert!(
            matches!(&orders[1].order_type, OrderType::LimitBuy { limit_price, .. } if (*limit_price - 0.46).abs() < 1e-9)
        );
    }

    #[test]
    fn test_cancel_all_bids() {
        let engine = ReconciliationEngine::new();
        let mut current_orders = HashMap::new();
        current_orders.insert(
            "bid1".to_string(),
            create_pending_order("bid1", "token123", OrderSide::Buy, 0.45, 100.0),
        );
        current_orders.insert(
            "bid2".to_string(),
            create_pending_order("bid2", "token123", OrderSide::Buy, 0.44, 50.0),
        );

        let intent = OrderIntent::new(
            "token123".to_string(),
            Some("market1".to_string()),
            Some(vec![]), // Cancel all bids
            None,
            "signal1".to_string(),
            Utc::now(),
            None,
        );

        let orders = engine
            .compute_reconciliation(&intent, &current_orders)
            .unwrap();

        assert_eq!(orders.len(), 2);
        assert!(orders.iter().all(|o| o.order_type.is_cancel()));
    }

    #[test]
    fn test_none_preserves_existing() {
        let engine = ReconciliationEngine::new();
        let mut current_orders = HashMap::new();
        current_orders.insert(
            "bid1".to_string(),
            create_pending_order("bid1", "token123", OrderSide::Buy, 0.45, 100.0),
        );

        let intent = OrderIntent::new(
            "token123".to_string(),
            Some("market1".to_string()),
            None,         // Preserve existing bids
            Some(vec![]), // Cancel all asks (none exist)
            "signal1".to_string(),
            Utc::now(),
            None,
        );

        let orders = engine
            .compute_reconciliation(&intent, &current_orders)
            .unwrap();

        // Should generate no orders
        assert!(orders.is_empty());
    }

    #[test]
    fn test_context_propagated_to_orders() {
        let engine = ReconciliationEngine::new();
        let current_orders = HashMap::new();
        let timestamp = Utc::now();
        let context = serde_json::json!({
            "fair_price": 0.50,
            "inventory": 100,
            "spread": 0.02
        });

        let intent = OrderIntent::new(
            "token123".to_string(),
            Some("market1".to_string()),
            Some(vec![QuoteLevel::gtc(0.45, 100.0)]),
            Some(vec![QuoteLevel::gtc(0.55, 100.0)]),
            "signal1".to_string(),
            timestamp,
            Some(context.clone()),
        );

        let orders = engine
            .compute_reconciliation(&intent, &current_orders)
            .unwrap();

        assert_eq!(orders.len(), 2);
        // Both orders should have context propagated
        for order in &orders {
            assert!(order.context.is_some());
            assert_eq!(order.context.as_ref().unwrap(), &context);
        }
    }
}
