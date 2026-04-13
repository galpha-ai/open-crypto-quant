//! Polymarket client module for CTF operations
//!
//! This module provides functionality for interacting with Polymarket's
//! Conditional Tokens Framework (CTF) through a Safe wallet, including:
//! - Split: Convert USDC into YES/NO conditional tokens
//! - Merge: Convert YES/NO tokens back to USDC
//! - Redeem: Claim USDC from winning tokens after market resolution

pub mod abi;
pub mod constants;
pub mod encode;
pub mod safe_client;

#[cfg(test)]
mod safe_client_test;

pub use constants::*;
pub use encode::{OperationType, SafeTransaction};
pub use safe_client::{SafeClient, SafeClientConfig, TransactionConfirmation};
