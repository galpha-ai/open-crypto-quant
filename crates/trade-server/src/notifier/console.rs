use anyhow::Result;
use async_trait::async_trait;

use super::{Notifiable, NotificationRegistry, Notifier};

#[derive(Clone)]
pub struct ConsoleNotifier {
    registry: NotificationRegistry,
}

impl ConsoleNotifier {
    pub fn new(registry: NotificationRegistry) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl Notifier for ConsoleNotifier {
    async fn notify(&self, notification: &(dyn Notifiable + Sync)) -> Result<()> {
        // Get config for this notification type
        if let Some(config) = self.registry.get_config(notification.notification_type()) {
            if config.enabled {
                println!(
                    "Notification Type: {}\n{}",
                    notification.notification_type(),
                    notification.to_json()?
                );
            }
        }
        Ok(())
    }
}
