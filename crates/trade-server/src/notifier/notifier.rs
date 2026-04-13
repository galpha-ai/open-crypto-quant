use anyhow::Result;
use async_trait::async_trait;

pub const POSITION_NOTIFICATION: &str = "position_notification";
pub const SIGNAL_NOTIFICATION: &str = "signal_notification";

pub trait Notifiable {
    fn notification_type(&self) -> &str;
    fn to_json(&self) -> Result<String>;
    fn to_html(&self) -> Result<String>;
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, notification: &(dyn Notifiable + Sync)) -> Result<()>;
}
