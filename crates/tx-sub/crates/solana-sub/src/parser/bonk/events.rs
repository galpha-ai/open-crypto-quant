use super::types::{PoolStatus, TradeDirection};
use borsh::BorshDeserialize;
use solana_sdk::pubkey::Pubkey;

/// Bonk trade event data structure (borsh deserialized from inner instruction data)
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshDeserialize)]
pub struct BonkTradeEventData {
    pub pool_state: Pubkey,
    pub total_base_sell: u64,
    pub virtual_base: u64,
    pub virtual_quote: u64,
    pub real_base_before: u64,
    pub real_quote_before: u64,
    pub real_base_after: u64,
    pub real_quote_after: u64,
    pub amount_in: u64,
    pub amount_out: u64,
    pub protocol_fee: u64,
    pub platform_fee: u64,
    pub share_fee: u64,
    pub trade_direction: TradeDirection,
    pub pool_status: PoolStatus,
}

/// Event discriminator constants
pub mod discriminators {
    // Event discriminators (used to identify inner instruction log types)
    // 0xe445a52e51cb9a1dbddb7fd34ee661ee
    pub const TRADE_EVENT: &[u8] = &[
        0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d, 0xbd, 0xdb, 0x7f, 0xd3, 0x4e, 0xe6, 0x61,
        0xee,
    ];
    // 0xe445a52e51cb9a1d97d7e20976a173ae
    pub const POOL_CREATE_EVENT: &[u8] = &[
        0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d, 0x97, 0xd7, 0xe2, 0x09, 0x76, 0xa1, 0x73,
        0xae,
    ];

    // Instruction discriminators (used to determine trade direction)
    pub const BUY_EXACT_IN: &[u8] = &[250, 234, 13, 123, 213, 156, 19, 236];
    pub const BUY_EXACT_OUT: &[u8] = &[24, 211, 116, 40, 105, 3, 153, 56];
    pub const SELL_EXACT_IN: &[u8] = &[149, 39, 222, 155, 211, 124, 152, 26];
    pub const SELL_EXACT_OUT: &[u8] = &[95, 200, 71, 34, 8, 9, 11, 166];
}
