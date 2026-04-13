use anyhow::Result;
use async_trait::async_trait;

use crate::{domain::SystemEvent, signal::TradableSignal};

#[async_trait]
pub trait SignalGenerator {
    async fn generate_signal(
        &mut self,
        event: &SystemEvent,
    ) -> Result<Vec<Box<dyn TradableSignal>>>;
}

pub struct NoopSignalGenerator;

#[async_trait]
impl SignalGenerator for NoopSignalGenerator {
    async fn generate_signal(
        &mut self,
        _event: &SystemEvent,
    ) -> Result<Vec<Box<dyn TradableSignal>>> {
        Ok(vec![])
    }
}
