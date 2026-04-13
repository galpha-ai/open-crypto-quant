use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub struct TimerEvent {
    pub timestamp: DateTime<Utc>,
}
