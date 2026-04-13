use super::{
    events::{BonkTradeEventData, discriminators},
    types::{BonkSwap, TradeDirection},
};
use crate::parser::TxnDecodeCtx;
use borsh::BorshDeserialize;
use solana_sdk::{instruction::CompiledInstruction, pubkey::Pubkey};
use tracing::{debug, warn};

/// Bonk program ID
pub const BONK_ID: Pubkey = solana_sdk::pubkey!("LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj");

/// Process a Bonk transaction
pub fn process_bonk<'a>(
    ctx: &mut TxnDecodeCtx<'a>,
    ix: (usize, &'a CompiledInstruction),
) -> Option<BonkSwap> {
    let (ix_idx, inst) = ix;

    // Check if instruction data starts with valid discriminator
    if inst.data.len() < 8 {
        return None;
    }

    let discriminator = &inst.data[..8];
    let trade_direction = match discriminator {
        discriminators::BUY_EXACT_IN | discriminators::BUY_EXACT_OUT => TradeDirection::Buy,
        discriminators::SELL_EXACT_IN | discriminators::SELL_EXACT_OUT => TradeDirection::Sell,
        _ => return None,
    };

    debug!(
        discriminator = ?discriminator,
        direction = ?trade_direction,
        "Processing Bonk instruction"
    );

    // Extract account addresses
    let accounts = inst.accounts.as_slice();
    if accounts.len() < 11 {
        warn!("Insufficient accounts for Bonk swap instruction");
        return None;
    }

    // Map accounts based on Bonk instruction layout
    let payer_idx = accounts.get(0)?;
    let pool_state_idx = accounts.get(4)?;
    let _user_base_token_idx = accounts.get(5)?;
    let _user_quote_token_idx = accounts.get(6)?;
    let _base_vault_idx = accounts.get(7)?;
    let _quote_vault_idx = accounts.get(8)?;
    let base_mint_idx = accounts.get(9)?;
    let quote_mint_idx = accounts.get(10)?;

    let payer = ctx.static_keys.get(*payer_idx as usize)?;
    let pool_state = ctx.static_keys.get(*pool_state_idx as usize)?;
    let base_mint = ctx.static_keys.get(*base_mint_idx as usize)?;
    let quote_mint = ctx.static_keys.get(*quote_mint_idx as usize)?;

    // Look for the trade event in inner instructions
    let meta = ctx.bctx.meta;
    for inner_ixs in &meta.inner_instructions {
        if inner_ixs.index as usize == ix_idx {
            debug!(
                "Found {} inner instructions for Bonk instruction at index {}",
                inner_ixs.instructions.len(),
                ix_idx
            );
            for (inner_idx, inner_ix) in inner_ixs.instructions.iter().enumerate() {
                let program_id_idx = inner_ix.program_id_index;
                let program_id = ctx.static_keys.get(program_id_idx as usize)?;

                debug!(
                    "Inner instruction {}: program_id_idx={}, program_id={}, data_len={}",
                    inner_idx,
                    program_id_idx,
                    program_id,
                    inner_ix.data.len()
                );

                if program_id == &BONK_ID {
                    // Check if this is a trade event log
                    // Note: gRPC transactions provide raw bytes, not base64
                    let data = &inner_ix.data;
                    debug!(
                        "Checking Bonk inner instruction data: first 32 bytes = {:?}",
                        &data[..32.min(data.len())]
                    );
                    if data.starts_with(discriminators::TRADE_EVENT) {
                        // Skip discriminator (16 bytes) and parse event data
                        let event_data = &data[16..];

                        if let Ok(trade_event) =
                            BonkTradeEventData::deserialize(&mut event_data.as_ref())
                        {
                            debug!(
                                pool_state = %trade_event.pool_state,
                                amount_in = trade_event.amount_in,
                                amount_out = trade_event.amount_out,
                                "Parsed Bonk trade event"
                            );

                            return Some(BonkSwap {
                                direction: trade_event.trade_direction,
                                pool_address: pool_state.to_string(),
                                base_mint: base_mint.to_string(),
                                quote_mint: quote_mint.to_string(),
                                base_amount: match trade_event.trade_direction {
                                    TradeDirection::Buy => trade_event.amount_out,
                                    TradeDirection::Sell => trade_event.amount_in,
                                },
                                quote_amount: match trade_event.trade_direction {
                                    TradeDirection::Buy => trade_event.amount_in,
                                    TradeDirection::Sell => trade_event.amount_out,
                                },
                                user: payer.to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    None
}
