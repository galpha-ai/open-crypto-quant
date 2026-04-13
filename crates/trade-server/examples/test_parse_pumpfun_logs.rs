use std::str::FromStr;

use anyhow::{Context, Result};
use clap::Parser;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{commitment_config::CommitmentConfig, signature::Signature};
use solana_transaction_status_client_types::{
    UiTransactionEncoding, option_serializer::OptionSerializer,
};
use tracing::info;

fn setup_logging() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");
}

#[derive(Parser, Debug)]
struct Cli {
    #[arg(long)]
    rpc_url: String,

    #[arg(long)]
    signature: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    let cli = Cli::parse();

    // Parse signature
    info!("Parsing transaction signature...");
    let signature =
        Signature::from_str(&cli.signature).context("Failed to parse transaction signature")?;

    // Create RPC client
    info!("Connecting to RPC endpoint: {}", cli.rpc_url);
    let rpc_client = RpcClient::new(cli.rpc_url);

    // Get transaction details with logs using proper config for versioned transactions
    info!("Fetching transaction details...");
    let config = solana_client::rpc_config::RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Json),
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    };
    let tx_with_meta = rpc_client
        .get_transaction_with_config(&signature, config)
        .context("Failed to fetch transaction details")?;

    // Process logs
    if let Some(meta) = tx_with_meta.transaction.meta {
        if let OptionSerializer::Some(post_token_balances) = meta.post_token_balances {
            for (_i, balance) in post_token_balances.iter().enumerate() {
                info!(
                    mint = balance.mint,
                    ui_token_amount = ?balance.ui_token_amount,
                    owner = ?balance.owner,
                    program_id = ?balance.program_id,
                    "Found token balance",
                );
            }
        }
    } else {
        info!("No transaction metadata available");
    }

    Ok(())
}
