use std::{collections::HashMap, sync::Mutex, time::Duration};

use anyhow::Result;
use once_cell::sync::Lazy;

use super::{
    BloxrouteTransactionSubmitter, JitoTransactionSubmitter, SolanaTransactionSubmitter,
    TransactionSubmitter, ZeroSlotTransactionSubmitter,
};
use crate::execution::solana::{SolanaOrderExecutorBuilder, confirmation::GrpcConfirmationMonitor};

// Type alias for the factory function
// It takes the builder's state and the confirmer, returns a submitter
type SubmitterFactory = Box<
    dyn Fn(
            &SolanaOrderExecutorBuilder, // Pass the builder state for config access
            GrpcConfirmationMonitor,
        ) -> Result<Box<dyn TransactionSubmitter>>
        + Send
        + Sync,
>;

// Static registry using Lazy and Mutex
static SUBMITTER_REGISTRY: Lazy<Mutex<HashMap<String, SubmitterFactory>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Registers a new transaction submitter factory.
/// Should be called during application initialization.
pub fn register_submitter_factory(name: String, factory: SubmitterFactory) {
    let mut registry = SUBMITTER_REGISTRY.lock().unwrap();
    if registry.insert(name.clone(), factory).is_some() {
        tracing::warn!("Submitter type '{}' was registered more than once.", name);
    } else {
        tracing::info!("Registered transaction submitter factory for '{}'", name);
    }
}

/// Creates a transaction submitter instance using a registered factory.
pub fn create_submitter(
    name: &str,
    config: &SolanaOrderExecutorBuilder,
    confirmer: GrpcConfirmationMonitor,
) -> Result<Box<dyn TransactionSubmitter>> {
    let registry = SUBMITTER_REGISTRY.lock().unwrap();
    let factory = registry
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Unknown transaction submitter type: {}", name))?;

    // Call the factory function with the builder's config and the confirmer
    factory(config, confirmer)
}

// --- Factory Functions ---

fn create_jito_submitter(
    config: &SolanaOrderExecutorBuilder,
    confirmer: GrpcConfirmationMonitor,
) -> Result<Box<dyn TransactionSubmitter>> {
    let jito_url = config
        .jito_block_engine_url()
        .ok_or_else(|| anyhow::anyhow!("'jito_block_engine_url' is required for 'jito' submitter"))?
        .to_string(); // Convert &str to String
    let confirmation_timeout = config
        .confirmation_timeout() // Use general confirmation timeout
        .unwrap_or(Duration::from_secs(60));
    Ok(Box::new(JitoTransactionSubmitter::new(
        jito_url.to_string(),
        config.jito_api_key().map(String::from), // Convert Option<&str> to Option<String>
        confirmer,
        config.jito_metrics().cloned(),
        confirmation_timeout,
    )))
}

fn create_bloxroute_submitter(
    config: &SolanaOrderExecutorBuilder,
    confirmer: GrpcConfirmationMonitor,
) -> Result<Box<dyn TransactionSubmitter>> {
    let bloxroute_url = config
        .bloxroute_api_url()
        .ok_or_else(|| {
            anyhow::anyhow!("'bloxroute_api_url' is required for 'bloxroute' submitter")
        })?
        .to_string();
    let auth_header = config
        .bloxroute_auth_header()
        .ok_or_else(|| {
            anyhow::anyhow!("'bloxroute_auth_header' is required for 'bloxroute' submitter")
        })?
        .to_string();
    let confirmation_timeout = config
        .confirmation_timeout() // Use general confirmation timeout
        .unwrap_or(Duration::from_secs(60)); // Use default if not provided
    Ok(Box::new(BloxrouteTransactionSubmitter::new(
        bloxroute_url,
        auth_header,
        config.rpc_url().to_string(), // Use main RPC for confirmation fallback
        confirmer,
        confirmation_timeout,
    )))
}

fn create_zeroslot_submitter(
    config: &SolanaOrderExecutorBuilder,
    confirmer: GrpcConfirmationMonitor,
) -> Result<Box<dyn TransactionSubmitter>> {
    let zeroslot_url = config
        .zeroslot_api_url()
        .ok_or_else(|| anyhow::anyhow!("'zeroslot_api_url' is required for 'zeroslot' submitter"))?
        .to_string();
    let api_key = config
        .zeroslot_api_key()
        .ok_or_else(|| anyhow::anyhow!("'zeroslot_api_key' is required for 'zeroslot' submitter"))?
        .to_string();
    let confirmation_timeout = config
        .confirmation_timeout() // Use general confirmation timeout
        .unwrap_or(Duration::from_secs(60));
    Ok(Box::new(ZeroSlotTransactionSubmitter::new(
        zeroslot_url,
        api_key,
        config.rpc_url().to_string(), // Use main RPC for confirmation fallback
        confirmer,
        confirmation_timeout,
    )))
}

fn create_solana_rpc_submitter(
    config: &SolanaOrderExecutorBuilder,
    _confirmer: GrpcConfirmationMonitor, // Not used by SolanaTransactionSubmitter
) -> Result<Box<dyn TransactionSubmitter>> {
    Ok(Box::new(SolanaTransactionSubmitter::new(
        config.rpc_url().to_string(),
    )))
}

/// Call this function once at application startup to register all known submitters.
pub fn register_all_submitters() {
    register_submitter_factory("jito".to_string(), Box::new(create_jito_submitter));
    register_submitter_factory(
        "bloxroute".to_string(),
        Box::new(create_bloxroute_submitter),
    );
    register_submitter_factory("zeroslot".to_string(), Box::new(create_zeroslot_submitter));
    register_submitter_factory("rpc".to_string(), Box::new(create_solana_rpc_submitter));
}
