use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose};
use prometheus::Registry;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::message::VersionedMessage;
use tracing::{info, instrument};

use super::{
    BonkRequestBuilder, DexRequestBuilder, DexType, PumpFunRequestBuilder, TransactionConstructor,
    TxnMakerClient, metrics::TxnMakerMetrics,
};
use crate::execution::order::Order;
use crate::signal::TradableSignal;
use crate::utils::token_decimal_cache::TokenDecimalCache;

pub struct TxnMakerTransactionConstructor {
    client: Box<dyn TxnMakerClient>,
    signer_pubkey: solana_sdk::pubkey::Pubkey,
    buy_slippage: f64,  // Used as max_or_min percentage for buys
    sell_slippage: f64, // Used as max_or_min percentage for sells
    compute_unit_price: u64,
    compute_unit_limit: u32,
    jito_buy_tip: Option<u64>,  // Tip for buy transactions
    jito_sell_tip: Option<u64>, // Tip for sell transactions
    blxr_tip: Option<u64>,
    zeroslot_tip: Option<u64>,
    metrics: TxnMakerMetrics,
    dex_builders: HashMap<DexType, Box<dyn DexRequestBuilder>>,
    rpc_client: Arc<RpcClient>,
    timeout: Duration,
}

impl TxnMakerTransactionConstructor {
    pub fn new(
        client: Box<dyn TxnMakerClient>,
        signer_pubkey: solana_sdk::pubkey::Pubkey,
        buy_slippage: f64,
        sell_slippage: f64,
        compute_unit_price: u64,
        compute_unit_limit: u32,
        jito_buy_tip: Option<u64>,
        jito_sell_tip: Option<u64>,
        blxr_tip: Option<u64>,
        zeroslot_tip: Option<u64>,
        registry: &Registry,
        rpc_client: Arc<RpcClient>,
    ) -> Result<Self> {
        tracing::info!(
            signer_pubkey = signer_pubkey.to_string().as_str(),
            buy_slippage,
            sell_slippage,
            compute_unit_price,
            compute_unit_limit,
            jito_buy_tip,
            jito_sell_tip,
            blxr_tip,
            zeroslot_tip,
            "Creating TxnMakerTransactionConstructor"
        );

        // Create shared decimal cache for both DEX builders
        let decimal_cache = Arc::new(TokenDecimalCache::new());

        let mut dex_builders: HashMap<DexType, Box<dyn DexRequestBuilder>> = HashMap::new();
        dex_builders.insert(
            DexType::PumpFun,
            Box::new(PumpFunRequestBuilder::new(decimal_cache.clone())),
        );
        dex_builders.insert(
            DexType::Bonk,
            Box::new(BonkRequestBuilder::new(decimal_cache)),
        );

        Ok(Self {
            metrics: TxnMakerMetrics::new(registry).context("Failed to create TxnMaker metrics")?,
            client,
            signer_pubkey,
            buy_slippage,
            sell_slippage,
            compute_unit_price,
            compute_unit_limit,
            jito_buy_tip,
            jito_sell_tip,
            blxr_tip,
            zeroslot_tip,
            dex_builders,
            rpc_client,
            timeout: Duration::from_millis(500),
        })
    }

    // cache signal info
    pub fn cache_signal(&self, signal: &dyn TradableSignal) {
        // Only cache for PumpFun
        if let Some(builder) = self.dex_builders.get(&DexType::PumpFun) {
            if let Some(pumpfun_builder) = builder.as_any().downcast_ref::<PumpFunRequestBuilder>()
            {
                if let Some(mint) = signal.get_mint() {
                    let creator = signal.get_creator();
                    pumpfun_builder.cache_creator(mint, creator);
                }
            }
        }
    }

    fn get_dex_type(order: &Order) -> Option<DexType> {
        // First check if DEX type is explicitly set in the order
        if let Some(dex_type) = order.dex_type {
            return Some(dex_type);
        }

        // Try to detect from mint address
        if let Some(detected) = DexType::detect_from_mint(&order.mint) {
            return Some(detected);
        }

        // Cannot determine DEX type
        None
    }

    async fn send_txn_maker_request(
        &self,
        request: &serde_json::Value,
        dex_type: DexType,
    ) -> Result<super::client::TxnMakerResponse> {
        let timer = self.metrics.request_latency_milliseconds.start_timer();

        info!(
            endpoint = dex_type.endpoint(),
            "Sending request to TxnMaker service"
        );

        let response = self
            .client
            .send_request(request, dex_type, self.timeout)
            .await
            .context("Failed to send request to TxnMaker service")?;

        let latency_ms = timer.stop_and_record() * 1000.0;

        info!(latency_ms = latency_ms, "Received TxnMaker response");

        if response.error.is_some() {
            self.metrics
                .requests_total
                .with_label_values(&["failure_api"])
                .inc();
        } else if response.encoded64.is_none() {
            self.metrics
                .requests_total
                .with_label_values(&["failure_malformed"])
                .inc();
        } else {
            self.metrics
                .requests_total
                .with_label_values(&["success"])
                .inc();
        }

        Ok(response)
    }
}

#[async_trait]
impl TransactionConstructor for TxnMakerTransactionConstructor {
    fn cache_signal(&self, signal: &dyn TradableSignal) {
        self.cache_signal(signal);
    }

    #[instrument(skip(self), level = "info")]
    async fn construct_transaction(
        &self,
        order: &Order,
        no_later_than_slot: Option<u64>,
        _log_str: Option<String>,
    ) -> Result<VersionedMessage> {
        let dex_type = Self::get_dex_type(order)
            .ok_or_else(|| anyhow!("Cannot determine DEX type for mint: {}", order.mint))?;

        let builder = self
            .dex_builders
            .get(&dex_type)
            .ok_or_else(|| anyhow!("No builder available for DEX type: {:?}", dex_type))?;

        let request = builder
            .build_request(
                order,
                self.signer_pubkey,
                self.compute_unit_price,
                self.compute_unit_limit,
                if order.order_type.is_buy() {
                    self.jito_buy_tip
                } else {
                    self.jito_sell_tip
                },
                self.blxr_tip,
                self.zeroslot_tip,
                no_later_than_slot,
                self.buy_slippage,
                self.sell_slippage,
                &self.rpc_client,
            )
            .await?;

        info!(
            request = ?request,
            dex_type = ?dex_type,
            "Sending request to TxnMaker service"
        );

        let response = self.send_txn_maker_request(&request, dex_type).await?;

        if let Some(error) = response.error {
            return Err(anyhow!("TxnMaker error: {}", error));
        }

        let encoded64 = response
            .encoded64
            .ok_or_else(|| anyhow!("TxnMaker response missing encoded transaction data"))?;

        let decoded = general_purpose::STANDARD
            .decode(&encoded64)
            .context("Failed to decode base64-encoded transaction")?;

        bincode::deserialize::<VersionedMessage>(&decoded)
            .context("Failed to deserialize transaction message")
    }
}
