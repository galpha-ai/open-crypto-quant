use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
};

use popeyes_trading_types::TokenTradeEvent;
use solana_sdk::signature::Signature;
use tokio::sync::oneshot;
use tracing::{debug, info, warn};

/// Monitors a gRPC stream (implicitly) for transaction confirmations
/// and notifies waiting tasks.
#[derive(Clone, Debug)]
pub struct GrpcConfirmationMonitor {
    // Map Signature to a channel sender to notify when confirmed via gRPC
    pending_confirmations: Arc<Mutex<HashMap<Signature, oneshot::Sender<()>>>>,
}

impl GrpcConfirmationMonitor {
    pub fn new() -> Self {
        info!("Creating GrpcConfirmationMonitor");
        Self {
            pending_confirmations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Registers a signature and returns a receiver to wait on.
    /// The caller should await the receiver.
    /// Registers a signature and returns a receiver to wait on.
    /// The caller should await the receiver.
    pub fn register_confirmation_channel(&self, signature: Signature) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel::<()>();
        let mut pending = self.pending_confirmations.lock().unwrap();
        if pending.insert(signature, tx).is_some() {
            // This indicates we are trying to register the same signature twice,
            // which might signify a logic error elsewhere (e.g., submitting the same tx twice).
            warn!(%signature, "Attempted to re-register confirmation channel for the same signature. Overwriting.");
        }
        debug!(%signature, "Registered confirmation channel with gRPC monitor");
        rx
    }

    /// Removes a signature registration, typically called when confirmation
    /// happens via other means (e.g., polling) or on timeout.
    pub fn deregister_confirmation_channel(&self, signature: Signature) {
        let mut pending = self.pending_confirmations.lock().unwrap();
        if pending.remove(&signature).is_some() {
            debug!(%signature, "Deregistered confirmation channel from gRPC monitor");
        } else {
            // This might happen if the gRPC event already triggered removal. Benign.
            debug!(%signature, "Attempted to deregister signature not found in gRPC monitor (likely already confirmed/removed)");
        }
    }

    /// Handles an incoming trade event from a gRPC stream.
    /// If the event's signature matches a pending confirmation, it notifies the waiter.
    pub async fn handle_token_trade(&self, trade: &TokenTradeEvent) {
        // Only process events that have a base event (i.e., not Polymarket)
        let Some(base) = trade.base() else {
            return;
        };

        if let Ok(event_sig) = Signature::from_str(&base.signature) {
            let sender = {
                // Lock, remove, and get the sender in one scope
                let mut pending = self.pending_confirmations.lock().unwrap();
                pending.remove(&event_sig) // Remove returns Option<V>
            };

            if let Some(sender) = sender {
                info!(signature = %event_sig, "Matching gRPC event found for pending tx. Notifying waiter.");
                // Send the notification. Ignore error if the receiver was dropped
                // (e.g., the waiting task timed out or confirmed via polling).
                let _ = sender.send(());
            }
            // If no sender was found, it means either:
            // 1. The signature wasn't registered (not a tx we submitted or are waiting for).
            // 2. It was already confirmed/deregistered (via polling or timeout).
            // Both are normal scenarios, no warning needed unless debugging specific issues.
            else {
                // Optional trace log for debugging missed confirmations
                tracing::trace!(signature = %event_sig, "Received gRPC event for signature not actively monitored or already handled.");
            }
        } else {
            tracing::warn!(
                signature = base.signature,
                "Failed to parse signature from TokenTradeEvent"
            );
        }
    }
}

impl Default for GrpcConfirmationMonitor {
    fn default() -> Self {
        Self::new()
    }
}
