use std::sync::Arc;

use anyhow::Result;
use tracing;

use crate::{notifier::Notifier, signal::TradableSignal, trade_server::PositionHandler};

pub struct SignalProcessor {
    notifier: Arc<dyn Notifier>,
    position_handler: Arc<PositionHandler>,
}

impl SignalProcessor {
    /// Creates a new SignalProcessor.
    pub fn new(notifier: Arc<dyn Notifier>, position_handler: Arc<PositionHandler>) -> Self {
        Self {
            notifier,
            position_handler,
        }
    }

    /// Process a tradable signal.
    ///
    /// - Notifies via the notifier if the signal implements `Notifiable`.
    /// - Passes the signal to the `PositionHandler` for potential trading actions.
    pub async fn process(&self, signal: &(impl TradableSignal + Sync)) -> Result<()> {
        // 1. Notify if the signal is Notifiable
        if let Some(notifiable_signal) = signal.as_notifiable() {
            if let Err(e) = self.notifier.notify(notifiable_signal.as_ref()).await {
                // Log the error but continue processing the signal for trading logic
                tracing::error!("Failed to send notification for signal: {:?}", e);
            }
        }

        // 2. Handle trading logic via PositionHandler
        if let Err(e) = self.position_handler.handle_signal(signal).await {
            tracing::error!("Error handling signal in position handler: {:?}", e);
            // Propagate the error from the position handler
            return Err(e.into());
        }

        Ok(())
    }
}
