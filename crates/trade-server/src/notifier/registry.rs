use std::collections::HashMap;

// Notification configuration for a signal type
#[derive(Debug, Clone)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub telegram_chat_ids: Vec<String>,
}

// Registry to store notification configs for different signal types
#[derive(Debug, Default, Clone)]
pub struct NotificationRegistry {
    configs: HashMap<String, NotificationConfig>,
}

impl NotificationRegistry {
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }

    pub fn register_config(&mut self, signal_type: &str, config: NotificationConfig) {
        self.configs.insert(signal_type.to_string(), config);
    }

    pub fn get_config(&self, signal_type: &str) -> Option<&NotificationConfig> {
        self.configs.get(signal_type)
    }
}
