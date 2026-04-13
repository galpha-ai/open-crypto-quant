use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::{api::ApiHandler, position::PositionManager};

pub struct CoreApiHandler {
    position_manager: Arc<dyn PositionManager>,
}

impl CoreApiHandler {
    pub fn new(position_manager: Arc<dyn PositionManager>) -> Self {
        Self { position_manager }
    }
}

#[async_trait]
impl ApiHandler for CoreApiHandler {
    fn method_prefix(&self) -> &str {
        "core"
    }

    async fn handle_request(&self, method: &str, _params: Option<Value>) -> Result<Value> {
        match method {
            "core_getBalance" => {
                let available_quote = self.position_manager.get_available_quote().await;
                let total_quote_received = self.position_manager.get_total_quote_received().await;
                let total_quote_spent = self.position_manager.get_total_quote_spent().await;
                let net_cash_flow = self.position_manager.get_net_cash_flow().await;
                let total_pnl = self.position_manager.get_total_pnl().await;

                Ok(json!({
                    "available_quote": available_quote,
                    "total_quote_received": total_quote_received,
                    "total_quote_spent": total_quote_spent,
                    "net_cash_flow": net_cash_flow,
                    "total_pnl": total_pnl
                }))
            }
            "core_getPositions" => {
                let positions = self.position_manager.get_all_open_positions().await;
                Ok(json!({
                    "open_positions": positions,
                    "count": positions.len()
                }))
            }
            "core_getStats" => {
                let total_closed = self.position_manager.get_total_closed_positions().await;
                let winning_trades = self.position_manager.get_winning_trades().await;
                let open_count = self.position_manager.get_open_position_count().await;
                let net_cash_flow = self.position_manager.get_net_cash_flow().await;
                let total_unrealized_pnl = self.position_manager.get_total_unrealized_pnl().await;
                let total_pnl = self.position_manager.get_total_pnl().await;

                let win_rate = if total_closed > 0 {
                    (winning_trades as f64 / total_closed as f64) * 100.0
                } else {
                    0.0
                };

                Ok(json!({
                    "total_closed_positions": total_closed,
                    "winning_trades": winning_trades,
                    "open_position_count": open_count,
                    "net_cash_flow": net_cash_flow,
                    "total_unrealized_pnl": total_unrealized_pnl,
                    "total_pnl": total_pnl,
                    "win_rate_pct": win_rate
                }))
            }
            _ => Err(anyhow::anyhow!("Unknown method: {}", method)),
        }
    }

    fn list_methods(&self) -> Vec<String> {
        vec![
            "core_getBalance".to_string(),
            "core_getPositions".to_string(),
            "core_getStats".to_string(),
        ]
    }
}
