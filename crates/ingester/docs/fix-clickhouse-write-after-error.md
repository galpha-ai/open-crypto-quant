# Fix: ClickHouse "write() after error" Panic

## Problem

The ingester crashes with a Rust panic when ClickHouse Cloud experiences transient TLS connection failures.

### Crash Log

```
ERROR ch-insert-error error="Network(hyper_util::client::legacy::Error(Connect, Custom { kind: Other, error: Custom { kind: UnexpectedEof, error: \"tls handshake eof\" } }))"
ERROR ch-write-error  error="Network(...tls handshake eof...)"
thread 'main' panicked at clickhouse-0.13.3/src/insert.rs:232: write() after error
```

**Exit code 101** (Rust panic), K8s restarts the pod. Observed on `polymarket-ingester` — 2 restarts in 4 days.

### Root Cause

The `clickhouse` crate's `Insert` type has an invariant: **once `write()` or `end()` returns an error, any subsequent `write()` call will panic**. This is by design in the crate to prevent writing to a corrupted stream.

The current code in `app.rs` has two bugs:

**Bug 1: `write()` error not handled** (line ~198-206)

```rust
ch_inserter.write(&ChRow { ... }).await.unwrap_or_else(|e| {
    tracing::error!(error = ?e, "ch-write-error");
});
// Loop continues → next write() panics
```

When `write()` fails, the error is logged but the inserter is NOT replaced. The next loop iteration calls `write()` on the poisoned inserter → panic.

**Bug 2: `end()` error in spawned task not propagated** (line ~170-176)

```rust
tokio::spawn(async move {
    prev_ch_inserter.end().await.unwrap_or_else(|e| {
        tracing::error!(error = ?e, "ch-insert-error");
    });
});
```

When `end()` fails in the background task, the error is logged but the current inserter (which may share the same broken TLS connection pool) is not affected. However, during a ClickHouse outage, the *current* inserter's `write()` will also fail on the next call, triggering Bug 1.

### Failure Sequence

```
1. ClickHouse Cloud TLS connection drops
2. Spawned task: end() fails → "ch-insert-error" logged
3. Main loop: write() fails → "ch-write-error" logged (but inserter NOT replaced)
4. Main loop: write() called again on poisoned inserter → PANIC "write() after error"
5. Process exits code 101, K8s restarts pod
```

## Proposed Fix

**Core invariant: after any `write()` or `end()` error, discard the inserter and create a new one.**

### Option A: Recreate inserter on write error (minimal change)

Replace the current write error handling:

```rust
// BEFORE (broken):
ch_inserter.write(&ChRow { ... }).await.unwrap_or_else(|e| {
    tracing::error!(error = ?e, "ch-write-error");
});

// AFTER (fixed):
if let Err(e) = ch_inserter.write(&ChRow { ... }).await {
    tracing::error!(error = ?e, "ch-write-error, recreating inserter");
    // Discard poisoned inserter, create fresh one
    // The failed row and any rows in the current batch are lost
    ch_inserter = ch_client.insert(&self.config.clickhouse.table)?;
    ch_bathlen = 0;
}
```

**Pros**: Minimal code change, fixes the panic.
**Cons**: Loses the current batch (up to `batch_size` rows). Acceptable since the data is still in the events table upstream, and the ingester will continue processing new events.

### Option B: Batch-and-retry pattern (recommended by Codex review)

Refactor to collect rows into a `Vec<ChRow>` buffer, then flush the whole batch with retry:

```rust
async fn flush_batch_once(
    ch_client: &clickhouse::Client,
    table: &str,
    batch: &[ChRow],
) -> anyhow::Result<()> {
    let mut inserter = ch_client.insert(table)?;
    for row in batch {
        inserter.write(row).await?;
    }
    inserter.end().await?;
    Ok(())
}

async fn flush_batch_with_retry(
    ch_client: &clickhouse::Client,
    table: &str,
    batch: &[ChRow],
    max_retries: u32,
) -> anyhow::Result<()> {
    for attempt in 1..=max_retries {
        match flush_batch_once(ch_client, table, batch).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                let delay = Duration::from_millis(100 * 2u64.pow(attempt.min(7)));
                tracing::warn!(
                    error = ?e, attempt, batch_len = batch.len(),
                    delay_ms = delay.as_millis() as u64,
                    "ClickHouse flush failed, retrying with new inserter"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
    anyhow::bail!("ClickHouse flush failed after {max_retries} retries")
}
```

Main loop becomes:

```rust
let mut pending_batch: Vec<ChRow> = Vec::with_capacity(batch_size);

loop {
    match self.next_event().await {
        Ok(Some((queue_name, json_str))) => {
            pending_batch.push(ChRow {
                ts: time::OffsetDateTime::now_utc(),
                queue: queue_name,
                json: json_str,
            });

            if pending_batch.len() >= batch_size {
                flush_batch_with_retry(&ch_client, &table, &pending_batch, 10).await?;
                pending_batch.clear();
            }
        }
        Ok(None) => {
            // Flush remaining
            if !pending_batch.is_empty() {
                flush_batch_with_retry(&ch_client, &table, &pending_batch, 10).await?;
            }
            break;
        }
        Err(e) => { ... }
    }
}
```

**Pros**: No data loss on transient failures. Each retry creates a fresh inserter. Exponential backoff prevents hammering ClickHouse.
**Cons**: Requires `ChRow` to implement `Clone` (or be rebuildable). Removes the fire-and-forget async flush (batches are now flushed synchronously in the main loop). This is actually better for correctness — the current async flush means the main loop can race ahead and accumulate unbounded inserters during an outage.

### Recommendation

**Option B** is the better fix. It solves both bugs, prevents data loss, and the synchronous flush is actually safer. The `ChRow` struct is small (DateTime + 2 Strings) so cloning is cheap.

The current async `tokio::spawn` for `end()` is an optimization that trades correctness for throughput — during an outage it spawns many failing tasks in rapid succession. Synchronous flush with retry is simpler and more robust.

## Additional Notes

- The `ChRow` struct needs `Clone` derive (currently only has `Debug, Row, Serialize`)
- Consider adding a metric for ClickHouse write retries to monitor connection stability
- The 2 restarts in 4 days suggest the TLS issue is infrequent but real — likely ClickHouse Cloud maintenance windows
