use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;

use crate::execution::order::Order;
use crate::utils::token_decimal_cache::TokenDecimalCache;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DexType {
    PumpFun,
    Bonk,
}

impl DexType {
    pub fn endpoint(&self) -> &'static str {
        match self {
            DexType::PumpFun => "/pfun_swap",
            DexType::Bonk => "/bonk_swap",
        }
    }

    /// Detect DEX type from token mint address
    /// Returns None if no specific DEX is detected (defaults will be used)
    pub fn detect_from_mint(mint: &str) -> Option<DexType> {
        // Check for bonk-specific suffix
        if mint.ends_with("bonk") {
            Some(DexType::Bonk)
        } else if mint.ends_with("pump") {
            Some(DexType::PumpFun)
        } else {
            None
        }
    }
}

#[async_trait]
pub trait DexRequestBuilder: Send + Sync {
    async fn build_request(
        &self,
        order: &Order,
        signer: Pubkey,
        compute_unit_price: u64,
        compute_unit_limit: u32,
        jito_tip: Option<u64>,
        blxr_tip: Option<u64>,
        zeroslot_tip: Option<u64>,
        no_later_than_slot: Option<u64>,
        buy_slippage: f64,
        sell_slippage: f64,
        rpc_client: &RpcClient,
    ) -> Result<serde_json::Value>;

    fn as_any(&self) -> &dyn std::any::Any;
}

pub struct PumpFunRequestBuilder {
    creator_cache: std::sync::Mutex<std::collections::HashMap<String, Option<Pubkey>>>,
    decimal_cache: Arc<TokenDecimalCache>,
}

impl PumpFunRequestBuilder {
    pub fn new(decimal_cache: Arc<TokenDecimalCache>) -> Self {
        Self {
            creator_cache: std::sync::Mutex::new(std::collections::HashMap::new()),
            decimal_cache,
        }
    }

    pub fn cache_creator(&self, mint: &str, creator: Option<Pubkey>) {
        if let Ok(mut cache) = self.creator_cache.lock() {
            cache.insert(mint.to_string(), creator);

            if let Some(creator_pubkey) = creator {
                tracing::info!(
                    mint = mint,
                    creator = creator_pubkey.to_string(),
                    "Cached creator for mint"
                );
            }
        }
    }

    pub fn get_creator_for_mint(&self, mint: &str) -> Option<Pubkey> {
        if let Ok(cache) = self.creator_cache.lock() {
            if let Some(creator) = cache.get(mint).cloned().flatten() {
                // Check if the creator is the "bad" creator (all 1's)
                if creator.to_string() == "11111111111111111111111111111111" {
                    tracing::info!(
                        ?mint,
                        creator = creator.to_string(),
                        "Invalid creator for mint"
                    );
                    return None;
                }
                return Some(creator);
            }
        }
        None
    }
}

#[async_trait]
impl DexRequestBuilder for PumpFunRequestBuilder {
    async fn build_request(
        &self,
        order: &Order,
        signer: Pubkey,
        compute_unit_price: u64,
        compute_unit_limit: u32,
        jito_tip: Option<u64>,
        blxr_tip: Option<u64>,
        zeroslot_tip: Option<u64>,
        no_later_than_slot: Option<u64>,
        buy_slippage: f64,
        sell_slippage: f64,
        rpc_client: &RpcClient,
    ) -> Result<serde_json::Value> {
        #[allow(deprecated)]
        use crate::execution::order::OrderType;

        const LAMPORTS_PER_SOL: f64 = 1_000_000_000.0;

        // Get token multiplier dynamically using the decimal cache
        let token_multiplier = self
            .decimal_cache
            .get_token_multiplier(&order.mint, rpc_client)
            .await?;

        #[allow(deprecated)]
        let (is_buy, amount, max_or_min) = match &order.order_type {
            OrderType::Buy { sol_amount }
            | OrderType::MarketBuy {
                quote_amount: sol_amount,
            } => {
                let sol_amount_lamports = sol_amount * LAMPORTS_PER_SOL;
                let price = order
                    .price
                    .ok_or_else(|| anyhow::anyhow!("Order price must be set for buy orders"))?;
                let token_amount = sol_amount_lamports / price;

                let slippage_factor = 1.0 + buy_slippage;
                let max_sol_lamports = (sol_amount_lamports * slippage_factor) as u64;

                tracing::info!(
                    sol_amount = sol_amount,
                    sol_amount_lamports = sol_amount_lamports,
                    price = order.price,
                    token_amount = token_amount,
                    slippage_factor = slippage_factor,
                    max_sol_lamports = max_sol_lamports,
                    "Calculated amounts for buy order"
                );

                (true, token_amount as u64, max_sol_lamports)
            }
            OrderType::Sell {
                token_amount,
                clear_position: _,
            }
            | OrderType::MarketSell {
                token_amount,
                clear_position: _,
            } => {
                let sol_amount_lamports = token_amount * order.price.unwrap_or(0.0);
                let token_amount_base = token_amount * token_multiplier;

                let slippage_factor = 1.0 - sell_slippage;
                let min_sol_lamports = (sol_amount_lamports * slippage_factor) as u64;

                tracing::info!(
                    token_amount_standard = token_amount,
                    token_amount_base = token_amount_base,
                    price = order.price,
                    sol_amount_lamports = sol_amount_lamports,
                    slippage_factor = slippage_factor,
                    min_sol_lamports = min_sol_lamports,
                    "Calculated amounts for sell order"
                );

                (false, token_amount_base as u64, min_sol_lamports)
            }
            OrderType::LimitBuy { .. } | OrderType::LimitSell { .. } => {
                return Err(anyhow::anyhow!(
                    "Limit orders are not supported by PumpFun DEX"
                ));
            }
            OrderType::Cancel { order_id } => {
                return Err(anyhow::anyhow!(
                    "Cancel orders are not supported by PumpFun DEX: {}",
                    order_id
                ));
            }
        };

        let creator = self
            .get_creator_for_mint(&order.mint)
            .map(|pubkey| pubkey.to_string());

        let selected_jito_tip = if is_buy { jito_tip } else { jito_tip };

        Ok(serde_json::json!({
            "signer": signer.to_string(),
            "payer": signer.to_string(),
            "computeUnitPx": compute_unit_price,
            "computeUnitLimit": compute_unit_limit,
            "jitoTip": selected_jito_tip,
            "blxrTip": blxr_tip,
            "zslotTip": zeroslot_tip,
            "mint": order.mint,
            "isBuy": is_buy,
            "inAmount": amount.to_string(),
            "maxOrMin": max_or_min,
            "noLaterThan": no_later_than_slot,
            "creatorWallet": creator,
        }))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct BonkRequestBuilder {
    decimal_cache: Arc<TokenDecimalCache>,
}

impl BonkRequestBuilder {
    pub fn new(decimal_cache: Arc<TokenDecimalCache>) -> Self {
        Self { decimal_cache }
    }
}

#[async_trait]
impl DexRequestBuilder for BonkRequestBuilder {
    async fn build_request(
        &self,
        order: &Order,
        signer: Pubkey,
        compute_unit_price: u64,
        compute_unit_limit: u32,
        jito_tip: Option<u64>,
        blxr_tip: Option<u64>,
        zeroslot_tip: Option<u64>,
        no_later_than_slot: Option<u64>,
        buy_slippage: f64,
        sell_slippage: f64,
        rpc_client: &RpcClient,
    ) -> Result<serde_json::Value> {
        #[allow(deprecated)]
        use crate::execution::order::OrderType;

        const LAMPORTS_PER_SOL: f64 = 1_000_000_000.0;

        // Get token multiplier dynamically using the decimal cache
        let token_multiplier = self
            .decimal_cache
            .get_token_multiplier(&order.mint, rpc_client)
            .await?;

        #[allow(deprecated)]
        let (is_buy, amount, minimum_amount_out) = match &order.order_type {
            OrderType::Buy { sol_amount }
            | OrderType::MarketBuy {
                quote_amount: sol_amount,
            } => {
                let sol_amount_lamports = (sol_amount * LAMPORTS_PER_SOL) as u64;

                // For buy orders, we're spending SOL to get tokens
                // minimumAmountOut is the minimum tokens we expect to receive
                let price = order
                    .price
                    .ok_or_else(|| anyhow::anyhow!("Order price must be set for buy orders"))?;
                let expected_tokens = sol_amount_lamports as f64 / price;
                let slippage_factor = 1.0 - buy_slippage;
                let minimum_tokens = (expected_tokens * slippage_factor) as u64;

                tracing::info!(
                    sol_amount = sol_amount,
                    sol_amount_lamports = sol_amount_lamports,
                    price = price,
                    expected_tokens = expected_tokens,
                    slippage_factor = slippage_factor,
                    minimum_tokens = minimum_tokens,
                    "Calculated amounts for Bonk buy order"
                );

                (true, sol_amount_lamports, minimum_tokens)
            }
            OrderType::Sell {
                token_amount,
                clear_position: _,
            }
            | OrderType::MarketSell {
                token_amount,
                clear_position: _,
            } => {
                // For Bonk, token amount needs to be in base units
                let token_amount_base = (token_amount * token_multiplier) as u64;

                // For sell orders, we're spending tokens to get SOL
                // minimumAmountOut is the minimum SOL (in lamports) we expect to receive
                let price = order.price.unwrap_or(0.0);
                let expected_sol_lamports = token_amount * price;
                let slippage_factor = 1.0 - sell_slippage;
                let minimum_sol_lamports = (expected_sol_lamports * slippage_factor) as u64;

                tracing::info!(
                    token_amount_standard = token_amount,
                    token_amount_base = token_amount_base,
                    price = price,
                    expected_sol_lamports = expected_sol_lamports,
                    slippage_factor = slippage_factor,
                    minimum_sol_lamports = minimum_sol_lamports,
                    "Calculated amounts for Bonk sell order"
                );

                (false, token_amount_base, minimum_sol_lamports)
            }
            OrderType::LimitBuy { .. } | OrderType::LimitSell { .. } => {
                return Err(anyhow::anyhow!(
                    "Limit orders are not supported by Bonk DEX"
                ));
            }
            OrderType::Cancel { order_id } => {
                return Err(anyhow::anyhow!(
                    "Cancel orders are not supported by Bonk DEX: {}",
                    order_id
                ));
            }
        };

        let selected_jito_tip = if is_buy { jito_tip } else { jito_tip };

        Ok(serde_json::json!({
            "signer": signer.to_string(),
            "payer": signer.to_string(),
            "computeUnitPx": compute_unit_price,
            "computeUnitLimit": compute_unit_limit,
            "jitoTip": selected_jito_tip,
            "blxrTip": blxr_tip,
            "zslotTip": zeroslot_tip,
            "mint": order.mint,
            "isBuy": is_buy,
            "inAmount": amount,
            "autoHandleWsol": true,
            "minimumAmountOut": minimum_amount_out,
            "noLaterThan": no_later_than_slot,
        }))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
