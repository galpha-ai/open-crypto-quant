mod app;
mod cli;
mod config;
mod error;
mod logging;
mod models;

use anyhow::Context;
use clap::Parser;
use cli::Cli;
use config::IngesterAppConfig;
use logging::setup_logging;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("ingester: failed to install rustls crypto provider");

    // Initialize logging
    setup_logging();

    // Parse command line arguments
    let cli = Cli::parse();

    // Get config from file or use defaults
    let config_path = cli.config.ok_or(anyhow::anyhow!("Config file not provided"))?;

    // Parse the config file
    let config = IngesterAppConfig::from_file(&config_path).with_context(|| {
        format!(
            "Failed to read/parse config file: {}",
            config_path.display()
        )
    })?;

    // Log the ingester config
    tracing::info!(config = ?config, "Ingester configuration loaded");

    // Create and run the app
    let mut app = app::IngesterApp::new(config);
    app.run().await.map_err(|e| anyhow::anyhow!(e))
}