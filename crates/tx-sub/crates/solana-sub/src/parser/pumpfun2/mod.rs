//#![doc = include_str!("../RUSTDOC.md")]
#![allow(warnings)]

pub mod accounts;
pub mod common;
pub mod constants;
pub mod error;
pub mod instructions;
pub mod utils;

use std::sync::Arc;

use borsh::BorshDeserialize;
use common::types::{Cluster, PriorityFee};
use instructions::{Buy, Create};
use popeyes_trading_types::{TokenCreationEvent, TradeEventType};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    instruction::{CompiledInstruction, Instruction},
    pubkey::Pubkey,
};
#[cfg(feature = "create-ata")]
use spl_associated_token_account::instruction::create_associated_token_account;
#[cfg(feature = "close-ata")]
use spl_token::instruction::close_account;

use super::{PumpfunCreateLog, TxnDecodeCtx};
use crate::parser::PUMPFUN_CREATE_LOG_DISCRIMINATOR;
//use utils::transaction::get_transaction;

//// Main client for interacting with the Pump.fun program
////
//// This struct provides the primary interface for interacting with the Pump.fun
//// token platform on Solana. It handles connection to the Solana network and provides
//// methods for token creation, buying, and selling using bonding curves.
////
//// # Examples
////
//// ```no_run
//// use txn_maker::pumpfun2::{PumpFun, common::types::{Cluster, PriorityFee}};
//// use solana_sdk::{commitment_config::CommitmentConfig, signature::Keypair};
//// use std::sync::Arc;
////
//// // Create a new client connected to devnet
//// let payer = Arc::new(Keypair::new());
//// let commitment = CommitmentConfig::confirmed();
//// let priority_fee = PriorityFee::default();
//// let cluster = Cluster::devnet(commitment, priority_fee);
//// let client = PumpFun::new(payer, cluster);
/// ```
pub struct PumpFun {
    /// Keypair used to sign transactions
    pub payer: Arc<Pubkey>,
    /// RPC client for Solana network requests
    pub rpc: Arc<RpcClient>,
    /// Cluster configuration
    pub cluster: Cluster,
}

impl PumpFun {
    //// Creates a new PumpFun client instance
    ////
    //// Initializes a new client for interacting with the Pump.fun program on Solana.
    //// This client manages connection to the Solana network and provides methods for
    //// creating, buying, and selling tokens.
    ////
    //// # Arguments
    ////
    //// * `payer` - Keypair used to sign and pay for transactions
    //// * `cluster` - Solana cluster configuration including RPC endpoints and transaction parameters
    ////
    //// # Returns
    ////
    //// Returns a new PumpFun client instance configured with the provided parameters
    ////
    //// # Examples
    ////
    //// ```no_run
    //// use txn_maker::pumpfun2::{PumpFun, common::types::{Cluster, PriorityFee}};
    //// use solana_sdk::{commitment_config::CommitmentConfig, signature::Keypair};
    //// use std::sync::Arc;
    ////
    //// let payer = Arc::new(Keypair::new());
    //// let commitment = CommitmentConfig::confirmed();
    //// let priority_fee = PriorityFee::default();
    //// let cluster = Cluster::devnet(commitment, priority_fee);
    //// let client = PumpFun::new(payer, cluster);
    //// ```
    pub fn new(payer: Arc<Pubkey>, cluster: Cluster) -> Self {
        // Create Solana RPC Client with HTTP endpoint
        let rpc = Arc::new(RpcClient::new_with_commitment(
            cluster.rpc.http.clone(),
            cluster.commitment,
        ));

        // Return configured PumpFun client
        Self {
            payer,
            rpc,
            cluster,
        }
    }

    //// Creates a new token with metadata by uploading metadata to IPFS and initializing on-chain accounts
    ////
    //// This method handles the complete process of creating a new token on Pump.fun:
    //// 1. Uploads token metadata and image to IPFS
    //// 2. Creates a new SPL token with the provided mint keypair
    //// 3. Initializes the bonding curve that determines token pricing
    //// 4. Sets up metadata using the Metaplex standard
    ////
    //// # Arguments
    ////
    //// * `mint` - Keypair for the new token mint account that will be created
    //// * `metadata` - Token metadata including name, symbol, description and image file
    //// * `priority_fee` - Optional priority fee configuration for compute units. If None, uses the
    ////                    default from the cluster configuration
    ////
    //// # Returns
    ////
    //// Returns the transaction signature if successful, or a ClientError if the operation fails
    ////
    //// # Errors
    ////
    //// Returns an error if:
    //// - Metadata upload to IPFS fails
    //// - Transaction creation fails
    //// - Transaction execution on Solana fails
    ////
    //// # Examples
    ////
    //// ```no_run
    //// # use txn_maker::pumpfun2::{PumpFun, common::types::{Cluster, PriorityFee}, utils::CreateTokenMetadata};
    //// # use solana_sdk::{commitment_config::CommitmentConfig, signature::Keypair};
    //// # use std::sync::Arc;
    //// #
    //// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    //// # let payer = Arc::new(Keypair::new());
    //// # let commitment = CommitmentConfig::confirmed();
    //// # let cluster = Cluster::devnet(commitment, PriorityFee::default());
    //// # let client = PumpFun::new(payer, cluster);
    //// let mint = Keypair::new();
    //// let metadata = CreateTokenMetadata {
    ////     name: "My Token".to_string(),
    ////     symbol: "MYTKN".to_string(),
    ////     description: "A test token created with Pump.fun".to_string(),
    ////     file: "path/to/image.png".to_string(),
    ////     twitter: None,
    ////     telegram: None,
    ////     website: Some("https://example.com".to_string()),
    //// };
    ////
    //// let signature = client.create(mint, metadata, None).await?;
    //// println!("Token created! Signature: {}", signature);
    //// # Ok(())
    //// # }
    //// ```
    //pub async fn create(
    //    &self,
    //    mint: Keypair,
    //    metadata: utils::CreateTokenMetadata,
    //    priority_fee: Option<PriorityFee>,
    //) -> Result<Signature, error::ClientError> {
    //    // First upload metadata and image to IPFS
    //    let ipfs: utils::TokenMetadataResponse = utils::create_token_metadata(metadata)
    //        .await
    //        .map_err(error::ClientError::UploadMetadataError)?;

    //    // Add priority fee if provided or default to cluster priority fee
    //    let priority_fee = priority_fee.unwrap_or(self.cluster.priority_fee);
    //    let mut instructions = Self::get_priority_fee_instructions(&priority_fee);

    //    // Add create token instruction
    //    let create_ix = self.get_create_instruction(&mint, ipfs);
    //    instructions.push(create_ix);

    //    let pmint = [&mint];

    //    // Create and sign transaction
    //    let transaction = get_transaction(
    //        self.rpc.clone(),
    //        self.payer.clone(),
    //        &instructions,
    //        Some(&pmint),
    //        //#[cfg(feature = "versioned-tx")]
    //        None,
    //    )
    //    .await?;

    //    // Send and confirm transaction
    //    let signature = self
    //        .rpc
    //        .send_and_confirm_transaction(&transaction)
    //        .await
    //        .map_err(error::ClientError::SolanaClientError)?;

    //    Ok(signature)
    //}

    //// Creates a new token and immediately buys an initial amount in a single atomic transaction
    ////
    //// This method combines token creation and an initial purchase into a single atomic transaction.
    //// This is often preferred for new token launches as it:
    //// 1. Creates the token and its bonding curve
    //// 2. Makes an initial purchase to establish liquidity
    //// 3. Guarantees that the creator becomes the first holder
    ////
    //// The entire operation is executed as a single transaction, ensuring atomicity.
    ////
    //// # Arguments
    ////
    //// * `mint` - Keypair for the new token mint account that will be created
    //// * `metadata` - Token metadata including name, symbol, description and image file
    //// * `amount_sol` - Amount of SOL to spend on the initial buy, in lamports (1 SOL = 1,000,000,000 lamports)
    //// * `slippage_basis_points` - Optional maximum acceptable slippage in basis points (1 bp = 0.01%).
    ////                             If None, defaults to 500 (5%)
    //// * `priority_fee` - Optional priority fee configuration for compute units. If None, uses the
    ////                    default from the cluster configuration
    ////
    //// # Returns
    ////
    //// Returns the transaction signature if successful, or a ClientError if the operation fails
    ////
    //// # Errors
    ////
    //// Returns an error if:
    //// - Metadata upload to IPFS fails
    //// - Account retrieval fails
    //// - Transaction creation fails
    //// - Transaction execution on Solana fails
    ////
    //// # Examples
    ////
    //// ```no_run
    //// # use txn_maker::pumpfun2::{PumpFun, common::types::{Cluster, PriorityFee}, utils::CreateTokenMetadata};
    //// # use solana_sdk::{commitment_config::CommitmentConfig, native_token::sol_to_lamports, signature::Keypair};
    //// # use std::sync::Arc;
    //// #
    //// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    //// # let payer = Arc::new(Keypair::new());
    //// # let commitment = CommitmentConfig::confirmed();
    //// # let cluster = Cluster::devnet(commitment, PriorityFee::default());
    //// # let client = PumpFun::new(payer, cluster);
    //// let mint = Keypair::new();
    //// let metadata = CreateTokenMetadata {
    ////     name: "My Token".to_string(),
    ////     symbol: "MYTKN".to_string(),
    ////     description: "A test token created with Pump.fun".to_string(),
    ////     file: "path/to/image.png".to_string(),
    ////     twitter: None,
    ////     telegram: None,
    ////     website: Some("https://example.com".to_string()),
    //// };
    ////
    //// // Create token and buy 0.1 SOL worth with 5% slippage tolerance
    //// let amount_sol = sol_to_lamports(0.1f64); // 0.1 SOL in lamports
    //// let slippage_bps = Some(500); // 5%
    ////
    //// let signature = client.create_and_buy(mint, metadata, amount_sol, slippage_bps, None).await?;
    //// println!("Token created and bought! Signature: {}", signature);
    //// # Ok(())
    //// # }
    //// ```
    //pub async fn create_and_buy(
    //    &self,
    //    mint: Keypair,
    //    metadata: utils::CreateTokenMetadata,
    //    amount_sol: u64,
    //    slippage_basis_points: Option<u64>,
    //    priority_fee: Option<PriorityFee>,
    //) -> Result<Signature, error::ClientError> {
    //    // Upload metadata to IPFS first
    //    let ipfs: utils::TokenMetadataResponse = utils::create_token_metadata(metadata)
    //        .await
    //        .map_err(error::ClientError::UploadMetadataError)?;

    //    // Add priority fee if provided or default to cluster priority fee
    //    let priority_fee = priority_fee.unwrap_or(self.cluster.priority_fee);
    //    let mut instructions = Self::get_priority_fee_instructions(&priority_fee);

    //    // Add create token instruction
    //    let create_ix = self.get_create_instruction(&mint, ipfs);
    //    instructions.push(create_ix);

    //    // Add buy instruction
    //    let buy_ix = self
    //        .get_buy_instructions(mint.pubkey(), amount_sol, slippage_basis_points)
    //        .await?;
    //    instructions.extend(buy_ix);

    //    let pmint = [&mint];

    //    // Create and sign transaction
    //    let transaction = get_transaction(
    //        self.rpc.clone(),
    //        self.payer.clone(),
    //        &instructions,
    //        Some(&pmint),
    //        //#[cfg(feature = "versioned-tx")]
    //        None,
    //    )
    //    .await?;

    //    // Send and confirm transaction
    //    let signature = self
    //        .rpc
    //        .send_and_confirm_transaction(&transaction)
    //        .await
    //        .map_err(error::ClientError::SolanaClientError)?;

    //    Ok(signature)
    //}

    //// Buys tokens from a bonding curve by spending SOL
    ////
    //// This method purchases tokens from a bonding curve by providing SOL. The amount of tokens
    //// received is determined by the bonding curve formula for the specific token. As more tokens
    //// are purchased, the price increases according to the curve function.
    ////
    //// The method:
    //// 1. Calculates how many tokens will be received for the given SOL amount
    //// 2. Creates an associated token account for the buyer if needed
    //// 3. Executes the buy transaction with slippage protection
    ////
    //// A portion of the SOL is taken as a fee according to the global configuration.
    ////
    //// # Arguments
    ////
    //// * `mint` - Public key of the token mint to buy
    //// * `amount_sol` - Amount of SOL to spend, in lamports (1 SOL = 1,000,000,000 lamports)
    //// * `slippage_basis_points` - Optional maximum acceptable slippage in basis points (1 bp = 0.01%).
    ////                             If None, defaults to 500 (5%)
    //// * `priority_fee` - Optional priority fee configuration for compute units. If None, uses the
    ////                    default from the cluster configuration
    ////
    //// # Returns
    ////
    //// Returns the transaction signature if successful, or a ClientError if the operation fails
    ////
    //// # Errors
    ////
    //// Returns an error if:
    //// - The bonding curve account cannot be found
    //// - The buy price calculation fails
    //// - Transaction creation fails
    //// - Transaction execution on Solana fails
    ////
    //// # Examples
    ////
    //// ```no_run
    //// # use txn_maker::pumpfun2::{PumpFun, common::types::{Cluster, PriorityFee}};
    //// # use solana_sdk::{commitment_config::CommitmentConfig, native_token::sol_to_lamports, pubkey, signature::Keypair};
    //// # use std::sync::Arc;
    //// #
    //// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    //// # let payer = Arc::new(Keypair::new());
    //// # let commitment = CommitmentConfig::confirmed();
    //// # let cluster = Cluster::devnet(commitment, PriorityFee::default());
    //// # let client = PumpFun::new(payer, cluster);
    //// let token_mint = pubkey!("SoMeTokenM1ntAddr3ssXXXXXXXXXXXXXXXXXXXXXXX");
    ////
    //// // Buy 0.01 SOL worth of tokens with 3% max slippage
    //// let amount_sol = sol_to_lamports(0.01f64); // 0.01 SOL in lamports
    //// let slippage_bps = Some(300); // 3%
    ////
    //// let signature = client.buy(token_mint, amount_sol, slippage_bps, None).await?;
    //// println!("Tokens purchased! Signature: {}", signature);
    //// # Ok(())
    //// # }
    //// ```
    //pub async fn buy(
    //    &self,
    //    mint: Pubkey,
    //    amount_sol: u64,
    //    slippage_basis_points: Option<u64>,
    //    priority_fee: Option<PriorityFee>,
    //) -> Result<Vec<Instruction>, error::ClientError> {
    //    // Add priority fee if provided or default to cluster priority fee
    //    let priority_fee = priority_fee.unwrap_or(self.cluster.priority_fee);
    //    let mut instructions = Self::get_priority_fee_instructions(&priority_fee);

    //    // Add buy instruction
    //    let buy_ix = self
    //        .get_buy_instructions(mint, amount_sol, slippage_basis_points)
    //        .await?;
    //    instructions.extend(buy_ix);

    //    // Create and sign transaction
    //    //let transaction = get_transaction(
    //    //    self.rpc.clone(),
    //    //    self.payer.clone(),
    //    //    &instructions,
    //    //    None,
    //    //    //#[cfg(feature = "versioned-tx")]
    //    //    None,
    //    //)
    //    //.await?;

    //    Ok(instructions)

    //    // Send and confirm transaction
    //    //let signature = self
    //    //    .rpc
    //    //    .send_and_confirm_transaction(&transaction)
    //    //    .await
    //    //    .map_err(error::ClientError::SolanaClientError)?;

    //    //Ok(signature)
    //}

    //// Sells tokens back to the bonding curve in exchange for SOL
    ////
    //// This method sells tokens back to the bonding curve, receiving SOL in return. The amount of SOL
    //// received is determined by the bonding curve formula for the specific token. As more tokens
    //// are sold, the price decreases according to the curve function.
    ////
    //// The method:
    //// 1. Determines how many tokens to sell (all tokens or a specific amount)
    //// 2. Calculates how much SOL will be received for the tokens
    //// 3. Executes the sell transaction with slippage protection
    ////
    //// A portion of the SOL is taken as a fee according to the global configuration.
    ////
    //// # Arguments
    ////
    //// * `mint` - Public key of the token mint to sell
    //// * `amount_token` - Optional amount of tokens to sell in base units. If None, sells the entire balance
    //// * `slippage_basis_points` - Optional maximum acceptable slippage in basis points (1 bp = 0.01%).
    ////                             If None, defaults to 500 (5%)
    //// * `priority_fee` - Optional priority fee configuration for compute units. If None, uses the
    ////                    default from the cluster configuration
    ////
    //// # Returns
    ////
    //// Returns the transaction signature if successful, or a ClientError if the operation fails
    ////
    //// # Errors
    ////
    //// Returns an error if:
    //// - The token account cannot be found
    //// - The bonding curve account cannot be found
    //// - The sell price calculation fails
    //// - Transaction creation fails
    //// - Transaction execution on Solana fails
    ////
    //// # Examples
    ////
    //// ```no_run
    //// # use txn_maker::pumpfun2::{PumpFun, common::types::{Cluster, PriorityFee}};
    //// # use solana_sdk::{commitment_config::CommitmentConfig, signature::Keypair, pubkey};
    //// # use std::sync::Arc;
    //// #
    //// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    //// # let payer = Arc::new(Keypair::new());
    //// # let commitment = CommitmentConfig::confirmed();
    //// # let cluster = Cluster::devnet(commitment, PriorityFee::default());
    //// # let client = PumpFun::new(payer, cluster);
    //// let token_mint = pubkey!("SoMeTokenM1ntAddr3ssXXXXXXXXXXXXXXXXXXXXXXX");
    ////
    //// // Sell 1000 tokens with 2% max slippage
    //// let amount_tokens = Some(1000);
    //// let slippage_bps = Some(200); // 2%
    ////
    //// let signature = client.sell(token_mint, amount_tokens, slippage_bps, None).await?;
    //// println!("Tokens sold! Signature: {}", signature);
    ////
    //// // Or sell all tokens with default slippage (5%)
    //// let signature = client.sell(token_mint, None, None, None).await?;
    //// println!("All tokens sold! Signature: {}", signature);
    //// # Ok(())
    //// # }
    //// ```
    //pub async fn sell(
    //    &self,
    //    mint: Pubkey,
    //    amount_token: Option<u64>,
    //    slippage_basis_points: Option<u64>,
    //    priority_fee: Option<PriorityFee>,
    //) -> Result<Vec<Instruction>, error::ClientError> {
    //    // Add priority fee if provided or default to cluster priority fee
    //    let priority_fee = priority_fee.unwrap_or(self.cluster.priority_fee);
    //    let mut instructions = Self::get_priority_fee_instructions(&priority_fee);

    //    // Add sell instruction
    //    let sell_ix = self
    //        .get_sell_instructions(mint, amount_token, slippage_basis_points)
    //        .await?;
    //    instructions.extend(sell_ix);

    //    // Create and sign transaction
    //    //let transaction = get_transaction(
    //    //    self.rpc.clone(),
    //    //    self.payer.clone(),
    //    //    &instructions,
    //    //    None,
    //    //    //#[cfg(feature = "versioned-tx")]
    //    //    None,
    //    //)
    //    //.await?;

    //    // Send and confirm transaction
    //    //let signature = self
    //    //    .rpc
    //    //    .send_and_confirm_transaction(&transaction)
    //    //    .await
    //    //    .map_err(error::ClientError::SolanaClientError)?;

    //    //Ok(signature)
    //    Ok(instructions)
    //}

    /// Subscribes to real-time events from the Pump.fun program
    ///
    /// This method establishes a WebSocket connection to the Solana cluster and subscribes
    /// to program log events from the Pump.fun program. It parses the emitted events into
    /// structured data types and delivers them through the provided callback function.
    ///
    /// Event types include:
    /// - `CreateEvent`: Emitted when a new token is created
    /// - `TradeEvent`: Emitted when tokens are bought or sold
    /// - `CompleteEvent`: Emitted when a bonding curve operation completes
    /// - `SetParamsEvent`: Emitted when global parameters are updated
    ///
    /// # Arguments
    ///
    /// * `commitment` - Optional commitment level for the subscription. If None, uses the
    ///                  default from the cluster configuration
    /// * `callback` - A function that will be called for each event with the following parameters:
    ///   * `signature`: The transaction signature as a String
    ///   * `event`: The parsed PumpFunEvent if successful, or None if parsing failed
    ///   * `error`: Any error that occurred during parsing, or None if successful
    ///   * `response`: The complete RPC logs response for additional context
    ///
    /// # Returns
    ///
    /// Returns a `Subscription` object that manages the lifecycle of the subscription.
    /// When this object is dropped, the subscription is automatically terminated. If
    /// the subscription cannot be established, returns a ClientError.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The WebSocket connection cannot be established
    /// - The subscription request fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use txn_maker::pumpfun2::{PumpFun, common::types::{Cluster, PriorityFee}};
    /// # use solana_sdk::{commitment_config::CommitmentConfig, signature::Keypair};
    /// # use std::{sync::Arc, error::Error};
    /// #
    /// # async fn example() -> Result<(), Box<dyn Error>> {
    /// # let payer = Arc::new(Keypair::new());
    /// # let commitment = CommitmentConfig::confirmed();
    /// # let cluster = Cluster::devnet(commitment, PriorityFee::default());
    /// # let client = PumpFun::new(payer, cluster);
    /// #
    /// // Subscribe to token events
    /// let subscription = client.subscribe(None, |signature, event, error, _| {
    ///     match event {
    ///         Some(pumpfun::common::stream::PumpFunEvent::Create(create_event)) => {
    ///             println!("New token created: {} ({})", create_event.name, create_event.symbol);
    ///             println!("Mint address: {}", create_event.mint);
    ///         },
    ///         Some(pumpfun::common::stream::PumpFunEvent::Trade(trade_event)) => {
    ///             let action = if trade_event.is_buy { "bought" } else { "sold" };
    ///             println!(
    ///                 "User {} {} {} tokens for {} SOL",
    ///                 trade_event.user,
    ///                 action,
    ///                 trade_event.token_amount,
    ///                 trade_event.sol_amount as f64 / 1_000_000_000.0
    ///             );
    ///         },
    ///         Some(event) => println!("Other event received: {:#?}", event),
    ///         None => {
    ///             if let Some(err) = error {
    ///                 eprintln!("Error parsing event in tx {}: {}", signature, err);
    ///             }
    ///         }
    ///     }
    /// }).await?;
    ///
    /// // Keep the subscription active
    /// // When no longer needed, drop the subscription to unsubscribe
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "stream")]
    pub async fn subscribe<F>(
        &self,
        commitment: Option<solana_sdk::commitment_config::CommitmentConfig>,
        callback: F,
    ) -> Result<common::stream::Subscription, error::ClientError>
    where
        F: Fn(
                String,
                Option<common::stream::PumpFunEvent>,
                Option<Box<dyn std::error::Error>>,
                solana_client::rpc_response::Response<solana_client::rpc_response::RpcLogsResponse>,
            ) + Send
            + Sync
            + 'static,
    {
        common::stream::subscribe(self.cluster.clone(), commitment, callback).await
    }

    /// Creates compute budget instructions for priority fees
    ///
    /// Generates Solana compute budget instructions based on the provided priority fee
    /// configuration. These instructions are used to set the maximum compute units a
    /// transaction can consume and the price per compute unit, which helps prioritize
    /// transaction processing during network congestion.
    ///
    /// # Arguments
    ///
    /// * `priority_fee` - Priority fee configuration containing optional unit limit and unit price
    ///
    /// # Returns
    ///
    /// Returns a vector of instructions to set compute budget parameters, which can be
    /// empty if no priority fee parameters are provided
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use solana_sub::parser::pumpfun2::{PumpFun, common::types::PriorityFee};
    /// # use solana_sdk::instruction::Instruction;
    /// #
    /// // Set both compute unit limit and price
    /// let priority_fee = PriorityFee {
    ///     unit_limit: Some(200_000),
    ///     unit_price: Some(1_000), // 1000 micro-lamports per compute unit
    /// };
    ///
    /// let compute_instructions: Vec<Instruction> = PumpFun::get_priority_fee_instructions(&priority_fee);
    /// ```
    pub fn get_priority_fee_instructions(priority_fee: &PriorityFee) -> Vec<Instruction> {
        let mut instructions = Vec::new();

        if let Some(limit) = priority_fee.unit_limit {
            let limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(limit);
            instructions.push(limit_ix);
        }

        if let Some(price) = priority_fee.unit_price {
            let price_ix = ComputeBudgetInstruction::set_compute_unit_price(price);
            instructions.push(price_ix);
        }

        instructions
    }

    //// Creates an instruction for initializing a new token
    ////
    //// Generates a Solana instruction to create a new token with a bonding curve on Pump.fun.
    //// This instruction will initialize the token mint, metadata, and bonding curve accounts.
    ////
    //// # Arguments
    ////
    //// * `mint` - Keypair for the new token mint account that will be created
    //// * `ipfs` - Token metadata response from IPFS upload containing name, symbol, and URI
    ////
    //// # Returns
    ////
    //// Returns a Solana instruction for creating a new token
    ////
    //// # Examples
    ////
    //// ```no_run
    //// # use txn_maker::pumpfun2::{PumpFun, common::types::{Cluster, PriorityFee}, utils};
    //// # use solana_sdk::{commitment_config::CommitmentConfig, signature::Keypair};
    //// # use std::sync::Arc;
    //// #
    //// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    //// # let payer = Arc::new(Keypair::new());
    //// # let commitment = CommitmentConfig::confirmed();
    //// # let cluster = Cluster::devnet(commitment, PriorityFee::default());
    //// # let client = PumpFun::new(payer, cluster);
    //// #
    //// let mint = Keypair::new();
    //// let metadata_response = utils::create_token_metadata(
    ////     utils::CreateTokenMetadata {
    ////         name: "Example Token".to_string(),
    ////         symbol: "EXTKN".to_string(),
    ////         description: "An example token".to_string(),
    ////         file: "path/to/image.png".to_string(),
    ////         twitter: None,
    ////         telegram: None,
    ////         website: None,
    ////     }
    //// ).await?;
    ////
    //// let create_instruction = client.get_create_instruction(&mint.pubkey(), metadata_response);
    //// # Ok(())
    //// # }
    //// ```
    pub fn get_create_instruction(
        &self,
        mint: &Pubkey,
        ipfs: utils::TokenMetadataResponse,
    ) -> Instruction {
        instructions::create(
            &self.payer,
            mint,
            instructions::Create {
                name: ipfs.metadata.name,
                symbol: ipfs.metadata.symbol,
                uri: ipfs.metadata.image,
                creator: *self.payer,
            },
        )
    }

    //// Generates instructions for buying tokens from a bonding curve
    ////
    //// Creates a set of Solana instructions needed to purchase tokens using SOL. These
    //// instructions may include creating an associated token account if needed, and the actual
    //// buy instruction with slippage protection.
    ////
    //// # Arguments
    ////
    //// * `mint` - Public key of the token mint to buy
    //// * `amount_sol` - Amount of SOL to spend, in lamports (1 SOL = 1,000,000,000 lamports)
    //// * `slippage_basis_points` - Optional maximum acceptable slippage in basis points (1 bp = 0.01%).
    ////                             If None, defaults to 500 (5%)
    ////
    //// # Returns
    ////
    //// Returns a vector of Solana instructions if successful, or a ClientError if the operation fails
    ////
    //// # Errors
    ////
    //// Returns an error if:
    //// - The global account or bonding curve account cannot be fetched
    //// - The buy price calculation fails
    //// - Token account-related operations fail
    ////
    //// # Examples
    ////
    //// ```no_run
    //// # use txn_maker::pumpfun2::{PumpFun, common::types::{Cluster, PriorityFee}};
    //// # use solana_sdk::{commitment_config::CommitmentConfig, native_token::sol_to_lamports, signature::Keypair, pubkey};
    //// # use std::sync::Arc;
    //// #
    //// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    //// # let payer = Arc::new(Keypair::new());
    //// # let commitment = CommitmentConfig::confirmed();
    //// # let cluster = Cluster::devnet(commitment, PriorityFee::default());
    //// # let client = PumpFun::new(payer, cluster);
    //// #
    //// let mint = pubkey!("TokenM1ntPubk3yXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
    //// let amount_sol = sol_to_lamports(0.01); // 0.01 SOL
    //// let slippage_bps = Some(300); // 3%
    ////
    //// let buy_instructions = client.get_buy_instructions(mint, amount_sol, slippage_bps).await?;
    //// # Ok(())
    //// # }
    //// ```
    //pub async fn get_buy_instructions(
    //    &self,
    //    mint: Pubkey,
    //    amount_sol: u64,
    //    slippage_basis_points: Option<u64>,
    //) -> Result<Vec<Instruction>, error::ClientError> {
    //    // Get accounts and calculate buy amounts
    //    let global_account = self.get_global_account().await?;
    //    let bonding_curve_account = self.get_bonding_curve_account(&mint).await?;
    //    let buy_amount = bonding_curve_account
    //        .get_buy_price(amount_sol)
    //        .map_err(error::ClientError::BondingCurveError)?;
    //    let buy_amount_with_slippage =
    //        utils::calculate_with_slippage_buy(amount_sol, slippage_basis_points.unwrap_or(500));

    //    let mut instructions = Vec::new();

    //    // Create Associated Token Account if needed
    //    #[cfg(feature = "create-ata")]
    //    {
    //        let ata: Pubkey = get_associated_token_address(&self.payer.pubkey(), &mint);
    //        if self.rpc.get_account(&ata).await.is_err() {
    //            instructions.push(create_associated_token_account(
    //                &self.payer.pubkey(),
    //                &self.payer.pubkey(),
    //                &mint,
    //                &constants::accounts::TOKEN_PROGRAM,
    //            ));
    //        }
    //    }

    //    // Add buy instruction
    //    instructions.push(instructions::buy(
    //        &self.payer,
    //        &mint,
    //        &global_account.fee_recipient,
    //        instructions::Buy {
    //            amount: buy_amount,
    //            max_sol_cost: buy_amount_with_slippage,
    //        },
    //    ));

    //    Ok(instructions)
    //}

    //// Generates instructions for selling tokens back to a bonding curve
    ////
    //// Creates a set of Solana instructions needed to sell tokens in exchange for SOL. These
    //// instructions include the sell instruction with slippage protection and may include
    //// closing the associated token account if all tokens are being sold and the feature
    //// is enabled.
    ////
    //// # Arguments
    ////
    //// * `mint` - Public key of the token mint to sell
    //// * `amount_token` - Optional amount of tokens to sell in base units. If None, sells the entire balance
    //// * `slippage_basis_points` - Optional maximum acceptable slippage in basis points (1 bp = 0.01%).
    ////                             If None, defaults to 500 (5%)
    ////
    //// # Returns
    ////
    //// Returns a vector of Solana instructions if successful, or a ClientError if the operation fails
    ////
    //// # Errors
    ////
    //// Returns an error if:
    //// - The token account or token balance cannot be fetched
    //// - The global account or bonding curve account cannot be fetched
    //// - The sell price calculation fails
    //// - Token account closing operations fail (when applicable)
    ////
    //// # Examples
    ////
    //// ```no_run
    //// # use txn_maker::pumpfun2::{PumpFun, common::types::{Cluster, PriorityFee}};
    //// # use solana_sdk::{commitment_config::CommitmentConfig, signature::Keypair, pubkey};
    //// # use std::sync::Arc;
    //// #
    //// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    //// # let payer = Arc::new(Keypair::new());
    //// # let commitment = CommitmentConfig::confirmed();
    //// # let cluster = Cluster::devnet(commitment, PriorityFee::default());
    //// # let client = PumpFun::new(payer, cluster);
    //// #
    //// let mint = pubkey!("TokenM1ntPubk3yXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
    //// let amount_tokens = Some(1000); // Sell 1000 tokens
    //// let slippage_bps = Some(200); // 2%
    ////
    //// let sell_instructions = client.get_sell_instructions(mint, amount_tokens, slippage_bps).await?;
    ////
    //// // Or to sell all tokens:
    //// let sell_all_instructions = client.get_sell_instructions(mint, None, None).await?;
    //// # Ok(())
    //// # }
    //// ```
    //pub async fn get_sell_instructions(
    //    &self,
    //    mint: Pubkey,
    //    amount_token: Option<u64>,
    //    slippage_basis_points: Option<u64>,
    //) -> Result<Vec<Instruction>, error::ClientError> {
    //    // Get ATA
    //    let ata: Pubkey = get_associated_token_address(&self.payer, &mint);

    //    // Get token balance
    //    let token_balance = if amount_token.is_none() || cfg!(feature = "close-ata") {
    //        // We need the balance if amount_token is None OR if the close-ata feature is enabled
    //        let balance = self.rpc.get_token_account_balance(&ata).await?;
    //        Some(balance.amount.parse::<u64>().unwrap())
    //    } else {
    //        None
    //    };

    //    // Determine amount to sell
    //    let amount = amount_token.unwrap_or_else(|| token_balance.unwrap());

    //    // Calculate min sol output
    //    let global_account = self.get_global_account().await?;
    //    let bonding_curve_account = self.get_bonding_curve_account(&mint).await?;
    //    let min_sol_output = bonding_curve_account
    //        .get_sell_price(amount, global_account.fee_basis_points)
    //        .map_err(error::ClientError::BondingCurveError)?;
    //    let min_sol_output = utils::calculate_with_slippage_sell(
    //        min_sol_output,
    //        slippage_basis_points.unwrap_or(500),
    //    );

    //    let mut instructions = Vec::new();

    //    // Add sell instruction
    //    instructions.push(instructions::sell(
    //        self.rpc.clone(),
    //        &self.payer,
    //        &mint,
    //        &global_account.fee_recipient,
    //        instructions::Sell {
    //            amount,
    //            min_sol_output,
    //        },
    //    ));

    //    // Close account if balance equals amount
    //    #[cfg(feature = "close-ata")]
    //    {
    //        // Token balance should be guaranteed to be available at this point
    //        // due to our fetch logic in the beginning of the function
    //        if let Some(balance) = token_balance {
    //            // Only close the account if we're selling all tokens
    //            if balance == amount {
    //                let token_program = constants::accounts::TOKEN_PROGRAM;

    //                // Verify the token account exists before attempting to close it
    //                if self.rpc.get_account(&ata).await.is_ok() {
    //                    // Create instruction to close the ATA
    //                    let close_instruction = close_account(
    //                        &token_program,
    //                        &ata,
    //                        &self.payer.pubkey(),
    //                        &self.payer.pubkey(),
    //                        &[&self.payer.pubkey()],
    //                    )
    //                    .map_err(|err| {
    //                        error::ClientError::OtherError(format!(
    //                            "Failed to create close account instruction: pubkey={}: {}",
    //                            ata, err
    //                        ))
    //                    })?;

    //                    instructions.push(close_instruction);
    //                } else {
    //                    // Log warning but don't fail the transaction if account doesn't exist
    //                    eprintln!(
    //                        "Warning: Cannot close token account {}, it doesn't exist",
    //                        ata
    //                    );
    //                }
    //            }
    //        } else {
    //            // This case should not occur due to our balance fetch logic,
    //            // but handle it gracefully just in case
    //            eprintln!("Warning: Token balance unavailable, not closing account");
    //        }
    //    }

    //    Ok(instructions)
    //}

    /// Gets the Program Derived Address (PDA) for the global state account
    ///
    /// Derives the address of the global state account using the program ID and a
    /// constant seed. The global state account contains program-wide configuration
    /// such as fee settings and fee recipient.
    ///
    /// # Returns
    ///
    /// Returns the PDA public key derived from the GLOBAL_SEED
    ///
    /// # Examples
    ///
    /// ```
    /// # use solana_sub::parser::pumpfun2::PumpFun;
    /// # use solana_sdk::pubkey::Pubkey;
    /// #
    /// let global_pda: Pubkey = PumpFun::get_global_pda();
    /// println!("Global state account: {}", global_pda);
    /// ```
    pub fn get_global_pda() -> Pubkey {
        let seeds: &[&[u8]; 1] = &[constants::seeds::GLOBAL_SEED];
        let program_id: &Pubkey = &constants::accounts::PUMPFUN;
        Pubkey::find_program_address(seeds, program_id).0
    }

    /// Gets the Program Derived Address (PDA) for the mint authority
    ///
    /// Derives the address of the mint authority PDA using the program ID and a
    /// constant seed. The mint authority PDA is the authority that can mint new
    /// tokens for any token created through the Pump.fun program.
    ///
    /// # Returns
    ///
    /// Returns the PDA public key derived from the MINT_AUTHORITY_SEED
    ///
    /// # Examples
    ///
    /// ```
    /// # use solana_sub::parser::pumpfun2::PumpFun;
    /// # use solana_sdk::pubkey::Pubkey;
    /// #
    /// let mint_authority: Pubkey = PumpFun::get_mint_authority_pda();
    /// println!("Mint authority account: {}", mint_authority);
    /// ```
    pub fn get_mint_authority_pda() -> Pubkey {
        let seeds: &[&[u8]; 1] = &[constants::seeds::MINT_AUTHORITY_SEED];
        let program_id: &Pubkey = &constants::accounts::PUMPFUN;
        Pubkey::find_program_address(seeds, program_id).0
    }

    /// Gets the Program Derived Address (PDA) for a token's bonding curve account
    ///
    /// Derives the address of a token's bonding curve account using the program ID,
    /// a constant seed, and the token mint address. The bonding curve account stores
    /// the state and parameters that govern the token's price dynamics.
    ///
    /// # Arguments
    ///
    /// * `mint` - Public key of the token mint
    ///
    /// # Returns
    ///
    /// Returns Some(PDA) if derivation succeeds, or None if it fails
    ///
    /// # Examples
    ///
    /// ```
    /// # use solana_sub::parser::pumpfun2::PumpFun;
    /// # use solana_sdk::{pubkey, pubkey::Pubkey};
    /// #
    /// let mint = pubkey!("TokenM1ntPubk3yXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
    /// if let Some(bonding_curve) = PumpFun::get_bonding_curve_pda(&mint) {
    ///     println!("Bonding curve account: {}", bonding_curve);
    /// }
    /// ```
    pub fn get_bonding_curve_pda(mint: &Pubkey) -> Option<Pubkey> {
        let seeds: &[&[u8]; 2] = &[constants::seeds::BONDING_CURVE_SEED, mint.as_ref()];
        let program_id: &Pubkey = &constants::accounts::PUMPFUN;
        let pda: Option<(Pubkey, u8)> = Pubkey::try_find_program_address(seeds, program_id);
        pda.map(|pubkey| pubkey.0)
    }

    ///// Gets the Program Derived Address (PDA) for a token's metadata account
    /////
    ///// Derives the address of a token's metadata account following the Metaplex Token Metadata
    ///// standard. The metadata account stores information about the token such as name,
    ///// symbol, and URI pointing to additional metadata.
    /////
    ///// # Arguments
    /////
    ///// * `mint` - Public key of the token mint
    /////
    ///// # Returns
    /////
    ///// Returns the PDA public key for the token's metadata account
    /////
    ///// # Examples
    /////
    ///// ```
    ///// # use txn_maker::pumpfun2::PumpFun;
    ///// # use solana_sdk::{pubkey, pubkey::Pubkey};
    ///// #
    ///// let mint = pubkey!("TokenM1ntPubk3yXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
    ///// let metadata_pda = PumpFun::get_metadata_pda(&mint);
    ///// println!("Token metadata account: {}", metadata_pda);
    ///// ```
    pub fn get_metadata_pda(mint: &Pubkey) -> Pubkey {
        let seeds: &[&[u8]; 3] = &[
            constants::seeds::METADATA_SEED,
            constants::accounts::MPL_TOKEN_METADATA.as_ref(),
            mint.as_ref(),
        ];
        let program_id: &Pubkey = &constants::accounts::MPL_TOKEN_METADATA;
        Pubkey::find_program_address(seeds, program_id).0
    }

    //// Gets the global state account data containing program-wide configuration
    ////
    //// Fetches and deserializes the global state account which contains program-wide
    //// configuration parameters such as:
    //// - Fee basis points for trading
    //// - Fee recipient account
    //// - Bonding curve parameters
    //// - Other platform-wide settings
    ////
    //// # Returns
    ////
    //// Returns the deserialized GlobalAccount if successful, or a ClientError if the operation fails
    ////
    //// # Errors
    ////
    //// Returns an error if:
    //// - The account cannot be found on-chain
    //// - The account data cannot be properly deserialized
    ////
    //// # Examples
    ////
    //// ```no_run
    //// # use txn_maker::pumpfun2::{PumpFun, common::types::{Cluster, PriorityFee}};
    //// # use solana_sdk::{commitment_config::CommitmentConfig, signature::Keypair};
    //// # use std::sync::Arc;
    //// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    //// # let payer = Arc::new(Keypair::new());
    //// # let commitment = CommitmentConfig::confirmed();
    //// # let cluster = Cluster::devnet(commitment, PriorityFee::default());
    //// # let client = PumpFun::new(payer, cluster);
    //// let global = client.get_global_account().await?;
    //// println!("Fee basis points: {}", global.fee_basis_points);
    //// println!("Fee recipient: {}", global.fee_recipient);
    //// # Ok(())
    //// # }
    /// ```
    pub async fn get_global_account(&self) -> Result<accounts::GlobalAccount, error::ClientError> {
        let global: Pubkey = Self::get_global_pda();

        let account = self
            .rpc
            .get_account(&global)
            .await
            .map_err(error::ClientError::SolanaClientError)?;

        solana_sdk::borsh1::try_from_slice_unchecked::<accounts::GlobalAccount>(&account.data)
            .map_err(error::ClientError::BorshError)
    }

    //// Gets a token's bonding curve account data containing pricing parameters
    ////
    //// Fetches and deserializes a token's bonding curve account which contains the
    //// state and parameters that determine the token's price dynamics, including:
    //// - Current supply
    //// - Reserve balance
    //// - Bonding curve parameters
    //// - Other token-specific configuration
    ////
    //// # Arguments
    ////
    //// * `mint` - Public key of the token mint
    ////
    //// # Returns
    ////
    //// Returns the deserialized BondingCurveAccount if successful, or a ClientError if the operation fails
    ////
    //// # Errors
    ////
    //// Returns an error if:
    //// - The bonding curve PDA cannot be derived
    //// - The account cannot be found on-chain
    //// - The account data cannot be properly deserialized
    ////
    //// # Examples
    ////
    //// ```no_run
    //// # use txn_maker::pumpfun2::{PumpFun, common::types::{Cluster, PriorityFee}};
    //// # use solana_sdk::{commitment_config::CommitmentConfig, signature::Keypair, pubkey};
    //// # use std::sync::Arc;
    //// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    //// # let payer = Arc::new(Keypair::new());
    //// # let commitment = CommitmentConfig::confirmed();
    //// # let cluster = Cluster::devnet(commitment, PriorityFee::default());
    //// # let client = PumpFun::new(payer, cluster);
    //// let mint = pubkey!("TokenM1ntPubk3yXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
    //// let bonding_curve = client.get_bonding_curve_account(&mint).await?;
    //// println!("Bonding Curve Account: {:#?}", bonding_curve);
    //// # Ok(())
    //// # }
    /// ```
    pub async fn get_bonding_curve_account(
        &self,
        mint: &Pubkey,
    ) -> Result<accounts::BondingCurveAccount, error::ClientError> {
        let bonding_curve_pda =
            Self::get_bonding_curve_pda(mint).ok_or(error::ClientError::BondingCurveNotFound)?;

        let account = self
            .rpc
            .get_account(&bonding_curve_pda)
            .await
            .map_err(error::ClientError::SolanaClientError)?;

        solana_sdk::borsh1::try_from_slice_unchecked::<accounts::BondingCurveAccount>(&account.data)
            .map_err(error::ClientError::BorshError)
    }
}

pub fn process_create_event(
    ctx: &mut TxnDecodeCtx,
    cmp_insn: (usize, &CompiledInstruction),
    sig: &str,
) -> anyhow::Result<TokenCreationEvent> {
    let (ix_index, ix) = cmp_insn;

    if ix.accounts.len() == 14
        && ix.data.len() >= 8
        && ix.data.as_slice()[..8] == Create::DISCRIMINATOR
    {
        let create = Create::try_from_slice(&ix.data.as_slice()[8..])?;

        tracing::debug!(?create, "PumpFun create events ix params");

        let mint = super::parser::TransactionParser::lookup_addr(ctx, 0)
            .ok_or(anyhow::anyhow!("Mint not found"))?;

        let bonding_curve = super::parser::TransactionParser::lookup_addr(ctx, 2)
            .ok_or(anyhow::anyhow!("Bonding curve not found"))?;

        let user = super::parser::TransactionParser::lookup_addr(ctx, 7)
            .ok_or(anyhow::anyhow!("User not found"))?;

        let msg = &ctx.bctx.ver_txn.message;

        let initial_buy = msg
            .instructions()
            .iter()
            .find(|ix| {
                if let Some(ix_program_id) =
                    super::parser::TransactionParser::lookup_addr(ctx, ix.program_id_index)
                    && ix_program_id == constants::accounts::PUMPFUN
                    && ix.accounts.len() == 12
                    && ix.data.len() >= 8
                    && ix.data.as_slice()[..8] == Buy::DISCRIMINATOR
                {
                    true
                } else {
                    false
                }
            })
            .map_or(0.0f64, |ix| {
                let buy = Buy::try_from_slice(&ix.data.as_slice()[8..]).unwrap();
                buy.amount as f64
            });

        let inners = ctx
            .bctx
            .meta
            .inner_instructions
            .iter()
            .find(|inner| inner.index == ix_index as u32)
            .ok_or(anyhow::anyhow!("Inner instructions not found"))?;

        let pfun_create_log = inners
            .instructions
            .iter()
            .find(|iix| {
                let program_id =
                    super::parser::TransactionParser::lookup_addr(ctx, iix.program_id_index as u8);

                if let Some(program_id) = program_id
                    && program_id == constants::accounts::PUMPFUN
                {
                    if iix.accounts.len() == 1
                        && iix.data.len() >= 16
                        && iix.data.as_slice()[..16] == PUMPFUN_CREATE_LOG_DISCRIMINATOR
                    {
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .ok_or(anyhow::anyhow!("PFUN create log not found"))?;

        let create_log = PumpfunCreateLog::try_from_slice(&pfun_create_log.data.as_slice()[16..])
            .map_err(|_| anyhow::anyhow!("Failed to parse create log"))?;

        tracing::debug!(?create_log);

        Ok(TokenCreationEvent {
            signature: sig.to_owned(),
            mint: mint.to_string(),
            trader_public_key: user.to_string(),
            tx_type: TradeEventType::Create,
            initial_buy: initial_buy,
            bonding_curve_key: bonding_curve.to_string(),
            v_tokens_in_bonding_curve: create_log.virtual_token_reserves as f64,
            v_sol_in_bonding_curve: create_log.virtual_sol_reserves as f64,
            market_cap_sol: 0.0f64, // https://t.me/c/2590677350/1/87
            name: create.name,
            symbol: create.symbol,
            uri: create.uri,
            timestamp: chrono::Utc::now(),
            dex: popeyes_trading_types::Dex::PumpFun,
            slot: ctx.bctx.cur_slot,
        })
    } else {
        Err(anyhow::anyhow!("Not a valid Create instruction"))
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use solana_sdk::{message::VersionedMessage, transaction::VersionedTransaction};
    use yellowstone_grpc_proto::prelude::{
        InnerInstruction, InnerInstructions, TokenBalance, TransactionStatusMeta, UiTokenAmount,
    };

    use super::*;
    use crate::parser::BlockDecodeCtx;

    #[test]
    fn test_create_event() {
        let rpc_url = std::env::var("RPC_URL")
            .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());

        let rpc_client = solana_client::rpc_client::RpcClient::new(rpc_url);

        let raw_ver_msg: [u8; 891] = [
            128, 2, 0, 9, 19, 228, 157, 69, 147, 27, 49, 236, 87, 194, 159, 186, 194, 64, 107, 210,
            29, 78, 73, 44, 57, 128, 179, 174, 106, 250, 242, 122, 197, 47, 242, 136, 81, 71, 25,
            144, 197, 181, 43, 93, 229, 126, 153, 198, 99, 77, 11, 201, 201, 175, 29, 61, 20, 25,
            13, 146, 52, 56, 156, 86, 99, 95, 162, 60, 95, 5, 139, 160, 109, 108, 69, 66, 216, 239,
            152, 99, 112, 26, 184, 158, 18, 248, 6, 18, 31, 66, 111, 32, 225, 85, 96, 185, 238, 37,
            56, 89, 90, 6, 197, 193, 206, 99, 141, 37, 103, 210, 100, 104, 176, 94, 185, 81, 209,
            162, 141, 204, 110, 18, 52, 130, 181, 198, 117, 20, 151, 112, 230, 43, 242, 73, 136,
            218, 196, 31, 99, 232, 46, 39, 65, 163, 194, 36, 124, 58, 4, 26, 77, 36, 247, 25, 13,
            87, 109, 44, 153, 252, 1, 144, 250, 217, 195, 74, 194, 248, 208, 221, 92, 188, 151,
            227, 40, 156, 25, 124, 181, 6, 42, 84, 243, 217, 86, 185, 206, 110, 81, 21, 249, 101,
            103, 170, 92, 179, 230, 80, 21, 218, 18, 205, 145, 192, 178, 13, 48, 73, 16, 164, 235,
            12, 255, 156, 183, 9, 41, 123, 7, 107, 52, 250, 191, 44, 197, 240, 231, 187, 15, 84,
            188, 126, 39, 52, 161, 7, 146, 18, 162, 224, 249, 178, 238, 30, 173, 123, 242, 37, 6,
            36, 88, 58, 240, 85, 48, 202, 118, 173, 100, 23, 201, 208, 117, 213, 145, 191, 49, 217,
            113, 237, 23, 216, 99, 107, 114, 195, 158, 41, 40, 251, 53, 243, 125, 9, 63, 17, 77,
            204, 152, 179, 230, 44, 2, 250, 9, 42, 165, 199, 165, 42, 194, 153, 192, 208, 211, 174,
            9, 34, 134, 120, 212, 82, 34, 230, 61, 143, 40, 96, 238, 1, 187, 144, 142, 142, 176, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 1, 86, 224, 246, 147, 102, 90, 207, 68, 219, 21, 104, 191, 23, 91, 170, 81, 137,
            203, 151, 245, 210, 255, 59, 101, 93, 43, 182, 253, 109, 24, 176, 3, 6, 70, 111, 229,
            33, 23, 50, 255, 236, 173, 186, 114, 195, 155, 231, 188, 140, 229, 187, 197, 247, 18,
            107, 44, 67, 155, 58, 64, 0, 0, 0, 6, 167, 213, 23, 25, 44, 92, 81, 33, 140, 201, 76,
            61, 74, 241, 127, 88, 218, 238, 8, 155, 161, 253, 68, 227, 219, 217, 138, 0, 0, 0, 0,
            6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172, 28, 180,
            133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169, 11, 112, 101, 177, 227,
            209, 124, 69, 56, 157, 82, 127, 107, 4, 195, 205, 88, 184, 108, 115, 26, 160, 253, 181,
            73, 182, 209, 188, 3, 248, 41, 70, 58, 134, 94, 105, 238, 15, 84, 128, 202, 188, 246,
            99, 87, 228, 220, 47, 24, 213, 141, 69, 193, 234, 116, 137, 251, 55, 35, 217, 121, 60,
            114, 166, 140, 151, 37, 143, 78, 36, 137, 241, 187, 61, 16, 41, 20, 142, 13, 131, 11,
            90, 19, 153, 218, 255, 16, 132, 4, 142, 123, 216, 219, 233, 248, 89, 172, 241, 54, 235,
            1, 252, 28, 78, 136, 61, 35, 200, 181, 132, 74, 181, 154, 55, 246, 106, 221, 87, 197,
            233, 172, 59, 83, 224, 89, 211, 92, 100, 93, 157, 65, 216, 169, 56, 70, 146, 41, 253,
            99, 251, 180, 7, 171, 65, 210, 243, 124, 90, 143, 205, 151, 66, 118, 5, 240, 167, 182,
            156, 15, 235, 6, 12, 0, 9, 3, 0, 9, 61, 0, 0, 0, 0, 0, 12, 0, 5, 2, 208, 221, 6, 0, 10,
            2, 0, 2, 12, 2, 0, 0, 0, 24, 101, 154, 0, 0, 0, 0, 0, 11, 14, 1, 3, 6, 7, 16, 15, 9, 0,
            10, 14, 17, 13, 18, 11, 140, 1, 24, 30, 200, 40, 5, 28, 7, 119, 16, 0, 0, 0, 69, 85,
            32, 70, 105, 114, 115, 116, 32, 66, 105, 116, 99, 111, 105, 110, 5, 0, 0, 0, 69, 85,
            66, 84, 67, 67, 0, 0, 0, 104, 116, 116, 112, 115, 58, 47, 47, 105, 112, 102, 115, 46,
            105, 111, 47, 105, 112, 102, 115, 47, 81, 109, 90, 83, 121, 74, 57, 78, 57, 118, 106,
            118, 57, 119, 53, 117, 49, 90, 111, 89, 89, 112, 99, 50, 121, 101, 83, 50, 87, 80, 109,
            54, 101, 55, 88, 85, 55, 111, 107, 115, 104, 72, 69, 98, 115, 88, 228, 157, 69, 147,
            27, 49, 236, 87, 194, 159, 186, 194, 64, 107, 210, 29, 78, 73, 44, 57, 128, 179, 174,
            106, 250, 242, 122, 197, 47, 242, 136, 81, 17, 6, 0, 8, 0, 1, 10, 14, 1, 0, 11, 12, 16,
            5, 1, 6, 7, 8, 0, 10, 14, 4, 18, 11, 24, 102, 6, 61, 18, 1, 218, 235, 234, 46, 234, 42,
            146, 183, 88, 0, 0, 0, 1, 178, 196, 0, 0, 0, 0, 0,
        ];

        let ver_msg: VersionedMessage = bincode::deserialize(raw_ver_msg.as_slice())
            .expect("Failed to deserialize VersionedMessage");

        eprintln!("ver_msg: {:#?}", ver_msg);

        let sig = "5kBsGH9FGBCSAtmpvQ3kU6TpqcXcCrkMk2TQRJSyYq8f7cH1YQ2HzXumFvZAa7ZC6VoCy3Sg4bdwn13ZfPyovi89";

        let signatures = vec![solana_sdk::signature::Signature::from_str(sig).unwrap()];

        let ver_txn = VersionedTransaction {
            signatures,
            message: ver_msg,
        };

        eprintln!("ver_txn: {:#?}", ver_txn);

        let static_keys = vec![
            "GPR3jG8g5Bk1h64f5pk2vhvbQychPaeXcZR4tZPYPPxc",
            "5nYZJ82u3uxRieV4e34jBUiiGL9JgyxT1eRMk4xppump",
            "NeXTBLoCKs9F1y5PJS9CKrFNNLU1keHW71rfh7KgA1X",
            "TSLvdd1pWpHVjahSpsvCXUbgwsL3JAcvokwaKt1eokM",
            "5x3oDS29uxKVbj43frXjTTuoGSq2JwEg3EhnbxUaPzf4",
            "62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV",
            "6PcwcuPq8B9rjMcnytpCK69MkSddqmvp4ZEJXepPNhEv",
            "6hmx8my5wCAQ3TLEYuHmQpNB6mt2k9qQPeVkXkB3bXEp",
            "F2k1qGGJfJvPSTWxQ8eUG8HdBA1KCzukDF7M7DkZ7Dso",
            "Hq32phakCBUXxajkh6sM1DgJkVi3SH6veHxpnJfo47EF",
            "11111111111111111111111111111111",
            "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",
            "ComputeBudget111111111111111111111111111111",
            "SysvarRent111111111111111111111111111111111",
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
            "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s",
            "4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf",
            "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
            "Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1",
        ]
        .iter()
        .map(|s| Pubkey::from_str(s).unwrap())
        .collect::<Vec<Pubkey>>();

        let addr_lookups = Vec::new();

        let inner_instructions = vec![
            InnerInstructions {
                index: 3,
                instructions: vec![
                    InnerInstruction {
                        program_id_index: 10,
                        accounts: vec![0, 1],
                        data: vec![
                            0, 0, 0, 0, 96, 77, 22, 0, 0, 0, 0, 0, 82, 0, 0, 0, 0, 0, 0, 0, 6, 221,
                            246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172,
                            28, 180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0,
                            169,
                        ],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 14,
                        accounts: vec![1],
                        data: vec![
                            20, 6, 6, 197, 193, 206, 99, 141, 37, 103, 210, 100, 104, 176, 94, 185,
                            81, 209, 162, 141, 204, 110, 18, 52, 130, 181, 198, 117, 20, 151, 112,
                            230, 43, 242, 0,
                        ],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 10,
                        accounts: vec![0, 6],
                        data: vec![
                            0, 0, 0, 0, 48, 50, 22, 0, 0, 0, 0, 0, 81, 0, 0, 0, 0, 0, 0, 0, 1, 86,
                            224, 246, 147, 102, 90, 207, 68, 219, 21, 104, 191, 23, 91, 170, 81,
                            137, 203, 151, 245, 210, 255, 59, 101, 93, 43, 182, 253, 109, 24, 176,
                        ],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 17,
                        accounts: vec![0, 7, 6, 1, 10, 14],
                        data: vec![0],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 14,
                        accounts: vec![1],
                        data: vec![21, 7, 0],
                        stack_height: Some(3),
                    },
                    InnerInstruction {
                        program_id_index: 10,
                        accounts: vec![0, 7],
                        data: vec![
                            0, 0, 0, 0, 240, 29, 31, 0, 0, 0, 0, 0, 165, 0, 0, 0, 0, 0, 0, 0, 6,
                            221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121,
                            172, 28, 180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255,
                            0, 169,
                        ],

                        stack_height: Some(3),
                    },
                    InnerInstruction {
                        program_id_index: 14,
                        accounts: vec![7],
                        data: vec![22],
                        stack_height: Some(3),
                    },
                    InnerInstruction {
                        program_id_index: 14,
                        accounts: vec![7, 1],
                        data: vec![
                            18, 80, 21, 218, 18, 205, 145, 192, 178, 13, 48, 73, 16, 164, 235, 12,
                            255, 156, 183, 9, 41, 123, 7, 107, 52, 250, 191, 44, 197, 240, 231,
                            187, 15,
                        ],
                        stack_height: Some(3),
                    },
                    InnerInstruction {
                        program_id_index: 15,
                        accounts: vec![9, 1, 3, 0, 3, 10],
                        data: vec![
                            33, 16, 0, 0, 0, 69, 85, 32, 70, 105, 114, 115, 116, 32, 66, 105, 116,
                            99, 111, 105, 110, 5, 0, 0, 0, 69, 85, 66, 84, 67, 67, 0, 0, 0, 104,
                            116, 116, 112, 115, 58, 47, 47, 105, 112, 102, 115, 46, 105, 111, 47,
                            105, 112, 102, 115, 47, 81, 109, 90, 83, 121, 74, 57, 78, 57, 118, 106,
                            118, 57, 119, 53, 117, 49, 90, 111, 89, 89, 112, 99, 50, 121, 101, 83,
                            50, 87, 80, 109, 54, 101, 55, 88, 85, 55, 111, 107, 115, 104, 72, 69,
                            98, 115, 88, 0, 0, 1, 1, 0, 0, 0, 228, 157, 69, 147, 27, 49, 236, 87,
                            194, 159, 186, 194, 64, 107, 210, 29, 78, 73, 44, 57, 128, 179, 174,
                            106, 250, 242, 122, 197, 47, 242, 136, 81, 0, 100, 0, 0, 0, 0,
                        ],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 10,
                        accounts: vec![0, 9],
                        data: vec![2, 0, 0, 0, 80, 165, 230, 0, 0, 0, 0, 0],

                        stack_height: Some(3),
                    },
                    InnerInstruction {
                        program_id_index: 10,
                        accounts: vec![9],
                        data: vec![8, 0, 0, 0, 95, 2, 0, 0, 0, 0, 0, 0],
                        stack_height: Some(3),
                    },
                    InnerInstruction {
                        program_id_index: 10,
                        accounts: vec![9],
                        data: vec![
                            1, 0, 0, 0, 11, 112, 101, 177, 227, 209, 124, 69, 56, 157, 82, 127,
                            107, 4, 195, 205, 88, 184, 108, 115, 26, 160, 253, 181, 73, 182, 209,
                            188, 3, 248, 41, 70,
                        ],

                        stack_height: Some(3),
                    },
                    InnerInstruction {
                        program_id_index: 14,
                        accounts: vec![1, 7, 3],
                        data: vec![7, 0, 128, 198, 164, 126, 141, 3, 0],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 14,
                        accounts: vec![1, 3],
                        data: vec![6, 0, 0],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 11,
                        accounts: vec![18],
                        data: vec![
                            228, 69, 165, 46, 81, 203, 154, 29, 27, 114, 169, 77, 222, 235, 99,
                            118, 16, 0, 0, 0, 69, 85, 32, 70, 105, 114, 115, 116, 32, 66, 105, 116,
                            99, 111, 105, 110, 5, 0, 0, 0, 69, 85, 66, 84, 67, 67, 0, 0, 0, 104,
                            116, 116, 112, 115, 58, 47, 47, 105, 112, 102, 115, 46, 105, 111, 47,
                            105, 112, 102, 115, 47, 81, 109, 90, 83, 121, 74, 57, 78, 57, 118, 106,
                            118, 57, 119, 53, 117, 49, 90, 111, 89, 89, 112, 99, 50, 121, 101, 83,
                            50, 87, 80, 109, 54, 101, 55, 88, 85, 55, 111, 107, 115, 104, 72, 69,
                            98, 115, 88, 71, 25, 144, 197, 181, 43, 93, 229, 126, 153, 198, 99, 77,
                            11, 201, 201, 175, 29, 61, 20, 25, 13, 146, 52, 56, 156, 86, 99, 95,
                            162, 60, 95, 80, 21, 218, 18, 205, 145, 192, 178, 13, 48, 73, 16, 164,
                            235, 12, 255, 156, 183, 9, 41, 123, 7, 107, 52, 250, 191, 44, 197, 240,
                            231, 187, 15, 228, 157, 69, 147, 27, 49, 236, 87, 194, 159, 186, 194,
                            64, 107, 210, 29, 78, 73, 44, 57, 128, 179, 174, 106, 250, 242, 122,
                            197, 47, 242, 136, 81, 228, 157, 69, 147, 27, 49, 236, 87, 194, 159,
                            186, 194, 64, 107, 210, 29, 78, 73, 44, 57, 128, 179, 174, 106, 250,
                            242, 122, 197, 47, 242, 136, 81, 12, 189, 48, 104, 0, 0, 0, 0, 0, 16,
                            216, 71, 227, 207, 3, 0, 0, 172, 35, 252, 6, 0, 0, 0, 0, 120, 197, 251,
                            81, 209, 2, 0, 0, 128, 198, 164, 126, 141, 3, 0,
                        ],
                        stack_height: Some(2),
                    },
                ],
            },
            InnerInstructions {
                index: 4,
                instructions: vec![
                    InnerInstruction {
                        program_id_index: 14,
                        accounts: vec![1],
                        data: vec![21, 7, 0],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 10,
                        accounts: vec![0, 8],
                        data: vec![
                            0, 0, 0, 0, 240, 29, 31, 0, 0, 0, 0, 0, 165, 0, 0, 0, 0, 0, 0, 0, 6,
                            221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121,
                            172, 28, 180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255,
                            0, 169,
                        ],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 14,
                        accounts: vec![8],
                        data: vec![22],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 14,
                        accounts: vec![8, 1],
                        data: vec![
                            18, 228, 157, 69, 147, 27, 49, 236, 87, 194, 159, 186, 194, 64, 107,
                            210, 29, 78, 73, 44, 57, 128, 179, 174, 106, 250, 242, 122, 197, 47,
                            242, 136, 81,
                        ],
                        stack_height: Some(2),
                    },
                ],
            },
            InnerInstructions {
                index: 5,
                instructions: vec![
                    InnerInstruction {
                        program_id_index: 14,
                        accounts: vec![7, 8, 6],
                        data: vec![3, 46, 234, 42, 146, 183, 88, 0, 0],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 10,
                        accounts: vec![0, 4],
                        data: vec![2, 0, 0, 0, 96, 227, 22, 0, 0, 0, 0, 0],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 10,
                        accounts: vec![0, 6],
                        data: vec![2, 0, 0, 0, 96, 227, 22, 0, 0, 0, 0, 0],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 10,
                        accounts: vec![0, 5],
                        data: vec![2, 0, 0, 0, 32, 224, 178, 1, 0, 0, 0, 0],
                        stack_height: Some(2),
                    },
                    InnerInstruction {
                        program_id_index: 11,
                        accounts: vec![18],
                        data: vec![
                            228, 69, 165, 46, 81, 203, 154, 29, 189, 219, 127, 211, 78, 230, 97,
                            238, 71, 25, 144, 197, 181, 43, 93, 229, 126, 153, 198, 99, 77, 11,
                            201, 201, 175, 29, 61, 20, 25, 13, 146, 52, 56, 156, 86, 99, 95, 162,
                            60, 95, 0, 94, 208, 178, 0, 0, 0, 0, 46, 234, 42, 146, 183, 88, 0, 0,
                            1, 228, 157, 69, 147, 27, 49, 236, 87, 194, 159, 186, 194, 64, 107,
                            210, 29, 78, 73, 44, 57, 128, 179, 174, 106, 250, 242, 122, 197, 47,
                            242, 136, 81, 12, 189, 48, 104, 0, 0, 0, 0, 0, 10, 244, 174, 7, 0, 0,
                            0, 210, 37, 173, 181, 43, 119, 3, 0, 0, 94, 208, 178, 0, 0, 0, 0, 210,
                            141, 154, 105, 154, 120, 2, 0, 74, 194, 248, 208, 221, 92, 188, 151,
                            227, 40, 156, 25, 124, 181, 6, 42, 84, 243, 217, 86, 185, 206, 110, 81,
                            21, 249, 101, 103, 170, 92, 179, 230, 95, 0, 0, 0, 0, 0, 0, 0, 32, 224,
                            178, 1, 0, 0, 0, 0, 228, 157, 69, 147, 27, 49, 236, 87, 194, 159, 186,
                            194, 64, 107, 210, 29, 78, 73, 44, 57, 128, 179, 174, 106, 250, 242,
                            122, 197, 47, 242, 136, 81, 5, 0, 0, 0, 0, 0, 0, 0, 96, 227, 22, 0, 0,
                            0, 0, 0,
                        ],
                        stack_height: Some(2),
                    },
                ],
            },
        ];

        let meta = TransactionStatusMeta {
            err: None,
            fee: 1810000,
            pre_balances: vec![
                4909409306,
                0,
                150900004,
                475958901,
                206532253,
                56438298102282,
                0,
                0,
                0,
                0,
                1,
                1141440,
                1,
                1009200,
                934087680,
                1141440,
                290898417,
                731913600,
                137104014,
            ],
            post_balances: vec![
                1845370482,
                1461600,
                161018428,
                475958901,
                208032253,
                56438326602282,
                3001454640,
                2039280,
                2039280,
                15115600,
                1,
                1141440,
                1,
                1009200,
                934087680,
                1141440,
                290898417,
                731913600,
                137104014,
            ],
            inner_instructions,
            inner_instructions_none: false,
            log_messages: vec![],
            log_messages_none: false,
            pre_token_balances: vec![],
            post_token_balances: vec![
                TokenBalance {
                    account_index: 7,
                    mint: "5nYZJ82u3uxRieV4e34jBUiiGL9JgyxT1eRMk4xppump".into(),
                    ui_token_amount: Some(UiTokenAmount {
                        ui_amount: 902454545.454546,
                        decimals: 6,
                        amount: "902454545454546".into(),
                        ui_amount_string: "902454545.454546".into(),
                    }),
                    owner: "6PcwcuPq8B9rjMcnytpCK69MkSddqmvp4ZEJXepPNhEv".into(),
                    program_id: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".into(),
                },
                TokenBalance {
                    account_index: 8,
                    mint: "5nYZJ82u3uxRieV4e34jBUiiGL9JgyxT1eRMk4xppump".into(),
                    ui_token_amount: Some(UiTokenAmount {
                        ui_amount: 97545454.545454,
                        decimals: 6,
                        amount: "97545454545454".into(),
                        ui_amount_string: "97545454.545454".into(),
                    }),
                    owner: "GPR3jG8g5Bk1h64f5pk2vhvbQychPaeXcZR4tZPYPPxc".into(),
                    program_id: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".into(),
                },
            ],
            rewards: vec![],
            loaded_writable_addresses: vec![],
            loaded_readonly_addresses: vec![],
            return_data: None,
            return_data_none: true,
            compute_units_consumed: Some(200661),
        };

        let mut ctx = TxnDecodeCtx {
            bctx: &BlockDecodeCtx {
                conn: &rpc_client,
                ver_txn: &ver_txn,
                meta: &meta,
                cur_slot: 341980546,
                tx_idx: 188,
            },
            static_keys: &static_keys,
            addr_lookups: &Some(&addr_lookups),
            txn_lut_cache: &mut None,
        };

        eprintln!("ctx: {:#?}", ctx);

        let ins_data_raw = "181ec828051c077710000000455520466972737420426974636f696e0500000045554254434300000068747470733a2f2f697066732e696f2f697066732f516d5a53794a394e39766a7639773575315a6f59597063327965533257506d3665375855376f6b73684845627358e49d45931b31ec57c29fbac2406bd21d4e492c3980b3ae6afaf27ac52ff28851";

        let cmp_insn = (
            3,
            &CompiledInstruction {
                program_id_index: 11,
                accounts: vec![1, 3, 6, 7, 16, 15, 9, 0, 10, 14, 17, 13, 18, 11],
                data: (0..ins_data_raw.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&ins_data_raw[i..i + 2], 16).unwrap())
                    .collect::<Vec<u8>>(),
            },
        );

        let result = process_create_event(&mut ctx, cmp_insn, sig);

        eprintln!("result: {:#?}", result);

        assert!(result.is_ok());

        let result = result.unwrap();

        assert_eq!(result.signature, sig);
        assert_eq!(result.mint, "GPR3jG8g5Bk1h64f5pk2vhvbQychPaeXcZR4tZPYPPxc");
        assert_eq!(
            result.trader_public_key,
            "6hmx8my5wCAQ3TLEYuHmQpNB6mt2k9qQPeVkXkB3bXEp"
        );
        assert_eq!(result.tx_type, TradeEventType::Create);
        assert_eq!(result.initial_buy, 97545454545454.0f64);
        assert_eq!(
            result.bonding_curve_key,
            "NeXTBLoCKs9F1y5PJS9CKrFNNLU1keHW71rfh7KgA1X"
        );
        assert_eq!(result.v_tokens_in_bonding_curve, 1073000000000000.0f64);
        assert_eq!(result.v_sol_in_bonding_curve, 30000000000.0f64);
        assert_eq!(result.market_cap_sol, 0.0f64);
        assert_eq!(result.name, "EU First Bitcoin");
        assert_eq!(result.symbol, "EUBTC");
        assert_eq!(
            result.uri,
            "https://ipfs.io/ipfs/QmZSyJ9N9vjv9w5u1ZoYYpc2yeS2WPm6e7XU7okshHEbsX"
        );
        assert_eq!(result.dex, popeyes_trading_types::Dex::PumpFun);
    }
}
