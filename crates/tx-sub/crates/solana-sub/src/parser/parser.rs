use std::str::FromStr;

use anyhow::{Context, Result};
use bs58;
use solana_rpc_client::rpc_client::RpcClient;
use solana_sdk::{
    address_lookup_table::state::AddressLookupTable,
    message::{VersionedMessage, v0::MessageAddressTableLookup},
    pubkey::Pubkey,
    transaction::VersionedTransaction,
};
use tokio::sync::broadcast;
use tracing::{debug, error, info, trace, warn};
use yellowstone_grpc_proto::{
    prelude::{SubscribeUpdateTransaction, SubscribeUpdateTransactionInfo},
    solana::storage::confirmed_block::TransactionStatusMeta,
};

use super::{bonk, meteora_dlmm, pumpfun2, ray_ammv4};
use crate::{
    grpc::TransactionData,
    parser::market::{MarketTrade, PumpfunLog},
    stats_monitor::StatsMonitor,
};

pub const PUMPFUN_LOG_DISCRIMINATOR: [u8; 16] = [
    0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d, 0xbd, 0xdb, 0x7f, 0xd3, 0x4e, 0xe6, 0x61, 0xee,
];

pub const PUMPFUN_CREATE_LOG_DISCRIMINATOR: [u8; 16] = [
    228, 69, 165, 46, 81, 203, 154, 29, 27, 114, 169, 77, 222, 235, 99, 118,
];

pub const RAY_AMMV4_ID: Pubkey =
    Pubkey::from_str_const("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");

pub const BONK_ID: Pubkey = Pubkey::from_str_const("LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj");

/// Configuration for the transaction parser
pub struct ParserConfig {
    /// PumpFun program ID to filter for
    pub pumpfun_program_id: Pubkey,
    /// RPC client URL, if not provided check for RPC_URL env var, otherwise uses solana foundations' endpoint
    pub rpc_url: Option<String>,
}

impl ParserConfig {
    /// Creates a new ParserConfig from a PumpfunConfig
    pub fn from_config(config: &crate::config::PumpfunConfig) -> Self {
        Self {
            pumpfun_program_id: Pubkey::from_str(&config.program_id)
                .context("Invalid PumpFun program ID in config")
                .unwrap(),
            rpc_url: config.rpc_url.clone(),
        }
    }
}

/// this is a non-owning type
pub enum ParseableInstruction<'a> {
    Inner(&'a yellowstone_grpc_proto::prelude::InnerInstruction),
    Compiled(&'a solana_sdk::instruction::CompiledInstruction),
}

pub struct BlockDecodeCtx<'a> {
    pub conn: &'a RpcClient,
    pub ver_txn: &'a VersionedTransaction,
    pub meta: &'a TransactionStatusMeta,
    pub cur_slot: u64,
    //pub rx_time: f64,
    //pub block_time: f64,
    pub tx_idx: usize,
}

impl std::fmt::Debug for BlockDecodeCtx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockDecodeCtx")
            .field("ver_txn", &self.ver_txn)
            .field("cur_slot", &self.cur_slot)
            .field("tx_idx", &self.tx_idx)
            .field("meta", &self.meta)
            .finish()
    }
}

#[derive(Debug)]
pub struct TxnDecodeCtx<'a> {
    pub bctx: &'a BlockDecodeCtx<'a>,
    pub static_keys: &'a [Pubkey],
    pub addr_lookups: &'a Option<&'a [MessageAddressTableLookup]>,
    pub txn_lut_cache: &'a mut Option<(Vec<(u8, Pubkey)>, solana_sdk::account::Account)>,
}

/// Parses transaction data from the Solana blockchain
pub struct TransactionParser {
    /// Channel receiver for incoming transaction data
    tx_rx: broadcast::Receiver<TransactionData>,
    /// Channel sender for outgoing parsed PumpfunLog data
    trade_tx: broadcast::Sender<MarketTrade>,
    /// Configuration for parsing
    config: ParserConfig,
    /// Optional stats monitor for tracking DEX events
    stats_monitor: Option<std::sync::Arc<StatsMonitor>>,
}

impl TransactionParser {
    /// Extract DEX name from MarketTrade for stats tracking
    fn get_dex_name(trade: &MarketTrade) -> &'static str {
        use crate::parser::market::TradeLog;
        match &trade.log {
            TradeLog::Pumpfun(_) => "pumpfun",
            TradeLog::RayAMMv4(_) => "rayammv4",
            TradeLog::PumpfunTokenCreate(_) => "pumpfun_create",
            TradeLog::MeteoraDLMM(_) => "meteora",
            TradeLog::Bonk(_) => "bonk",
        }
    }

    /// Record trade event to stats monitor if enabled
    async fn record_trade_stats(&self, trade: &MarketTrade) {
        if let Some(ref monitor) = self.stats_monitor {
            let dex = Self::get_dex_name(trade);
            monitor.record_dex_event(dex).await;
        }
    }
    /// Creates a new TransactionParser
    pub fn new(
        tx_rx: broadcast::Receiver<TransactionData>,
        trade_tx: broadcast::Sender<MarketTrade>,
        config: ParserConfig,
        stats_monitor: Option<std::sync::Arc<StatsMonitor>>,
    ) -> Self {
        Self {
            tx_rx,
            trade_tx,
            config,
            stats_monitor,
        }
    }

    /// Starts the transaction parser
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting TransactionParser");

        let rpc_client = RpcClient::new(self.config.rpc_url.clone().unwrap_or_else(|| {
            std::env::var("RPC_URL").unwrap_or("https://api.mainnet-beta.solana.com".to_string())
        }));

        loop {
            match self.tx_rx.recv().await {
                Ok(tx_data) => {
                    self.process_transaction(&rpc_client, tx_data).await?;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(count = n, "Transaction parser lagged behind GRPC stream");
                    // Continue processing, but log the lag
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("Transaction broadcast channel closed, stopping parser.");
                    break; // Exit the loop if the channel is closed
                }
            }
        }

        Ok(())
    }

    /// Process an individual transaction
    async fn process_transaction(
        &self,
        rpc_client: &RpcClient,
        tx_data: TransactionData,
    ) -> Result<()> {
        match tx_data {
            TransactionData::GrpcTransaction(tx) => {
                self.process_grpc_transaction(rpc_client, tx).await?
            }
            TransactionData::UiTransaction(_ui_tx) => {
                // TODO: not implemented.
                eprintln!("UI transaction processing not implemented: {:?}", _ui_tx);
            }
        }

        Ok(())
    }

    /// Process a GRPC transaction
    async fn process_grpc_transaction(
        &self,
        rpc_client: &RpcClient,
        tx: SubscribeUpdateTransaction,
    ) -> Result<()> {
        let slot = tx.slot;

        let tx_info = match tx.transaction.as_ref() {
            Some(info) => info,
            None => {
                debug!("Missing transaction info");
                return Ok(());
            }
        };

        let meta = match tx_info.meta.as_ref() {
            Some(meta) => meta,
            None => {
                debug!("Missing transaction metadata");
                return Ok(());
            }
        };

        // Get transaction signature for logging
        let signature = bs58::encode(&tx_info.signature).into_string();
        debug!(signature = %signature, "Processing transaction");

        if let Some(tx) = &tx_info.transaction
            && let Some(msg) = &tx.message
            && let Some(hdr) = &msg.header
        {
            trace!("tx={:?}; msg={:?}; hdr={:?}", tx, msg, hdr);

            let account_keys = msg
                .account_keys
                .iter()
                .map(|key| Pubkey::new_from_array(key.as_slice().try_into().unwrap()))
                .collect::<Vec<_>>();

            trace!(
                "account_keys={:?}",
                account_keys
                    .iter()
                    .map(|key| bs58::encode(key).into_string())
                    .collect::<Vec<_>>()
            );

            let instructions = msg
                .instructions
                .iter()
                .map(|inst| solana_sdk::instruction::CompiledInstruction {
                    data: inst.data.clone(),
                    program_id_index: inst.program_id_index as u8,
                    accounts: inst.accounts.clone(),
                })
                .collect::<Vec<_>>();

            let recent_blockhash = solana_sdk::hash::Hash::new_from_array(
                msg.recent_blockhash.clone().try_into().unwrap(),
            );

            let address_table_lookups = msg
                .address_table_lookups
                .iter()
                .map(|lookup| {
                    let account_key =
                        Pubkey::new_from_array(lookup.account_key.as_slice().try_into().unwrap());
                    MessageAddressTableLookup {
                        account_key,
                        writable_indexes: lookup.writable_indexes.clone(),
                        readonly_indexes: lookup.readonly_indexes.clone(),
                    }
                })
                .collect::<Vec<_>>();

            let ver_msg = if msg.versioned {
                VersionedMessage::V0(solana_sdk::message::v0::Message {
                    account_keys,
                    instructions,
                    recent_blockhash,
                    address_table_lookups,
                    header: solana_sdk::message::MessageHeader {
                        num_required_signatures: hdr.num_required_signatures as u8,
                        num_readonly_signed_accounts: hdr.num_readonly_signed_accounts as u8,
                        num_readonly_unsigned_accounts: hdr.num_readonly_unsigned_accounts as u8,
                    },
                })
            } else {
                VersionedMessage::Legacy(solana_sdk::message::Message {
                    account_keys,
                    instructions,
                    recent_blockhash,
                    header: solana_sdk::message::MessageHeader {
                        num_required_signatures: hdr.num_required_signatures as u8,
                        num_readonly_signed_accounts: hdr.num_readonly_signed_accounts as u8,
                        num_readonly_unsigned_accounts: hdr.num_readonly_unsigned_accounts as u8,
                    },
                })
            };

            let ver_txn = VersionedTransaction {
                signatures: vec![(tx_info.signature.as_slice()).try_into().unwrap()],
                message: ver_msg,
            };

            let bctx = BlockDecodeCtx {
                conn: rpc_client,
                ver_txn: &ver_txn,
                //raw_txn: &tx_info,
                meta,
                //observe_list: &std::collections::HashSet::new(),
                cur_slot: slot,
                //rx_time: 0.0,
                //block_time: 0.0,
                tx_idx: tx_info.index as usize,
            };

            let mut decode_ctx = TxnDecodeCtx {
                bctx: &bctx,
                static_keys: ver_txn.message.static_account_keys(),
                addr_lookups: &ver_txn.message.address_table_lookups(),
                txn_lut_cache: &mut None,
            };

            // Process inner instructions
            self.process_tx_instructions(&mut decode_ctx, &signature, tx_info, meta)
                .await
        } else {
            anyhow::bail!("Transaction not found")
        }
    }

    /// Process the inner instructions of a transaction
    async fn process_tx_instructions(
        &self,
        ctx: &mut TxnDecodeCtx<'_>,
        sig: &str,
        tx_info: &SubscribeUpdateTransactionInfo,
        meta: &TransactionStatusMeta,
    ) -> Result<()> {
        // Process the main instructions

        if let Some(_meta) = &tx_info.meta
            && let Some(_tx) = &tx_info.transaction
        //&& let Some(msg) = ctx.bctx.ver_txn.message
        {
            let msg = &ctx.bctx.ver_txn.message;
            for ix in msg.instructions().iter().enumerate() {
                if let Some(program_id) =
                    self.get_program_id(tx_info, ParseableInstruction::Compiled(ix.1))
                {
                    //if program_id == self.config.pumpfun_program_id.to_string() {
                    //    self.process_pumpfun_instruction(ctx.bctx.cur_slot, sig, inst)
                    //        .await?;
                    //}
                    if program_id == self.config.pumpfun_program_id {
                        let pfun_create = pumpfun2::process_create_event(ctx, ix, sig);

                        if let Ok(pfun_create) = pfun_create {
                            debug!(
                                tx_url = format!("https://solscan.io/tx/{}", sig),
                                ?pfun_create,
                                "Processed PumpFun create event",
                            );

                            let event = MarketTrade {
                                slot: ctx.bctx.cur_slot,
                                signature: sig.to_string(),
                                log: crate::parser::TradeLog::PumpfunTokenCreate(pfun_create),
                            };

                            // Record stats before sending
                            self.record_trade_stats(&event).await;

                            if let Err(e) = self.trade_tx.send(event) {
                                error!(error = %e, "Failed to send PumpFun token creation log to channel");
                            }
                        }
                    }
                    if program_id == RAY_AMMV4_ID {
                        if let Some(ammv4_swap) = ray_ammv4::process_ammv4(ctx, ix) {
                            debug!(
                                swap = ?ammv4_swap,
                                "Processed AMMv4 swap"
                            );

                            let trade = MarketTrade {
                                slot: ctx.bctx.cur_slot,
                                signature: sig.to_string(),
                                log: crate::parser::TradeLog::RayAMMv4(ammv4_swap),
                            };

                            // Record stats and send the parsed log to the output channel
                            self.record_trade_stats(&trade).await;

                            if let Err(e) = self.trade_tx.send(trade) {
                                error!(error = %e, "Failed to send Raydium AMMv4 log to channel");
                            }
                        } else {
                            // Consider keeping count of failed AMMv4 swaps
                            // or some other diagnostic
                        }
                    }

                    if program_id == meteora_dlmm::ID {
                        if let Some(met_dlmm_swap) = meteora_dlmm::process_met_dlmm(ctx, ix) {
                            tracing::debug!(
                                "Processed Meteora DLMM swap: https://solscan.io/tx/{}: {:?}",
                                sig,
                                met_dlmm_swap
                            );

                            let trade = MarketTrade {
                                slot: ctx.bctx.cur_slot,
                                signature: sig.to_string(),
                                log: crate::parser::TradeLog::MeteoraDLMM(met_dlmm_swap),
                            };

                            // Record stats and send the parsed Meteora DLMM log to the output channel
                            self.record_trade_stats(&trade).await;

                            if let Err(e) = self.trade_tx.send(trade) {
                                error!(error = %e, "Failed to send Meteora DLMM log to channel");
                            }
                        }
                    }

                    if program_id == BONK_ID {
                        if let Some(bonk_swap) = bonk::process_bonk(ctx, ix) {
                            tracing::debug!(
                                "Processed Bonk swap: https://solscan.io/tx/{}: {:?}",
                                sig,
                                bonk_swap
                            );

                            let trade = MarketTrade {
                                slot: ctx.bctx.cur_slot,
                                signature: sig.to_string(),
                                log: crate::parser::TradeLog::Bonk(bonk_swap),
                            };

                            // Record stats and send the parsed Bonk log to the output channel
                            self.record_trade_stats(&trade).await;

                            if let Err(e) = self.trade_tx.send(trade) {
                                error!(error = %e, "Failed to send Bonk log to channel");
                            }
                        }
                    } else {
                        // consider logging or counting unparsable ixs
                    }
                }
            }
        }

        // do a separate pass on the inner instructions
        for inst in &meta.inner_instructions {
            for inner_inst in &inst.instructions {
                // Try to process as a PumpFun instruction
                if let Some(program_id) =
                    self.get_program_id(tx_info, ParseableInstruction::Inner(inner_inst))
                {
                    if program_id == self.config.pumpfun_program_id {
                        self.process_pumpfun_instruction(ctx.bctx.cur_slot, sig, inner_inst)
                            .await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Get the program ID for an instruction
    fn get_program_id(
        &self,
        tx_info: &SubscribeUpdateTransactionInfo,
        instruction: ParseableInstruction,
    ) -> Option<Pubkey> {
        let program_id_index = match instruction {
            ParseableInstruction::Inner(inner_inst) => inner_inst.program_id_index as usize,
            ParseableInstruction::Compiled(compiled_inst) => {
                compiled_inst.program_id_index as usize
            }
        };
        tx_info
            .transaction
            .as_ref()
            .and_then(|tx| tx.message.as_ref())
            .and_then(|msg| msg.account_keys.get(program_id_index))
            //.map(|key| bs58::encode(key).into_string())
            .map(|key| Pubkey::new_from_array(key.as_slice().try_into().unwrap()))
    }

    /// Process a PumpFun instruction
    async fn process_pumpfun_instruction(
        &self,
        slot: u64,
        signature: &str,
        instruction: &yellowstone_grpc_proto::prelude::InnerInstruction,
    ) -> Result<()> {
        // Check if data starts with PumpFun discriminator
        if instruction.data.len() >= 16 && &instruction.data[..16] == PUMPFUN_LOG_DISCRIMINATOR {
            match PumpfunLog::from_bytes(&instruction.data) {
                Ok(log) => {
                    tracing::debug!(
                        mint = %log.mint,
                        user = %log.user,
                        is_buy = log.is_buy,
                        sol_amount = log.sol_amount,
                        token_amount = log.token_amount,
                        "Processed PumpFun trade"
                    );

                    let trade = MarketTrade {
                        slot,
                        signature: signature.to_string(),
                        log: crate::parser::TradeLog::Pumpfun(log.clone()),
                    };

                    // Record stats and send the parsed pumpfun log to the output channel
                    self.record_trade_stats(&trade).await;

                    if let Err(e) = self.trade_tx.send(trade) {
                        error!(error = %e, "Failed to send PumpFun log to channel");
                    }
                }
                Err(e) => {
                    error!(error = ?e, "Failed to parse PumpFun log");
                }
            }
        }

        Ok(())
    }

    /// Lookup an address in the transaction based on index
    /// supports both static and dynamic address lookups
    pub fn lookup_addr(ctx: &mut TxnDecodeCtx, idx: u8) -> Option<Pubkey> {
        if (idx as usize) < ctx.static_keys.len() {
            return Some(ctx.static_keys[idx as usize]);
        }

        //let mut reconstitued_keys = Vec::new();
        //reconstitued_keys.extend_from_slice(static_keys);

        //if let raw_txn_meta = ctx.bctx.meta
        //&& let OptionSerializer::Some(loaded_addrs) = &raw_txn_meta.loaded_addresses
        let raw_txn_meta = ctx.bctx.meta;
        {
            let offset_len = idx as usize - ctx.static_keys.len();

            let loaded_writable_addrs = &raw_txn_meta.loaded_writable_addresses;
            let loaded_readonly_addrs = &raw_txn_meta.loaded_readonly_addresses;

            let (wr_loaded, rd_loaded): (Vec<String>, Vec<String>) = (
                //loaded_addrs.writable.iter().map(|e| e.as_str()).collect(),
                loaded_writable_addrs
                    .iter()
                    .map(|e| bs58::encode(e).into_string())
                    .collect(),
                //loaded_addrs.readonly.iter().map(|e| e.as_str()).collect(),
                loaded_readonly_addrs
                    .iter()
                    .map(|e| bs58::encode(e).into_string())
                    .collect(),
            );

            let (wr_loaded, rd_loaded) = (
                wr_loaded.iter().map(|e| e.as_str()).collect::<Vec<&str>>(),
                rd_loaded.iter().map(|e| e.as_str()).collect::<Vec<&str>>(),
            );

            let loaded_addrs = [wr_loaded, rd_loaded].concat();

            if offset_len < loaded_addrs.len() {
                trace!(
                    "+++++ [META] lookup_addr: idx={} ; loaded_addrs[{}]={}",
                    idx, offset_len, loaded_addrs[offset_len]
                );
                return Some(Pubkey::from_str_const(loaded_addrs[offset_len]));
            }
        }

        let (extended_lut, lut_account) = if ctx.txn_lut_cache.is_none() {
            let mut new_ext_lut = Vec::new();
            new_ext_lut.extend(
                ctx.static_keys
                    .iter()
                    .enumerate()
                    .map(|(i, k)| (i as u8, *k)),
            );

            while let Some(addr_lut) = ctx.addr_lookups {
                for lut_entry in addr_lut.iter() {
                    for wlut in &lut_entry.writable_indexes {
                        new_ext_lut.push((*wlut, lut_entry.account_key));
                    }
                    for rlut in &lut_entry.readonly_indexes {
                        new_ext_lut.push((*rlut, lut_entry.account_key));
                    }
                }
            }

            let lut_account_pkey = new_ext_lut[idx as usize].1;

            let t_0 = std::time::Instant::now();
            let lut_account = { ctx.bctx.conn.get_account(&lut_account_pkey) };
            let t_1 = std::time::Instant::now();

            trace!("-------> get_account took {:?}", t_1 - t_0);

            if let Ok(lut_account) = lut_account {
                trace!(
                    "+++++ [EXT0] lookup_addr: idx={} ; lut_account[{}]={}",
                    idx,
                    new_ext_lut[idx as usize].1,
                    lut_account.data.len()
                );
                ctx.txn_lut_cache.replace((new_ext_lut, lut_account));
            } else {
                return None;
            }
            ctx.txn_lut_cache.as_ref().unwrap()
        } else {
            // cache hit
            ctx.txn_lut_cache.as_ref().unwrap()
        };

        if idx >= extended_lut.len() as u8 {
            panic!(
                "\n++++ lookup_addr: idx={} out of bounds; reconstitued_keys.len={}: {:?}",
                idx,
                extended_lut.len(),
                extended_lut,
            );
        }

        return std::panic::catch_unwind(|| {
            let lut_idx = extended_lut[idx as usize].0;
            if let Ok(the_acct) = AddressLookupTable::deserialize(&lut_account.data)
                && the_acct.addresses.len() < lut_idx as usize
            {
                trace!(
                    "\n+++++ [EXT1] lookup_addr: idx={} ; lut_account[{}]={}",
                    idx,
                    extended_lut[idx as usize].1,
                    lut_account.data.len()
                );
                Some(the_acct.addresses[lut_idx as usize])
            } else {
                trace!(
                    "\n+++++ [REG] lookup_addr: idx={} ; lut_account[{}]={}",
                    idx,
                    extended_lut[idx as usize].1,
                    lut_account.data.len()
                );
                None
            }
        })
        .unwrap_or_else(|_| {
            error!(
                "\nPANIC would return LUT[{}] of LUT<{}>={:?}",
                idx,
                extended_lut.len(),
                extended_lut,
            );
            warn!(
                "\n+++++ lookup_addr: idx={} ; static_keys={:?}",
                idx, ctx.static_keys
            );
            // panic!("{:?}", arg);
            None
        });
    }
}
