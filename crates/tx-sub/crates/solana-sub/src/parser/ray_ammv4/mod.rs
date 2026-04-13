//use color_eyre::owo_colors::OwoColorize;
//use solana_pubkey::Pubkey;
use solana_program::pubkey::Pubkey;
use solana_sdk::{instruction::CompiledInstruction, signer};
use state::Loadable;

use super::TxnDecodeCtx;
use crate::{
    parser::ray_ammv4,
    //sol_px::SOL_PRICE_FEED,
    //unirok::{self, RayAMMv4Swap},
    //WSOL_ID,
};

pub mod error;
pub mod instruction;
pub mod invokers;
pub mod log;
pub mod math;
pub mod processor;
pub mod state;

#[derive(PartialEq, Debug, Clone)]
pub enum Ray4AMMv4SwapDirection {
    BaseIn,
    BaseOut,
}

#[derive(Debug, Clone)]
pub struct RayAMMv4Swap {
    pub direction: Ray4AMMv4SwapDirection,
    pub market: Pubkey,
    pub input_token_mint: Pubkey,
    pub output_token_mint: Pubkey,
    pub qty_in: u64,
    //pub qty_in_f64: f64,
    pub qty_out: u64,
    //pub qty_out_f64: f64,
    //pub qty_usd: f64,
    pub user: Pubkey,
    pub timestamp: i64,
    pub inverted: bool,
    pub input_decimals: u8,
    pub output_decimals: u8,
}

pub struct MarketsInfo {
    markets: std::collections::HashMap<Pubkey, ray_ammv4::state::AmmInfo>,
}

impl MarketsInfo {
    pub fn lookup(&self, market: &Pubkey) -> Option<&ray_ammv4::state::AmmInfo> {
        self.markets.get(market)
    }

    pub fn insert(&mut self, market: Pubkey, info: ray_ammv4::state::AmmInfo) {
        self.markets.insert(market, info);
    }

    pub fn lookup_or_insert_with<F: FnOnce() -> ray_ammv4::state::AmmInfo>(
        &mut self,
        market: &Pubkey,
        f: F,
    ) -> &ray_ammv4::state::AmmInfo {
        self.markets.entry(*market).or_insert_with(f)
    }

    pub fn lookup_or_insert_if<F: FnOnce() -> Option<ray_ammv4::state::AmmInfo>>(
        &mut self,
        market: &Pubkey,
        f: F,
    ) -> Option<&ray_ammv4::state::AmmInfo> {
        let entry = self.markets.entry(*market);

        return if let std::collections::hash_map::Entry::Vacant(_) = entry {
            //eprintln!("lookup_or_insert_if: market={}", market);
            if let Some(info) = f() {
                entry.insert_entry(info);
                Some(self.markets.get(market).unwrap())
            } else {
                None
            }
        } else {
            Some(self.markets.get(market).unwrap())
        };
    }
}

static MARKET_INFO_CACHE: once_cell::sync::Lazy<std::sync::RwLock<MarketsInfo>> =
    once_cell::sync::Lazy::new(|| {
        std::sync::RwLock::new(MarketsInfo {
            markets: std::collections::HashMap::new(),
        })
    });

pub fn process_ammv4(
    ctx: &mut TxnDecodeCtx,
    cmp_insn: (usize, &CompiledInstruction),
) -> Option<RayAMMv4Swap> {
    //MARKET_INFO_CACHE
    //    .write()
    //    .unwrap()
    //    .markets
    //    .insert(Pubkey::default(), ray_ammv4::state::AmmInfo::default());

    let (insn_idx, cmp_insn) = cmp_insn;

    let amm_market_idx = super::parser::TransactionParser::lookup_addr(ctx, cmp_insn.accounts[1]);

    if let Some(swap_info) =
        if let Ok(ammv4_insn) = &ray_ammv4::instruction::AmmInstruction::unpack(&cmp_insn.data) {
            match ammv4_insn {
                ray_ammv4::instruction::AmmInstruction::SwapBaseIn(swap_in_insn) => Some((
                    Ray4AMMv4SwapDirection::BaseIn,
                    swap_in_insn.amount_in,
                    swap_in_insn.minimum_amount_out,
                )),
                ray_ammv4::instruction::AmmInstruction::SwapBaseOut(swap_out_insn) => Some((
                    Ray4AMMv4SwapDirection::BaseOut,
                    swap_out_insn.amount_out,
                    swap_out_insn.max_amount_in,
                )),
                _ => {
                    eprintln!(
                        ">>>>> [AMMv4] warning: not a swap instruction; txn: {:?}; insn: {:?}",
                        ctx.bctx.ver_txn, ammv4_insn
                    );
                    None
                }
            }
            //ctx.bctx.metrics.trades_total.as_ref().unwrap().inc();
        } else {
            eprintln!(
                ">>>>> [AMMv4] warning: cannot unpack instruction; txn: {:?}; insn: {:?}",
                ctx.bctx.ver_txn, cmp_insn
            );
            None
        }
    {
        let signer =
            super::parser::TransactionParser::lookup_addr(ctx, *cmp_insn.accounts.last().unwrap())
                .unwrap();

        let (direction, amount_in_or_out, _min_or_max) = swap_info;

        if let Some(amm_market) = &amm_market_idx
            && let Some(amm_market_info) =
                MARKET_INFO_CACHE
                    .write()
                    .unwrap()
                    .lookup_or_insert_if(amm_market, || {
                        let client = ctx.bctx.conn;

                        if let Ok(amm_market_data_raw) = client.get_account_data(amm_market)
                            && let Ok(amm_market_data) =
                                ray_ammv4::state::AmmInfo::load_from_bytes(&amm_market_data_raw)
                        {
                            return Some(*amm_market_data);
                        }
                        None
                    })
        {
            let inners = ctx
                .bctx
                .meta
                .inner_instructions
                //.as_ref()
                //.unwrap()
                .iter()
                .find(|inner| inner.index == insn_idx as u32);

            if inners.is_none() {
                eprintln!(
                    ">>>>> [AMMv4] warning: cannot find inner instructions;\n\t >txn: {:?}\n\t >insn: {:?}\n\t >meta: {:?}\n\t >idx={}",
                    ctx.bctx.ver_txn, cmp_insn, ctx.bctx.meta, insn_idx
                );
                return None;
            }

            let inners = inners.unwrap();

            if inners.instructions.len() < 2 {
                eprintln!("warn: insn_inners.len < 2: {:?}", inners);
                //panic!(
                //    "insn_inners.instructions.len()={} < 2",
                //    inners.instructions.len()
                //);
                return None;
            }

            let inner1 = super::token::get_token_xfer(
                ctx,
                &inners.instructions[0].data,
                &inners.instructions[0].accounts,
            );

            let inner2 = super::token::get_token_xfer(
                ctx,
                &inners.instructions[1].data,
                &inners.instructions[1].accounts,
            );

            if inner1.is_none() || inner2.is_none() {
                eprintln!(
                    "warning: inner1 or inner2 is None; txn: {:?}; inner1: {:?}; inner2: {:?}",
                    ctx.bctx.ver_txn, inner1, inner2
                );
                return None;
            }

            let (inner1, inner2) = (inner1.unwrap(), inner2.unwrap());

            //let pool_pc_acct =
            //    super::parser::TransactionParser::lookup_addr(ctx, cmp_insn.accounts[6]).unwrap();

            let is_inverted = inner1.2 == amm_market_info.pc_vault;

            let matching_inner = if direction == Ray4AMMv4SwapDirection::BaseIn {
                &inner2
            } else {
                &inner1
            };

            let coin_mint = &amm_market_info.coin_vault_mint;
            let pc_mint = &amm_market_info.pc_vault_mint;

            let (input_mint, output_mint, amount_in, amount_out, input_decimals, output_decimals) =
                if direction == Ray4AMMv4SwapDirection::BaseIn {
                    if is_inverted {
                        (
                            pc_mint,
                            coin_mint,
                            amount_in_or_out,
                            matching_inner.0,
                            amm_market_info.pc_decimals,
                            amm_market_info.coin_decimals,
                        )
                    } else {
                        (
                            coin_mint,
                            pc_mint,
                            amount_in_or_out,
                            matching_inner.0,
                            amm_market_info.coin_decimals,
                            amm_market_info.pc_decimals,
                        )
                    }
                } else {
                    if is_inverted {
                        (
                            pc_mint,
                            coin_mint,
                            matching_inner.0,
                            amount_in_or_out,
                            amm_market_info.pc_decimals,
                            amm_market_info.coin_decimals,
                        )
                    } else {
                        (
                            coin_mint,
                            pc_mint,
                            matching_inner.0,
                            amount_in_or_out,
                            amm_market_info.coin_decimals,
                            amm_market_info.pc_decimals,
                        )
                    }
                };

            let ammv4_grok = //new_from_ray_ammv4_swap(
                RayAMMv4Swap {
                    direction,
                    market: *amm_market,
                    input_token_mint: *input_mint,
                    output_token_mint: *output_mint,
                    qty_in: amount_in,
                    //qty_in_f64,
                    qty_out: amount_out,
                    //qty_out_f64,
                    //qty_usd,
                    timestamp: chrono::Utc::now().timestamp(),
                    user: signer,
                    inverted: is_inverted,
                    input_decimals: input_decimals as u8,
                    output_decimals: output_decimals as u8,
                };
            //signer,
            //ctx.bctx.cur_slot,
            //ctx.bctx.block_time,
            //ctx.bctx.rx_time,
            //);

            return Some(ammv4_grok);
        } else {
            eprintln!(
                ">>>>> [AMMv4] warning: cannot find market/info; txn: {:?}; insn: {:?}",
                ctx.bctx.ver_txn, cmp_insn
            );
        }
    }

    None
}
