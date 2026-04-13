#![allow(warnings)]

solana_program::declare_id!("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");

//pub mod accounts;
//pub use accounts::*;
pub mod typedefs;
use popeyes_trading_types::MetDLMMSwapDirection;
use solana_sdk::{instruction::CompiledInstruction, pubkey::Pubkey};
pub use typedefs::*;
pub mod instructions;
pub use instructions::*;
pub mod errors;
pub use errors::*;
pub mod events;
pub use events::*;

use super::TxnDecodeCtx;

#[derive(Debug, Clone, Copy)]
pub struct MetDLMMSwap {
    pub direction: MetDLMMSwapDirection,
    pub market: Pubkey,
    pub input_token_mint: Pubkey,
    pub output_token_mint: Pubkey,
    pub qty_in: u64,
    pub qty_out: u64,
    pub user: Pubkey,
    pub timestamp: i64,
}

impl MetDLMMSwap {
    pub(crate) fn new(
        direction: MetDLMMSwapDirection,
        market: Pubkey,
        input_token_mint: Pubkey,
        output_token_mint: Pubkey,
        qty_in: u64,
        qty_out: u64,
        user: Pubkey,
    ) -> Self {
        Self {
            direction,
            market,
            input_token_mint,
            output_token_mint,
            qty_in,
            qty_out,
            user,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
    pub fn is_base_in(&self) -> bool {
        self.direction == MetDLMMSwapDirection::BaseIn
    }
    pub fn is_base_out(&self) -> bool {
        self.direction == MetDLMMSwapDirection::BaseOut
    }
}

pub fn process_met_dlmm(
    ctx: &mut TxnDecodeCtx,
    insn: (usize, &CompiledInstruction),
) -> Option<MetDLMMSwap> {
    if let Ok(dlmm_ix) = LbClmmProgramIx::deserialize(&insn.1.data) {
        match dlmm_ix {
            LbClmmProgramIx::Swap(SwapIxArgs {
                amount_in,
                min_amount_out,
            }) => {
                let market_pubkey =
                    super::parser::TransactionParser::lookup_addr(ctx, insn.1.accounts[0]);

                let token_x_mint =
                    super::parser::TransactionParser::lookup_addr(ctx, insn.1.accounts[6]);

                let token_y_mint =
                    super::parser::TransactionParser::lookup_addr(ctx, insn.1.accounts[7]);

                let signer =
                    super::parser::TransactionParser::lookup_addr(ctx, insn.1.accounts[10]);

                let token_x_pool =
                    super::parser::TransactionParser::lookup_addr(ctx, insn.1.accounts[2]);

                let token_y_pool =
                    super::parser::TransactionParser::lookup_addr(ctx, insn.1.accounts[3]);

                let ix_inners = ctx
                    .bctx
                    .meta
                    .inner_instructions
                    .iter()
                    .find(|x| x.index == insn.0 as u32);

                if let Some(ix_inners) = ix_inners
                    && let Some(market_pubkey) = market_pubkey
                    && let Some(token_x_mint) = token_x_mint
                    && let Some(token_y_mint) = token_y_mint
                    && let Some(signer) = signer
                    && let Some(token_x_pool) = token_x_pool
                    && let Some(token_y_pool) = token_y_pool
                {
                    let token_xfer = ix_inners.instructions.iter().find_map(|inner_ix| {
                        let token_xfer =
                            super::token::get_token_xfer(ctx, &inner_ix.data, &inner_ix.accounts);

                        if token_xfer.is_none() {
                            return None;
                        }

                        let token_xfer = token_xfer.unwrap();

                        if token_xfer.0 == amount_in {
                            return None;
                        }

                        if token_xfer.1 == token_y_pool {
                            return Some((token_y_pool, token_xfer));
                        }

                        if token_xfer.1 == token_x_pool {
                            return Some((token_x_pool, token_xfer));
                        }

                        None
                    });

                    if let Some(token_xfer) = token_xfer {
                        let (swap_dir, in_token, out_token) = if token_xfer.0 == token_y_pool {
                            (MetDLMMSwapDirection::BaseIn, token_x_mint, token_y_mint)
                        } else {
                            (MetDLMMSwapDirection::BaseOut, token_y_mint, token_x_mint)
                        };

                        let swap = MetDLMMSwap {
                            direction: swap_dir,
                            market: market_pubkey,
                            input_token_mint: in_token,
                            output_token_mint: out_token,
                            qty_in: amount_in,
                            qty_out: token_xfer.1.0,
                            user: signer,
                            timestamp: chrono::Utc::now().timestamp(),
                        };

                        return Some(swap);
                    }
                }
            }
            _ => {}
        }
    }

    None
}
