use std::{collections::HashSet, sync::Arc, time::Duration};

use crate::leader_monitor::LeaderMonitorService;
use anyhow::Result;
use prometheus::Registry;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Signer, signer::keypair::Keypair};
use solana_sub::redis_tx_retriever::RedisTxRetriever;
use tracing::info;

use super::{
    confirmation::GrpcConfirmationMonitor,
    executor::SolanaOrderExecutor,
    signing::LocalKeypairSigningService,
    submission::{JitoMetrics, TransactionService, registry as submitter_registry},
    tx_constructor::{
        ShadowTxnMakerClient, TcpTxnMakerClient, TransactionConstructor, TxnMakerClient,
        TxnMakerTransactionConstructor, UnixSocketTxnMakerClient,
    },
};
use crate::execution::events::SimulationMode;

/// Builder for constructing a SolanaOrderExecutor with flexible configuration
#[derive(Debug)]
pub struct SolanaOrderExecutorBuilder {
    signer_keypair: Keypair,
    rpc_url: String,
    buy_slippage: f64,
    sell_slippage: f64,
    sim_buy_slippage: f64,
    sim_sell_slippage: f64,
    simulation_mode: SimulationMode,
    max_real_trades: Option<u32>,
    skip_simulation_when_sell: bool,
    skip_simulation_for_buy: bool,

    compute_unit_price: Option<u64>,
    compute_unit_limit: Option<u32>,

    txn_maker_url: Option<String>,
    txn_maker_connection_type: Option<String>,
    txn_maker_unix_socket_path: Option<String>,

    // Jito specific
    jito_block_engine_url: Option<String>,
    jito_api_key: Option<String>,
    jito_buy_tip: Option<u64>, // Optional tip in lamports for Jito BUY bundles
    jito_sell_tip: Option<u64>, // Optional tip in lamports for Jito SELL bundles
    max_slot_latency: Option<u64>,
    jito_metrics: Option<JitoMetrics>,

    // Bloxroute specific
    bloxroute_api_url: Option<String>,
    bloxroute_auth_header: Option<String>,
    bloxroute_tip: Option<u64>,

    // ZeroSlot specific
    zeroslot_api_url: Option<String>,
    zeroslot_api_key: Option<String>,
    zeroslot_tip: Option<u64>,
    confirmation_timeout: Option<Duration>, // General timeout for transaction confirmation across submitters

    // Transaction submitter type
    transaction_submitter_type: Option<String>,

    // Redis transaction retriever
    redis_tx_url: Option<String>,
    redis_tx_key_prefix: Option<String>,

    // Leader Monitor Config
    bad_leader_validators: Option<HashSet<Pubkey>>,
    leader_monitor_refresh_interval_secs: Option<u64>,
}

impl SolanaOrderExecutorBuilder {
    pub fn jito_block_engine_url(&self) -> Option<&str> {
        self.jito_block_engine_url.as_deref()
    }

    pub fn jito_api_key(&self) -> Option<&str> {
        self.jito_api_key.as_deref()
    }

    pub fn jito_metrics(&self) -> Option<&JitoMetrics> {
        self.jito_metrics.as_ref()
    }

    pub fn signer_keypair_bytes(&self) -> Vec<u8> {
        self.signer_keypair.to_bytes().to_vec()
    }

    pub fn bloxroute_api_url(&self) -> Option<&str> {
        self.bloxroute_api_url.as_deref()
    }

    pub fn bloxroute_auth_header(&self) -> Option<&str> {
        self.bloxroute_auth_header.as_deref()
    }

    pub fn zeroslot_api_url(&self) -> Option<&str> {
        self.zeroslot_api_url.as_deref()
    }

    pub fn zeroslot_api_key(&self) -> Option<&str> {
        self.zeroslot_api_key.as_deref()
    }

    pub fn confirmation_timeout(&self) -> Option<Duration> {
        self.confirmation_timeout
    }

    pub fn rpc_url(&self) -> &str {
        &self.rpc_url
    }

    pub fn new(signer_keypair: Keypair, rpc_url: impl Into<String>) -> Self {
        Self {
            signer_keypair,
            rpc_url: rpc_url.into(),
            buy_slippage: 0.01,
            sell_slippage: 0.01,
            sim_buy_slippage: 0.01,
            sim_sell_slippage: 0.01,
            simulation_mode: SimulationMode::RpcBased,
            max_real_trades: None,
            skip_simulation_when_sell: true,
            skip_simulation_for_buy: true,
            compute_unit_price: Some(1000),
            txn_maker_url: None,
            txn_maker_connection_type: None,
            txn_maker_unix_socket_path: None,
            compute_unit_limit: Some(1400000),
            jito_block_engine_url: None,
            jito_api_key: None,
            jito_buy_tip: None,
            jito_sell_tip: None,
            max_slot_latency: None,
            jito_metrics: None,
            redis_tx_url: None,
            redis_tx_key_prefix: None,
            bloxroute_api_url: None,
            bloxroute_auth_header: None,
            bloxroute_tip: None,
            zeroslot_api_url: None,
            zeroslot_api_key: None,
            zeroslot_tip: None,
            confirmation_timeout: None, // Default to None
            transaction_submitter_type: None,
            bad_leader_validators: None,
            leader_monitor_refresh_interval_secs: None,
        }
    }

    pub fn with_slippage(mut self, buy: f64, sell: f64) -> Self {
        self.buy_slippage = buy;
        self.sell_slippage = sell;
        self
    }

    pub fn with_simulation_slippage(mut self, buy: f64, sell: f64) -> Self {
        self.sim_buy_slippage = buy;
        self.sim_sell_slippage = sell;
        self
    }

    pub fn with_simulation_mode(mut self, mode: SimulationMode) -> Self {
        self.simulation_mode = mode;
        self
    }

    pub fn with_compute_unit_price(mut self, price: u64) -> Self {
        self.compute_unit_price = Some(price);
        self
    }

    pub fn with_compute_unit_limit(mut self, limit: u32) -> Self {
        self.compute_unit_limit = Some(limit);
        self
    }

    pub fn with_max_real_trades(mut self, max: u32) -> Self {
        self.max_real_trades = Some(max);
        self
    }

    pub fn skip_simulation_when_sell(mut self, skip: bool) -> Self {
        self.skip_simulation_when_sell = skip;
        self
    }

    pub fn with_skip_simulation_for_buy(mut self, skip: bool) -> Self {
        self.skip_simulation_for_buy = skip;
        self
    }

    // TxnMaker configuration
    pub fn with_txn_maker_config(
        mut self,
        connection_type: String,
        tcp_url: Option<String>,
        unix_socket_path: Option<String>,
    ) -> Self {
        self.txn_maker_connection_type = Some(connection_type);
        if let Some(url) = tcp_url {
            self.txn_maker_url = Some(url);
        }
        self.txn_maker_unix_socket_path = unix_socket_path;
        self
    }

    // Transaction submitter configuration
    pub fn with_transaction_submitter_type(mut self, submitter_type: Option<String>) -> Self {
        self.transaction_submitter_type = submitter_type;
        self
    }

    // General confirmation timeout configuration
    pub fn with_confirmation_timeout(mut self, timeout_secs: Option<u64>) -> Self {
        self.confirmation_timeout = timeout_secs.map(Duration::from_secs);
        info!(timeout = ?self.confirmation_timeout, "Setting confirmation timeout for submitters");

        self
    }

    pub fn with_jito_config(mut self, url: Option<String>, api_key: Option<String>) -> Self {
        self.jito_block_engine_url = url;
        self.jito_api_key = api_key;
        self
    }

    pub fn with_bloxroute_config(
        mut self,
        url: Option<String>,
        auth_header: Option<String>,
    ) -> Self {
        self.bloxroute_api_url = url;
        self.bloxroute_auth_header = auth_header;
        self
    }

    pub fn with_bloxroute_tip(mut self, tip: u64) -> Self {
        self.bloxroute_tip = Some(tip);
        self
    }

    pub fn with_zeroslot_config(mut self, url: Option<String>, api_key: Option<String>) -> Self {
        self.zeroslot_api_url = url;
        self.zeroslot_api_key = api_key;
        self
    }

    pub fn with_zeroslot_tip(mut self, tip: u64) -> Self {
        self.zeroslot_tip = Some(tip);
        self
    }

    pub fn with_jito_buy_tip(mut self, tip: u64) -> Self {
        self.jito_buy_tip = Some(tip);
        self
    }

    pub fn with_jito_sell_tip(mut self, tip: u64) -> Self {
        self.jito_sell_tip = Some(tip);
        self
    }

    pub fn with_max_slot_latency(mut self, latency: u64) -> Self {
        self.max_slot_latency = Some(latency);
        self
    }

    pub fn with_jito_metrics(mut self, metrics: JitoMetrics) -> Self {
        self.jito_metrics = Some(metrics);
        self
    }

    pub fn with_redis_tx_retriever(mut self, redis_url: &str, key_prefix: &str) -> Self {
        self.redis_tx_url = Some(redis_url.to_string());
        self.redis_tx_key_prefix = Some(key_prefix.to_string());
        self
    }

    pub fn with_leader_monitor_config(
        mut self,
        validators: Option<HashSet<Pubkey>>,
        interval_secs: Option<u64>,
    ) -> Self {
        self.bad_leader_validators = validators;
        self.leader_monitor_refresh_interval_secs = interval_secs;
        self
    }

    pub async fn build(self, registry: &Registry) -> Result<SolanaOrderExecutor> {
        let signer_pub_key = self.signer_keypair.pubkey();

        // Create the gRPC confirmation monitor that will be shared
        let grpc_confirmer = GrpcConfirmationMonitor::new();

        // Create RPC client for token decimal fetching
        let rpc_client = Arc::new(RpcClient::new(self.rpc_url.clone()));

        // Create the appropriate TxnMaker client based on configuration
        let timeout = Duration::from_millis(500);

        let txn_maker_client: Box<dyn TxnMakerClient> =
            match self.txn_maker_connection_type.as_deref() {
                Some("shadow") => {
                    // Shadow mode: create both clients and wrap in shadow client
                    let txn_maker_url = self
                        .txn_maker_url
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("tcp_url is required for shadow mode"))?;
                    let socket_path = self.txn_maker_unix_socket_path.clone().ok_or_else(|| {
                        anyhow::anyhow!("unix_socket_path is required for shadow mode")
                    })?;

                    info!(
                        tcp_url = txn_maker_url.as_str(),
                        unix_socket = socket_path.as_str(),
                        "Creating Shadow TxnMaker client"
                    );

                    let tcp_client = Box::new(TcpTxnMakerClient::new(txn_maker_url, timeout)?);
                    let unix_client = Box::new(UnixSocketTxnMakerClient::new(socket_path));
                    Box::new(ShadowTxnMakerClient::new(tcp_client, unix_client))
                }
                Some("unix_socket") => {
                    let socket_path = self.txn_maker_unix_socket_path.clone().ok_or_else(|| {
                        anyhow::anyhow!("unix_socket_path required for unix_socket connection type")
                    })?;

                    info!(
                        socket_path = socket_path.as_str(),
                        "Creating Unix socket TxnMaker client"
                    );
                    Box::new(UnixSocketTxnMakerClient::new(socket_path))
                }
                _ => {
                    // Default to TCP connection (includes "tcp" or None)
                    let txn_maker_url = self.txn_maker_url.clone().ok_or_else(|| {
                        anyhow::anyhow!("tcp_url is required for tcp connection type")
                    })?;

                    info!(url = txn_maker_url.as_str(), "Creating TCP TxnMaker client");
                    Box::new(TcpTxnMakerClient::new(txn_maker_url, timeout)?)
                }
            };

        let transaction_constructor: Box<dyn TransactionConstructor> =
            Box::new(TxnMakerTransactionConstructor::new(
                txn_maker_client,
                signer_pub_key,
                self.buy_slippage,
                self.sell_slippage,
                self.compute_unit_price.unwrap_or(1000),
                self.compute_unit_limit.unwrap_or(1400000),
                self.jito_buy_tip,
                self.jito_sell_tip,
                self.bloxroute_tip,
                self.zeroslot_tip,
                registry,
                rpc_client,
            )?);

        let submitter_type = self.transaction_submitter_type.as_deref().unwrap_or("rpc"); // Default to RPC
        info!(
            submitter_type = submitter_type,
            "Creating transaction submitter"
        );

        // Use the registry to create the submitter instance
        let transaction_submitter =
            submitter_registry::create_submitter(submitter_type, &self, grpc_confirmer.clone())?;

        let redis_tx_retriever = if let (Some(redis_url), Some(key_prefix)) =
            (self.redis_tx_url, self.redis_tx_key_prefix)
        {
            Some(RedisTxRetriever::new(&redis_url, key_prefix).await?)
        } else {
            None
        };

        // Create leader monitor service if configured
        let leader_monitor_service = if let (Some(bad_validators), Some(refresh_interval)) = (
            self.bad_leader_validators,
            self.leader_monitor_refresh_interval_secs,
        ) {
            info!(
                "Creating leader monitor service with {} bad validators and {}s refresh interval",
                bad_validators.len(),
                refresh_interval
            );
            let rpc_client = Arc::new(RpcClient::new(self.rpc_url.clone()));
            Some(Arc::new(LeaderMonitorService::new(
                rpc_client,
                bad_validators,
                Some(Duration::from_secs(refresh_interval)),
            )))
        } else {
            None
        };

        // Create signing service for local keypair signing
        let signing_service = Arc::new(LocalKeypairSigningService::new(
            self.signer_keypair.insecure_clone(),
        ));

        // Create transaction service that combines signing and submission
        let transaction_submitter = Arc::from(transaction_submitter);
        let transaction_service = TransactionService::new(signing_service, transaction_submitter);

        Ok(SolanaOrderExecutor::new(
            transaction_service,
            transaction_constructor,
            self.rpc_url,
            self.sim_buy_slippage,
            self.sim_sell_slippage,
            self.simulation_mode,
            self.max_real_trades,
            self.skip_simulation_when_sell,
            self.skip_simulation_for_buy,
            self.max_slot_latency,
            redis_tx_retriever,
            grpc_confirmer,
            leader_monitor_service,
        ))
    }
}
