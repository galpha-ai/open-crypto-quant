use anyhow::Result;
use async_trait::async_trait;
use teloxide::{Bot, payloads::SendMessageSetters, prelude::Requester, types::ParseMode};
use tracing::{debug, info, instrument};

use super::{Notifiable, NotificationRegistry, Notifier};

#[derive(Clone)]
pub struct TelegramNotifier {
    registry: NotificationRegistry,
    bot: Bot,
}

impl TelegramNotifier {
    pub fn new(registry: NotificationRegistry, token: &str) -> Self {
        tracing::info!("Creating new TelegramNotifier");
        Self {
            bot: Bot::new(token),
            registry,
        }
    }
}

#[async_trait]
impl Notifier for TelegramNotifier {
    #[instrument(skip(self, notification), fields(notification_type = notification.notification_type()))]
    async fn notify(&self, notification: &(dyn Notifiable + Sync)) -> Result<()> {
        // Get config for this notification type
        if let Some(config) = self.registry.get_config(notification.notification_type()) {
            if config.enabled && !config.telegram_chat_ids.is_empty() {
                let message = notification.to_html()?;
                info!(
                    chat_ids = ?config.telegram_chat_ids,
                    "Sending Telegram notification"
                );

                for chat_id in &config.telegram_chat_ids {
                    debug!(chat_id, "Sending to chat");
                    // Spawn a task for each message to avoid blocking
                    tokio::spawn({
                        let bot = self.bot.clone();
                        let chat_id = chat_id.clone();
                        let message = message.clone();
                        async move {
                            if let Err(e) = bot
                                .send_message(chat_id, message)
                                .parse_mode(ParseMode::Html)
                                .await
                            {
                                tracing::error!("Failed to send Telegram message: {}", e);
                            }
                        }
                    });
                }
            } else {
                info!("Telegram notifications disabled or no chat IDs configured for this type");
            }
        } else {
            info!("No notification config found for this type");
        }

        Ok(())
    }
}
