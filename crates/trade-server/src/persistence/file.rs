//! File-based persistence for backtest signal analysis.

use async_trait::async_trait;
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use super::TradingEventPersistence;

/// File-based persistence for backtest analysis.
/// Writes signals and position events to JSONL files.
pub struct FileTradingEventPersistence {
    signal_file: Mutex<BufWriter<std::fs::File>>,
    position_file: Mutex<BufWriter<std::fs::File>>,
}

impl FileTradingEventPersistence {
    /// Create a new file-based persistence.
    pub fn new(signal_path: PathBuf, position_path: PathBuf) -> anyhow::Result<Self> {
        let signal_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(signal_path)?;

        let position_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(position_path)?;

        Ok(Self {
            signal_file: Mutex::new(BufWriter::new(signal_file)),
            position_file: Mutex::new(BufWriter::new(position_file)),
        })
    }
}

#[async_trait]
impl TradingEventPersistence for FileTradingEventPersistence {
    async fn persist_signal(&self, signal_json: Value) -> anyhow::Result<()> {
        let json_str = serde_json::to_string(&signal_json)?;
        let mut writer = self.signal_file.lock().unwrap();
        writeln!(writer, "{}", json_str)?;
        writer.flush()?;
        Ok(())
    }

    async fn persist_position_closed(&self, event_json: Value) -> anyhow::Result<()> {
        let json_str = serde_json::to_string(&event_json)?;
        let mut writer = self.position_file.lock().unwrap();
        writeln!(writer, "{}", json_str)?;
        writer.flush()?;
        Ok(())
    }
}
