use tracing_subscriber::{
    EnvFilter,
    fmt::{format::FmtSpan, time::UtcTime},
};

const ENABLE_SPAN_EVENTS: &str = "ENABLE_SPAN_EVENTS";

pub fn setup_logging() {
    let enable_span_events = std::env::var(ENABLE_SPAN_EVENTS)
        .map(|v| v.parse::<bool>().unwrap_or(false))
        .unwrap_or(false);

    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::from_default_env())
        .with_current_span(true)
        .with_span_list(true)
        .with_span_events(if enable_span_events {
            FmtSpan::CLOSE
        } else {
            FmtSpan::NONE
        })
        .with_timer(UtcTime::rfc_3339())
        .flatten_event(true)
        .with_file(true)
        .with_line_number(true)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_level(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
}