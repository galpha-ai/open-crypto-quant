//! Exit management module.
//!
//! This module contains logic for managing position exits.
//!
//! Note: The `exceeds_safety_limits()` function was removed because:
//! 1. For Automatic mode: `ConfigurableExitStrategy` already has user-configurable stop-loss
//! 2. For StrategyManaged mode: Position-based PnL thresholds don't make sense for market making
//!    strategies where profit is tracked via cash flows, not position mark-to-market
