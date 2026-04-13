use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time;
use tracing::info;

pub struct EventMonitor {
    event_records: Arc<Mutex<VecDeque<(Instant, String)>>>,
    window_duration: Duration,
}

impl EventMonitor {
    pub fn new(window_duration: Duration) -> Self {
        Self {
            event_records: Arc::new(Mutex::new(VecDeque::new())),
            window_duration,
        }
    }

    pub async fn record_event(&self, event_type: String) {
        let now = Instant::now();
        let mut records = self.event_records.lock().await;

        records.push_back((now, event_type));

        let cutoff = now - self.window_duration;
        while let Some((timestamp, _)) = records.front() {
            if *timestamp < cutoff {
                records.pop_front();
            } else {
                break;
            }
        }
    }

    pub async fn get_stats(&self) -> EventStats {
        let now = Instant::now();
        let mut records = self.event_records.lock().await;

        let cutoff = now - self.window_duration;
        while let Some((timestamp, _)) = records.front() {
            if *timestamp < cutoff {
                records.pop_front();
            } else {
                break;
            }
        }

        let total_count = records.len();
        let last_event_age = records.back().map(|(ts, _)| now - *ts);

        // Count events by type
        let mut event_type_counts = HashMap::new();
        for (_, event_type) in records.iter() {
            *event_type_counts.entry(event_type.clone()).or_insert(0) += 1;
        }

        EventStats {
            total_count,
            event_type_counts,
            window_seconds: self.window_duration.as_secs_f64(),
            last_event_age_secs: last_event_age.map(|d| d.as_secs_f64()),
        }
    }

    pub fn spawn_logging_task(&self, log_interval: Duration) {
        let event_records = Arc::clone(&self.event_records);
        let window_duration = self.window_duration;

        tokio::spawn(async move {
            let mut interval = time::interval(log_interval);
            interval.tick().await;

            loop {
                interval.tick().await;

                let monitor = EventMonitor {
                    event_records: Arc::clone(&event_records),
                    window_duration,
                };

                let stats = monitor.get_stats().await;
                let rate = stats.total_count as f64 / stats.window_seconds;

                // Format event type breakdown
                let mut event_types_str = String::new();
                for (event_type, count) in &stats.event_type_counts {
                    if !event_types_str.is_empty() {
                        event_types_str.push_str(", ");
                    }
                    event_types_str.push_str(&format!("{}={}", event_type.to_lowercase(), count));
                }

                info!(
                    total_events = stats.total_count,
                    window_secs = stats.window_seconds,
                    events_per_sec = format!("{:.1}", rate),
                    last_event_age_secs = stats
                        .last_event_age_secs
                        .map(|age| format!("{:.1}", age))
                        .unwrap_or_else(|| "none".to_string()),
                    event_breakdown = event_types_str,
                    "event_monitor_stats"
                );
            }
        });
    }
}

pub(crate) struct EventStats {
    total_count: usize,
    event_type_counts: HashMap<String, usize>,
    window_seconds: f64,
    last_event_age_secs: Option<f64>,
}
