//! Position manager state module.
//!
//! This module contains the core state structure for `InMemoryPositionManager`
//! and basic accessors for position and pending order data.

// Allow dead code until the refactor is complete - these accessors will be used
// as more handler modules are extracted from in_mem_manager.rs
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use crate::execution::OrderSide;
use crate::position::Position;
use crate::position::pending_order::{InFlightOrder, PendingLimitOrder};
use crate::signal::{QuoteLevel, RedemptionPolicy};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IntentLaneKey {
    pub market: Option<String>,
    pub mint: String,
    pub side: OrderSide,
}

#[derive(Debug, Clone, Default)]
pub struct IntentDebounceState {
    pub pending_levels: Option<Vec<QuoteLevel>>,
}

/// Internal state for the in-memory position manager.
///
/// This struct holds all the mutable state that the position manager tracks,
/// including open positions, pending orders, and aggregate statistics.
#[derive(Debug)]
pub struct PositionManagerState {
    /// Active positions indexed by mint address
    pub positions: HashMap<String, Position>,

    /// Available quote currency balance for trading
    pub available_quote: f64,

    /// Set of mints currently awaiting sell confirmation
    pub pending_sells: HashSet<String>,

    /// Set of all mints we've ever bought (prevents repeat buys)
    pub bought_mints: HashSet<String>,

    /// Cumulative quote currency received from all sells
    pub total_quote_received: f64,

    /// Cumulative quote currency spent on all buys
    pub total_quote_spent: f64,

    /// Total number of closed positions
    pub total_closed_positions: u32,

    /// Number of trades that closed with positive PnL
    pub winning_trades: u32,

    /// Pending limit orders by mint -> order_id -> order
    /// Used for intent-based reconciliation in orderbook trading
    pub pending_limit_orders: HashMap<String, HashMap<String, PendingLimitOrder>>,

    /// In-flight orders by mint -> in_flight_id -> order
    /// Tracks orders that have been submitted but not yet confirmed by venue.
    /// This prevents the race condition where duplicate orders are placed
    /// because pending_limit_orders is not updated until OrderPlaced arrives.
    pub in_flight_orders: HashMap<String, HashMap<String, InFlightOrder>>,

    /// Redemption policies by market ID
    /// Used for binary market pair redemption
    pub redemption_policies: HashMap<String, RedemptionPolicy>,

    /// Pending debounced quote updates by (market, mint, side).
    pub intent_debounce: HashMap<IntentLaneKey, IntentDebounceState>,
}

impl PositionManagerState {
    /// Create a new position manager state with the given initial quote balance.
    pub fn new(initial_quote: f64) -> Self {
        Self {
            positions: HashMap::new(),
            available_quote: initial_quote,
            pending_sells: HashSet::new(),
            bought_mints: HashSet::new(),
            total_quote_received: 0.0,
            total_quote_spent: 0.0,
            total_closed_positions: 0,
            winning_trades: 0,
            pending_limit_orders: HashMap::new(),
            in_flight_orders: HashMap::new(),
            redemption_policies: HashMap::new(),
            intent_debounce: HashMap::new(),
        }
    }

    // === Position accessors ===

    /// Get a position by mint address.
    pub fn get_position(&self, mint: &str) -> Option<&Position> {
        self.positions.get(mint)
    }

    /// Get a mutable reference to a position by mint address.
    pub fn get_position_mut(&mut self, mint: &str) -> Option<&mut Position> {
        self.positions.get_mut(mint)
    }

    /// Insert or update a position.
    pub fn set_position(&mut self, mint: String, position: Position) {
        self.positions.insert(mint, position);
    }

    /// Remove a position by mint address.
    pub fn remove_position(&mut self, mint: &str) -> Option<Position> {
        self.positions.remove(mint)
    }

    /// Get the number of open positions.
    pub fn position_count(&self) -> usize {
        self.positions.len()
    }

    /// Get all positions as a vector.
    pub fn all_positions(&self) -> Vec<Position> {
        self.positions.values().cloned().collect()
    }

    // === Pending sells ===

    /// Check if a mint has a pending sell.
    pub fn has_pending_sell(&self, mint: &str) -> bool {
        self.pending_sells.contains(mint)
    }

    /// Mark a mint as having a pending sell.
    /// Returns true if successfully marked, false if already pending.
    pub fn mark_pending_sell(&mut self, mint: &str) -> bool {
        if self.pending_sells.contains(mint) {
            false
        } else {
            self.pending_sells.insert(mint.to_string());
            true
        }
    }

    /// Clear the pending sell flag for a mint.
    /// Returns true if the flag was set and is now cleared.
    pub fn clear_pending_sell(&mut self, mint: &str) -> bool {
        self.pending_sells.remove(mint)
    }

    // === Bought mints tracking ===

    /// Check if we've ever bought a mint.
    pub fn has_bought(&self, mint: &str) -> bool {
        self.bought_mints.contains(mint)
    }

    /// Mark a mint as bought.
    /// Returns true if this is a new purchase, false if already bought.
    pub fn mark_bought(&mut self, mint: &str) -> bool {
        if self.bought_mints.contains(mint) {
            false
        } else {
            self.bought_mints.insert(mint.to_string());
            true
        }
    }

    // === Pending limit orders ===

    /// Get pending limit orders for a mint.
    pub fn get_pending_orders(&self, mint: &str) -> Option<&HashMap<String, PendingLimitOrder>> {
        self.pending_limit_orders.get(mint)
    }

    /// Get pending limit orders for a mint (mutable).
    pub fn get_pending_orders_mut(
        &mut self,
        mint: &str,
    ) -> Option<&mut HashMap<String, PendingLimitOrder>> {
        self.pending_limit_orders.get_mut(mint)
    }

    /// Get or create the pending orders map for a mint.
    pub fn pending_orders_entry(
        &mut self,
        mint: String,
    ) -> &mut HashMap<String, PendingLimitOrder> {
        self.pending_limit_orders.entry(mint).or_default()
    }

    /// Add a pending limit order.
    pub fn add_pending_order(&mut self, mint: String, order_id: String, order: PendingLimitOrder) {
        let orders = self.pending_limit_orders.entry(mint).or_default();
        orders.insert(order_id, order);
    }

    /// Remove a pending order by order_id, searching across all mints.
    /// Returns the removed order if found.
    pub fn remove_pending_order_by_id(&mut self, order_id: &str) -> Option<PendingLimitOrder> {
        for orders in self.pending_limit_orders.values_mut() {
            if let Some(order) = orders.remove(order_id) {
                return Some(order);
            }
        }
        None
    }

    /// Get a cloned copy of pending orders for a mint.
    pub fn clone_pending_orders(&self, mint: &str) -> HashMap<String, PendingLimitOrder> {
        self.pending_limit_orders
            .get(mint)
            .cloned()
            .unwrap_or_default()
    }

    // === In-flight orders ===

    /// Add an in-flight order.
    pub fn add_in_flight_order(&mut self, mint: String, id: String, order: InFlightOrder) {
        let orders = self.in_flight_orders.entry(mint).or_default();
        orders.insert(id, order);
    }

    /// Remove an in-flight order by ID.
    /// Returns the removed order if found.
    pub fn remove_in_flight_order(&mut self, mint: &str, id: &str) -> Option<InFlightOrder> {
        self.in_flight_orders
            .get_mut(mint)
            .and_then(|orders| orders.remove(id))
    }

    /// Remove an in-flight order by matching price and size.
    /// Used when we receive OrderPlaced and need to match it to an in-flight order.
    /// Returns the removed order if found.
    pub fn remove_in_flight_order_by_level(
        &mut self,
        mint: &str,
        side: crate::execution::OrderSide,
        price: f64,
        size: f64,
    ) -> Option<InFlightOrder> {
        if let Some(orders) = self.in_flight_orders.get_mut(mint) {
            // Find an in-flight order matching the price/size/side
            let matching_id = orders
                .iter()
                .find(|(_, o)| o.side == side && o.matches_level(price, size))
                .map(|(id, _)| id.clone());

            if let Some(id) = matching_id {
                return orders.remove(&id);
            }
        }
        None
    }

    /// Get in-flight orders for a mint.
    pub fn get_in_flight_orders(&self, mint: &str) -> Option<&HashMap<String, InFlightOrder>> {
        self.in_flight_orders.get(mint)
    }

    /// Get a cloned copy of in-flight orders for a mint.
    pub fn clone_in_flight_orders(&self, mint: &str) -> HashMap<String, InFlightOrder> {
        self.in_flight_orders.get(mint).cloned().unwrap_or_default()
    }

    // === Redemption policies ===

    /// Set or update a redemption policy for a market.
    pub fn set_redemption_policy(&mut self, policy: RedemptionPolicy) {
        self.redemption_policies
            .insert(policy.market.clone(), policy);
    }

    /// Get a redemption policy by market ID.
    pub fn get_redemption_policy(&self, market: &str) -> Option<&RedemptionPolicy> {
        self.redemption_policies.get(market)
    }

    /// Get all redemption policies.
    pub fn all_redemption_policies(&self) -> impl Iterator<Item = &RedemptionPolicy> {
        self.redemption_policies.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::OrderSide;
    use chrono::Utc;

    #[test]
    fn test_new_state() {
        let state = PositionManagerState::new(10.0);
        assert_eq!(state.available_quote, 10.0);
        assert!(state.positions.is_empty());
        assert!(state.pending_sells.is_empty());
        assert!(state.bought_mints.is_empty());
        assert_eq!(state.total_quote_received, 0.0);
        assert_eq!(state.total_quote_spent, 0.0);
        assert_eq!(state.total_closed_positions, 0);
        assert_eq!(state.winning_trades, 0);
        assert!(state.pending_limit_orders.is_empty());
        assert!(state.in_flight_orders.is_empty());
        assert!(state.redemption_policies.is_empty());
    }

    #[test]
    fn test_pending_sell_operations() {
        let mut state = PositionManagerState::new(10.0);

        // First mark should succeed
        assert!(state.mark_pending_sell("mint1"));
        assert!(state.has_pending_sell("mint1"));

        // Second mark should fail
        assert!(!state.mark_pending_sell("mint1"));

        // Clear should succeed
        assert!(state.clear_pending_sell("mint1"));
        assert!(!state.has_pending_sell("mint1"));

        // Clear again should return false
        assert!(!state.clear_pending_sell("mint1"));
    }

    #[test]
    fn test_bought_mints_tracking() {
        let mut state = PositionManagerState::new(10.0);

        assert!(!state.has_bought("mint1"));
        assert!(state.mark_bought("mint1"));
        assert!(state.has_bought("mint1"));
        assert!(!state.mark_bought("mint1")); // Already bought
    }

    // === In-flight order tests ===

    fn create_test_in_flight_order(
        id: &str,
        mint: &str,
        side: OrderSide,
        price: f64,
        size: f64,
    ) -> InFlightOrder {
        InFlightOrder::new(
            id.to_string(),
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
    fn test_add_and_get_in_flight_orders() {
        let mut state = PositionManagerState::new(10.0);

        let order = create_test_in_flight_order("flight1", "token1", OrderSide::Buy, 0.45, 100.0);
        state.add_in_flight_order("token1".to_string(), "flight1".to_string(), order.clone());

        let orders = state.get_in_flight_orders("token1");
        assert!(orders.is_some());
        assert_eq!(orders.unwrap().len(), 1);
        assert_eq!(orders.unwrap().get("flight1").unwrap().price, 0.45);
    }

    #[test]
    fn test_remove_in_flight_order_by_id() {
        let mut state = PositionManagerState::new(10.0);

        let order = create_test_in_flight_order("flight1", "token1", OrderSide::Buy, 0.45, 100.0);
        state.add_in_flight_order("token1".to_string(), "flight1".to_string(), order);

        // Remove by ID
        let removed = state.remove_in_flight_order("token1", "flight1");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, "flight1");

        // Should be empty now
        let orders = state.get_in_flight_orders("token1");
        assert!(orders.is_some());
        assert!(orders.unwrap().is_empty());

        // Removing again should return None
        let removed_again = state.remove_in_flight_order("token1", "flight1");
        assert!(removed_again.is_none());
    }

    #[test]
    fn test_remove_in_flight_order_by_level() {
        let mut state = PositionManagerState::new(10.0);

        let order1 = create_test_in_flight_order("flight1", "token1", OrderSide::Buy, 0.45, 100.0);
        let order2 = create_test_in_flight_order("flight2", "token1", OrderSide::Sell, 0.55, 100.0);
        state.add_in_flight_order("token1".to_string(), "flight1".to_string(), order1);
        state.add_in_flight_order("token1".to_string(), "flight2".to_string(), order2);

        // Remove by matching price/size/side
        let removed = state.remove_in_flight_order_by_level("token1", OrderSide::Buy, 0.45, 100.0);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, "flight1");

        // Buy order should be gone, sell order should remain
        let orders = state.get_in_flight_orders("token1");
        assert!(orders.is_some());
        assert_eq!(orders.unwrap().len(), 1);
        assert!(orders.unwrap().contains_key("flight2"));

        // Try to remove with wrong side - should not match
        let not_removed =
            state.remove_in_flight_order_by_level("token1", OrderSide::Buy, 0.55, 100.0);
        assert!(not_removed.is_none());

        // Remove the sell order
        let removed_sell =
            state.remove_in_flight_order_by_level("token1", OrderSide::Sell, 0.55, 100.0);
        assert!(removed_sell.is_some());
        assert_eq!(removed_sell.unwrap().id, "flight2");
    }

    #[test]
    fn test_clone_in_flight_orders() {
        let mut state = PositionManagerState::new(10.0);

        let order = create_test_in_flight_order("flight1", "token1", OrderSide::Buy, 0.45, 100.0);
        state.add_in_flight_order("token1".to_string(), "flight1".to_string(), order);

        // Clone should return a copy
        let cloned = state.clone_in_flight_orders("token1");
        assert_eq!(cloned.len(), 1);

        // Clone of non-existent mint should return empty
        let empty = state.clone_in_flight_orders("nonexistent");
        assert!(empty.is_empty());
    }
}
