use anyhow::Result;
use async_trait::async_trait;

use crate::{domain::SystemEvent, event_coordinator::EventCoordinator};

pub struct NoopEventCoordinator {}

impl NoopEventCoordinator {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl EventCoordinator for NoopEventCoordinator {
    async fn enqueue_event(&self, _event: SystemEvent) -> Result<()> {
        // Noop implementation simply discards any enqueued events
        Ok(())
    }

    async fn next_event(&self) -> Result<SystemEvent> {
        Err(anyhow::anyhow!("No more events"))
    }
}
