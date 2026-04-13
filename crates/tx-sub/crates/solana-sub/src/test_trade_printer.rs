use crate::parser::{MarketTrade, TradeLog};
use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::broadcast;
use tracing::{error, info};

pub struct TestTradePrinter {
    trade_receiver: broadcast::Receiver<MarketTrade>,
    event_count: Arc<AtomicUsize>,
    target_count: usize,
    print_format: String,
    shutdown_tx: broadcast::Sender<()>,
}

impl TestTradePrinter {
    pub fn new(
        trade_receiver: broadcast::Receiver<MarketTrade>,
        target_count: usize,
        print_format: String,
        shutdown_tx: broadcast::Sender<()>,
    ) -> Self {
        Self {
            trade_receiver,
            event_count: Arc::new(AtomicUsize::new(0)),
            target_count,
            print_format,
            shutdown_tx,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!(
            "Test trade printer started, waiting for {} Bonk events",
            self.target_count
        );

        loop {
            match self.trade_receiver.recv().await {
                Ok(trade) => {
                    // Only count and print Bonk trades
                    if let TradeLog::Bonk(_) = &trade.log {
                        let count = self.event_count.fetch_add(1, Ordering::Relaxed) + 1;

                        self.print_trade(&trade, count)?;

                        if count >= self.target_count {
                            info!(
                                "Reached target of {} Bonk events, shutting down",
                                self.target_count
                            );
                            let _ = self.shutdown_tx.send(());
                            break;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    error!("Trade printer lagged, skipped {} messages", skipped);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("Trade channel closed");
                    break;
                }
            }
        }

        Ok(())
    }

    fn print_trade(&self, trade: &MarketTrade, count: usize) -> Result<()> {
        match self.print_format.as_str() {
            "json" => {
                // Convert to token event for JSON serialization
                let token_event = trade.to_token_event();
                let json = serde_json::to_string_pretty(&token_event)?;
                println!("=== Bonk Trade Event #{} ===", count);
                println!("{}", json);
                println!();
            }
            "pretty" | _ => {
                println!("=== Bonk Trade Event #{} ===", count);
                println!("Slot: {}", trade.slot);
                println!("Signature: {}", trade.signature);

                if let TradeLog::Bonk(bonk_trade) = &trade.log {
                    println!("Direction: {:?}", bonk_trade.direction);
                    println!("Pool Address: {}", bonk_trade.pool_address);
                    println!("Base Mint: {}", bonk_trade.base_mint);
                    println!("Quote Mint: {}", bonk_trade.quote_mint);
                    println!("Base Amount: {}", bonk_trade.base_amount);
                    println!("Quote Amount: {}", bonk_trade.quote_amount);
                    println!("User: {}", bonk_trade.user);
                }
                println!();
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::bonk::{BonkSwap, TradeDirection};

    #[tokio::test]
    async fn test_trade_printer() {
        let (trade_tx, trade_rx) = broadcast::channel(100);
        let (shutdown_tx, _) = broadcast::channel(1);

        let mut printer = TestTradePrinter::new(trade_rx, 1, "pretty".to_string(), shutdown_tx);

        // Send a test trade
        let test_trade = MarketTrade {
            slot: 12345,
            signature: "test_sig".to_string(),
            log: TradeLog::Bonk(BonkSwap {
                direction: TradeDirection::Buy,
                pool_address: "BonkPool123".to_string(),
                base_mint: "BonkMint123".to_string(),
                quote_mint: "So11111111111111111111111111111111111111112".to_string(),
                base_amount: 5000000000,
                quote_amount: 1000000000,
                user: "UserWallet123".to_string(),
            }),
        };

        let _ = trade_tx.send(test_trade);
        drop(trade_tx);

        // Run printer (should exit after 1 event)
        let result = printer.run().await;
        assert!(result.is_ok());
    }
}
