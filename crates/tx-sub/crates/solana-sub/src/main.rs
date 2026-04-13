use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use tracing_subscriber::{
    EnvFilter,
    fmt::{format::FmtSpan, time::UtcTime},
};

const ENABLE_SPAN_EVENTS: &str = "ENABLE_SPAN_EVENTS";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to YAML config file
    #[arg(short = 'c', value_name = "FILE")]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("txn-maker: failed to install rustls crypto provider");

    // Initialize logging
    setup_logging();

    // Parse command line arguments
    let cli = Cli::parse();

    // Get config from file or use defaults
    let config_path = cli.config.ok_or(anyhow!("Config file not provided"))?;

    // Parse the config file
    let config = solana_sub::AppConfig::from_file(&config_path).with_context(|| {
        format!(
            "Failed to read/parse config file: {}",
            config_path.display()
        )
    })?;

    // Create configs for app components
    let grpc_config = solana_sub::GrpcSubscriptionConfig::from_app_config(&config);
    let parser_config = solana_sub::ParserConfig::from_config(&config.pumpfun);

    // Create and run the app
    let mut app = solana_sub::App::new(&config, grpc_config, parser_config);
    app.run().await
}

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
