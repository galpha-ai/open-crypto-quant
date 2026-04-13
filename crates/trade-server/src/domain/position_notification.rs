use anyhow::Result;
use serde::Serialize;

use crate::{notifier, notifier::Notifiable, utils::serialize_duration_as_secs};

#[derive(Clone, Serialize, Debug)]
pub struct PositionCreatedNotification {
    pub mint: String,
    pub amount: f64,
    pub entry_price: Option<f64>,
    pub entry_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Serialize)]
pub struct PositionClosedNotification {
    pub mint: String,
    pub realized_pnl_sol: Option<f64>,
    pub pnl_pct: Option<f64>,
    #[serde(serialize_with = "serialize_duration_as_secs")]
    pub holding_period: chrono::Duration,
    pub exit_price: Option<f64>,
    pub current_quote_balance: f64,
    pub net_cash_flow: f64,
    pub total_closed_positions: u32,
    pub win_rate_pct: f64,
    pub avg_pnl_per_trade: f64,
}

impl Notifiable for PositionCreatedNotification {
    fn notification_type(&self) -> &str {
        notifier::POSITION_NOTIFICATION
    }

    fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    fn to_html(&self) -> Result<String> {
        Ok(format!(
            "<b>📈 Position Created!</b>\n\n\
            <b>Token Mint:</b> {}\n\
            <b>Amount:</b> {:.2}\n\
            <b>Entry Price:</b> {}\n\
            <b>Entry Time:</b> {} UTC",
            self.mint,
            self.amount,
            self.entry_price
                .map_or("N/A".to_string(), |p| format!("${:.8}", p)),
            self.entry_time.format("%Y-%m-%d %H:%M:%S")
        ))
    }
}

impl Notifiable for PositionClosedNotification {
    fn notification_type(&self) -> &str {
        notifier::POSITION_NOTIFICATION
    }

    fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    fn to_html(&self) -> Result<String> {
        Ok(format!(
            "<b>Position Closed!</b>\n\n\
            <b>Token Mint:</b> {}\n\
            <b>Realized PnL:</b> {}\n\
            <b>PnL %:</b> {}\n\
            <b>Holding Period:</b> {:.1} seconds\n\
            <b>Exit Price:</b> {}\n\n\
            <b>Strategy Performance:</b>\n\
            Quote Balance: {:.4}\n\
            Net Cash Flow: {:.4}\n\
            Total Trades: {}\n\
            Win Rate: {:.1}%\n\
            Avg PnL/Trade: {:.4}",
            self.mint,
            self.realized_pnl_sol
                .map_or("N/A".to_string(), |p| format!("{:.4}", p)),
            self.pnl_pct
                .map_or("N/A".to_string(), |p| format!("{:.2}%", p)),
            self.holding_period.num_seconds() as f64,
            self.exit_price
                .map_or("N/A".to_string(), |p| format!("${:.8}", p)),
            self.current_quote_balance,
            self.net_cash_flow,
            self.total_closed_positions,
            self.win_rate_pct,
            self.avg_pnl_per_trade
        ))
    }
}
