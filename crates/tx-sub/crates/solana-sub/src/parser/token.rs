use solana_sdk::pubkey::Pubkey;
use spl_token::instruction::TokenInstruction;

use super::TxnDecodeCtx;

/// Parses the token transfer instruction from the given data and accounts.
///
/// @return An optional tuple containing the amount, from address, and to address.
pub fn get_token_xfer(
    ctx: &mut TxnDecodeCtx,
    data: &[u8],
    accounts: &[u8],
) -> Option<(u64, Pubkey, Pubkey)> {
    return TokenInstruction::unpack(data)
        .map(|tokins| match tokins {
            TokenInstruction::Transfer { amount } => {
                let from_to = [accounts[0], accounts[1]]
                    .iter()
                    .map(|&idx| crate::parser::TransactionParser::lookup_addr(ctx, idx))
                    .collect::<Vec<_>>();

                if let (Some(from), Some(to)) = (from_to[0], from_to[1]) {
                    Some((amount, from, to))
                } else {
                    None
                }
            }
            TokenInstruction::TransferChecked {
                amount,
                decimals: _,
            } => {
                let from_to = [accounts[0], accounts[1]]
                    .iter()
                    .map(|&idx| crate::parser::TransactionParser::lookup_addr(ctx, idx))
                    .collect::<Vec<_>>();

                if let (Some(from), Some(to)) = (from_to[0], from_to[1]) {
                    Some((amount, from, to))
                } else {
                    None
                }
            }
            _ => None,
        })
        .unwrap_or(None);
}
