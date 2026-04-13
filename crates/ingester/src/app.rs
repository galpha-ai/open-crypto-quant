use crate::config::IngesterAppConfig;
use crate::error::Result;
use crate::models::ChRow;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const FLUSH_MAX_RETRIES: u32 = 10;
const FLUSH_BASE_DELAY_MS: u64 = 100;
const FLUSH_MAX_DELAY_MS: u64 = 30_000;

pub struct IngesterApp {
    config: IngesterAppConfig,
    event_rx: Option<mpsc::Receiver<(String, String)>>,
}

impl IngesterApp {
    pub fn new(config: IngesterAppConfig) -> Self {
        Self {
            config,
            event_rx: None,
        }
    }

    async fn next_event(&mut self) -> Result<Option<(String, String)>> {
        if let Some(rx) = &mut self.event_rx {
            Ok(rx.recv().await)
        } else {
            Ok(None)
        }
    }

    // Consume a single Redis queue and push to local channel (batch mode)
    async fn consume_redis_queue(
        queue_name: String,
        redis_url: String,
        tx: mpsc::Sender<(String, String)>,
        consumed_count: Arc<AtomicU64>,
        cancel: CancellationToken,
    ) -> Result<()> {
        const BATCH_SIZE: usize = 1000;

        tracing::info!(queue = %queue_name, batch_size = BATCH_SIZE, "Starting Redis queue consumer (batch mode)");

        let redis_client = redis::Client::open(redis_url)?;
        let mut redis_conn = redis::aio::ConnectionManager::new(redis_client).await?;

        let mut local_count: u64 = 0;
        let mut last_log_time = Instant::now();
        let mut fetch_total_ms: u64 = 0;
        let mut fetch_count: u64 = 0;

        loop {
            if cancel.is_cancelled() {
                tracing::info!(queue = %queue_name, "Shutdown signal received, stopping consumer");
                return Ok(());
            }

            let fetch_start = Instant::now();
            let result: Vec<String> = match redis::cmd("LPOP")
                .arg(&queue_name)
                .arg(BATCH_SIZE)
                .query_async(&mut redis_conn)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(error = ?e, queue = %queue_name, "Redis error, retrying");
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    continue;
                }
            };
            let fetch_elapsed = fetch_start.elapsed();
            fetch_total_ms += fetch_elapsed.as_millis() as u64;
            fetch_count += 1;

            if result.is_empty() {
                tokio::select! {
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {}
                    _ = cancel.cancelled() => {
                        tracing::info!(queue = %queue_name, "Shutdown signal received, stopping consumer");
                        return Ok(());
                    }
                }
                continue;
            }

            let batch_len = result.len();

            for json_str in result {
                if tx.send((queue_name.clone(), json_str)).await.is_err() {
                    tracing::error!(queue = %queue_name, "Local channel closed, exiting consumer");
                    return Ok(());
                }
            }

            local_count += batch_len as u64;
            consumed_count.fetch_add(batch_len as u64, Ordering::Relaxed);

            if last_log_time.elapsed().as_secs() >= 10 {
                let avg_fetch_ms = if fetch_count > 0 { fetch_total_ms / fetch_count } else { 0 };
                let avg_batch_size = if fetch_count > 0 { local_count / fetch_count } else { 0 };
                tracing::info!(
                    queue = %queue_name,
                    consumed = local_count,
                    fetches = fetch_count,
                    avg_fetch_ms = avg_fetch_ms,
                    avg_batch_size = avg_batch_size,
                    "Redis consumer stats (last 10s)"
                );
                local_count = 0;
                fetch_total_ms = 0;
                fetch_count = 0;
                last_log_time = Instant::now();
            }
        }
    }

    fn retry_delay(attempt: u32) -> Duration {
        let multiplier = 1u64 << attempt.saturating_sub(1).min(7);
        let delay_ms = FLUSH_BASE_DELAY_MS.saturating_mul(multiplier);
        Duration::from_millis(delay_ms.min(FLUSH_MAX_DELAY_MS))
    }

    async fn flush_batch_once(
        ch_client: &clickhouse::Client,
        table: &str,
        batch: &[ChRow],
    ) -> std::result::Result<(), clickhouse::error::Error> {
        let mut inserter = ch_client.insert(table)?;
        for row in batch {
            inserter.write(row).await?;
        }
        inserter.end().await?;
        Ok(())
    }

    /// Flush a batch with retry. Owns the data so it can run in a spawned task.
    async fn flush_batch_with_retry_owned(
        ch_client: clickhouse::Client,
        table: String,
        batch: Vec<ChRow>,
    ) {
        let batch_len = batch.len();
        for attempt in 1..=FLUSH_MAX_RETRIES {
            let flush_start = Instant::now();
            match Self::flush_batch_once(&ch_client, &table, &batch).await {
                Ok(()) => {
                    tracing::info!(
                        attempt,
                        batch_len,
                        flush_ms = flush_start.elapsed().as_millis() as u64,
                        "ch-batch-flushed"
                    );
                    return;
                }
                Err(e) => {
                    let delay = Self::retry_delay(attempt);
                    tracing::warn!(
                        error = ?e,
                        attempt,
                        max_retries = FLUSH_MAX_RETRIES,
                        batch_len,
                        retry_delay_ms = delay.as_millis() as u64,
                        "ch-flush-failed, retrying with new inserter"
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }

        if let Err(e) = Self::flush_batch_once(&ch_client, &table, &batch).await {
            tracing::error!(error = ?e, batch_len, "ch-flush-exhausted, dropping batch");
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        tracing::info!("Starting parallel ingester with {} queues", self.config.redis.queues.len());

        let cancel = CancellationToken::new();

        let (tx, rx) = mpsc::channel::<(String, String)>(10000);
        self.event_rx = Some(rx);

        let total_consumed = Arc::new(AtomicU64::new(0));

        for queue in &self.config.redis.queues {
            let queue_name = queue.name.clone();
            let redis_url = self.config.redis.url.clone();
            let tx_clone = tx.clone();
            let consumed_count = Arc::clone(&total_consumed);
            let cancel_clone = cancel.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::consume_redis_queue(queue_name.clone(), redis_url, tx_clone, consumed_count, cancel_clone).await {
                    tracing::error!(error = ?e, queue = %queue_name, "Redis consumer failed");
                }
            });
        }

        tracing::info!("Spawned {} parallel Redis consumers", self.config.redis.queues.len());

        drop(tx);

        let ch_client = clickhouse::Client::default()
            .with_url(self.config.clickhouse.url.clone())
            .with_user(self.config.clickhouse.user.clone())
            .with_password(self.config.clickhouse.password.clone())
            .with_database(self.config.clickhouse.db.clone());

        let batch_size = self.config.clickhouse.batch_size;
        let table = self.config.clickhouse.table.clone();
        let mut pending_batch: Vec<ChRow> = Vec::with_capacity(batch_size);
        let mut last_stats_time = Instant::now();
        let mut events_processed: u64 = 0;
        let mut flush_count: u64 = 0;

        // All in-flight flush handles — no limit, spawn freely
        let mut flush_handles: Vec<JoinHandle<()>> = Vec::new();

        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to register SIGTERM handler");

        loop {
            if last_stats_time.elapsed().as_secs() >= 30 {
                // Clean up completed handles
                flush_handles.retain(|h| !h.is_finished());

                let throughput = events_processed as f64 / last_stats_time.elapsed().as_secs_f64();
                tracing::info!(
                    events_processed = events_processed,
                    throughput_per_sec = format!("{:.1}", throughput),
                    total_consumed = total_consumed.load(Ordering::Relaxed),
                    flushes = flush_count,
                    inflight = flush_handles.len(),
                    "Ingester throughput stats (last 30s)"
                );
                events_processed = 0;
                flush_count = 0;
                last_stats_time = Instant::now();
            }

            tokio::select! {
                event = self.next_event() => {
                    match event {
                        Ok(Some((queue_name, json_str))) => {
                            pending_batch.push(ChRow {
                                ts: time::OffsetDateTime::now_utc(),
                                queue: queue_name,
                                json: json_str,
                            });

                            if pending_batch.len() >= batch_size {
                                let batch_to_flush = std::mem::replace(
                                    &mut pending_batch,
                                    Vec::with_capacity(batch_size),
                                );
                                events_processed += batch_to_flush.len() as u64;
                                flush_count += 1;

                                let ch = ch_client.clone();
                                let tbl = table.clone();
                                flush_handles.push(tokio::spawn(async move {
                                    Self::flush_batch_with_retry_owned(ch, tbl, batch_to_flush).await;
                                }));
                            }
                        }
                        Ok(None) => {
                            tracing::info!("All Redis consumers closed");
                            break;
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, "failed to get next event");
                        }
                    }
                }
                _ = sigterm.recv() => {
                    tracing::info!("SIGTERM received, initiating graceful shutdown");
                    cancel.cancel();
                    break;
                }
            }
        }

        // Flush remaining batch
        if !pending_batch.is_empty() {
            tracing::info!(batch_len = pending_batch.len(), "Flushing final batch before shutdown");
            flush_handles.push(tokio::spawn({
                let ch = ch_client.clone();
                let tbl = table.clone();
                async move {
                    Self::flush_batch_with_retry_owned(ch, tbl, pending_batch).await;
                }
            }));
        }

        // Drain remaining events from channel
        if let Some(rx) = &mut self.event_rx {
            let mut drain_batch: Vec<ChRow> = Vec::with_capacity(batch_size);
            let mut drained = 0;
            while let Ok((queue_name, json_str)) = rx.try_recv() {
                drain_batch.push(ChRow {
                    ts: time::OffsetDateTime::now_utc(),
                    queue: queue_name,
                    json: json_str,
                });
                drained += 1;
                if drain_batch.len() >= batch_size {
                    let batch = std::mem::replace(&mut drain_batch, Vec::with_capacity(batch_size));
                    flush_handles.push(tokio::spawn({
                        let ch = ch_client.clone();
                        let tbl = table.clone();
                        async move {
                            Self::flush_batch_with_retry_owned(ch, tbl, batch).await;
                        }
                    }));
                }
            }
            if !drain_batch.is_empty() {
                flush_handles.push(tokio::spawn({
                    let ch = ch_client.clone();
                    let tbl = table.clone();
                    async move {
                        Self::flush_batch_with_retry_owned(ch, tbl, drain_batch).await;
                    }
                }));
            }
            if drained > 0 {
                tracing::info!(drained, "Drained remaining events from channel");
            }
        }

        // Await all in-flight flushes
        let pending = flush_handles.len();
        if pending > 0 {
            tracing::info!(pending, "Waiting for all in-flight flushes to complete");
        }
        for handle in flush_handles {
            if let Err(e) = handle.await {
                tracing::error!(error = ?e, "flush task panicked");
            }
        }

        tracing::info!("Ingester shutdown complete");
        Ok(())
    }
}
