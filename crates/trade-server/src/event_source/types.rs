use anyhow::Result;
use async_trait::async_trait;

use crate::domain::SystemEvent;

#[async_trait]
pub trait EventSource: Send + Sync {
    /// Non-blocking check for the next event.
    /// Returns None immediately if no event is currently available.
    async fn try_next_event(&self) -> Result<Option<SystemEvent>>;

    /// Blocking call that waits until an event is available.
    /// This will not return None unless there's an error or the source is closed.
    async fn next_event(&self) -> Result<Option<SystemEvent>>;
}
