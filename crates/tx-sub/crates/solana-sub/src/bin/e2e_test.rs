use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use std::env;
use std::str::FromStr;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use solana_sub::parser::BONK_ID;
use solana_sub::{App, AppConfig, GrpcSubscriptionConfig, ParserConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize rustls crypto provider (required for TLS connections)
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("e2e_test: failed to install rustls crypto provider");

    // Initialize logging with environment filter
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("debug,h2=info,hyper=info,tower=info"));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();

    info!("Starting Bonk E2E test");

    // Load configuration
    let config_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "config/config.e2e_test.yaml".to_string());

    info!("Loading configuration from: {}", config_path);

    let app_config = AppConfig::from_file(&config_path).map_err(|e| {
        error!("Failed to load config file: {}", e);
        e
    })?;

    // Verify test config is present
    if app_config.test.is_none() {
        error!(
            "Test configuration not found in config file. Please ensure 'test' section is defined."
        );
        return Err(anyhow::anyhow!("Missing test configuration"));
    }

    let test_config = app_config.test.as_ref().unwrap();
    info!(
        "Test configuration loaded: event_count={}, print_format={}",
        test_config.event_count, test_config.print_format
    );

    // Check if Bonk program ID is in the list
    let bonk_program_id = BONK_ID.to_string();
    if !app_config.grpc.program_ids.contains(&bonk_program_id) {
        error!(
            "Bonk program ID {} not found in grpc.program_ids",
            bonk_program_id
        );
        return Err(anyhow::anyhow!("Bonk program ID not in monitored programs"));
    }

    info!("Bonk program ID found in configuration");

    // Convert program IDs from strings to Pubkeys
    let program_ids: Vec<Pubkey> = app_config
        .grpc
        .program_ids
        .iter()
        .filter_map(|id_str| Pubkey::from_str(id_str).ok())
        .collect();

    // Create gRPC configuration
    let grpc_config = GrpcSubscriptionConfig {
        endpoint: app_config.grpc.endpoint.clone(),
        x_token: app_config.grpc.x_token.clone(),
        program_ids,
    };

    // Create parser configuration
    let pumpfun_program_id = Pubkey::from_str(&app_config.pumpfun.program_id)
        .map_err(|e| anyhow::anyhow!("Invalid pumpfun program ID: {}", e))?;

    let parser_config = ParserConfig {
        rpc_url: None, // E2E test doesn't need RPC for ALT lookups
        pumpfun_program_id,
    };

    info!("Creating application instance");

    // Create and run the app
    let mut app = App::new(&app_config, grpc_config, parser_config);

    info!(
        "Starting E2E test - waiting for {} Bonk events",
        test_config.event_count
    );
    info!("Press Ctrl+C to stop early");

    // Run the app
    match app.run().await {
        Ok(_) => {
            info!("E2E test completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("E2E test failed: {}", e);
            Err(e)
        }
    }
}
