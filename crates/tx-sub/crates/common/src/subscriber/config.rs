use serde::Deserialize;

/// Configuration for a Redis list subscriber using BRPOP.
#[derive(Debug, Deserialize, Clone)]
pub struct ListSubscriberConfig {
    /// Queue name to subscribe to
    pub name: String,
    /// Timeout in seconds for BRPOP (0 = block forever)
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Configuration for a Redis stream subscriber using XREADGROUP.
#[derive(Debug, Deserialize, Clone)]
pub struct StreamSubscriberConfig {
    /// Stream name to subscribe to
    pub name: String,
    /// Consumer group name
    pub consumer_group: String,
    /// Consumer name within the group (defaults to hostname-pid)
    #[serde(default)]
    pub consumer_name: Option<String>,
    /// Block timeout in milliseconds (default: 5000)
    #[serde(default)]
    pub block_ms: Option<u64>,
    /// Maximum number of entries to read at once (default: 10)
    #[serde(default)]
    pub count: Option<usize>,
}

/// Configuration for a Redis pubsub subscriber.
#[derive(Debug, Deserialize, Clone)]
pub struct PubsubSubscriberConfig {
    /// Channels to subscribe to
    pub channels: Vec<String>,
}

impl Default for ListSubscriberConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            timeout_secs: Some(0),
        }
    }
}

impl StreamSubscriberConfig {
    /// Get the block timeout in milliseconds, defaulting to 5000.
    pub fn block_ms(&self) -> u64 {
        self.block_ms.unwrap_or(5000)
    }

    /// Get the count of entries to read, defaulting to 10.
    pub fn count(&self) -> usize {
        self.count.unwrap_or(10)
    }

    /// Get the consumer name, generating a default if not specified.
    pub fn consumer_name(&self) -> String {
        self.consumer_name.clone().unwrap_or_else(|| {
            let hostname = hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string());
            let pid = std::process::id();
            format!("{}-{}", hostname, pid)
        })
    }
}
