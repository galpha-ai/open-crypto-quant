//! Program state processor
#![allow(deprecated)]
use std::{
    cell::{Ref, RefMut},
    collections::VecDeque,
    convert::{TryFrom, identity},
    mem::size_of,
    num::NonZeroU64,
    ops::Deref,
};

use arrayref::{array_ref, array_refs};
use arrform::{ArrForm, arrform};
use serum_dex::{
    critbit::{LeafNode, Slab, SlabView},
    matching::{OrderType, Side},
    state::{Market, MarketState, OpenOrders, ToAlignedBytes},
};
use solana_program::{
    account_info::{AccountInfo, next_account_info},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    // log::sol_log_compute_units,
    program_error::ProgramError,
    program_option::COption,
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction,
    sysvar::{Sysvar, clock},
};

use super::log::*;
use crate::{
    check_assert_eq,
    parser::ray_ammv4::{
        error::AmmError,
        instruction::{
            AdminCancelOrdersInstruction, AmmInstruction, ConfigArgs, DepositInstruction,
            InitializeInstruction2, MonitorStepInstruction, SetParamsInstruction,
            SimulateInstruction, SwapInstructionBaseIn, SwapInstructionBaseOut,
            WithdrawInstruction, WithdrawSrmInstruction,
        },
        invokers::Invokers,
        math::{
            Calculator, CheckedCeilDiv, InvariantPool, InvariantToken, RoundDirection,
            SwapDirection, U128, U256,
        },
        state::{
            AmmConfig, AmmInfo, AmmParams, AmmResetFlag, AmmState, AmmStatus, GetPoolData,
            GetSwapBaseInData, GetSwapBaseOutData, Loadable, MAX_ORDER_LIMIT, RunCrankData,
            SimulateParams, TEN_THOUSAND, TargetOrders,
        },
    },
};

pub mod srm_token {
    solana_program::declare_id!("SRMuApVNdxXokk5GT7XD5cUUgXMBCoAz2LHeuAoKWRt");
}

pub mod msrm_token {
    solana_program::declare_id!("MSRMcoVyrFxnSgo5uXwone5SKcGhT1KEJMFEkMEWf9L");
}

#[cfg(feature = "testnet")]
pub mod config_feature {
    pub mod amm_owner {
        solana_program::declare_id!("75KWb5XcqPTgacQyNw9P5QU2HL3xpezEVcgsFCiJgTT");
    }
    pub mod openbook_program {
        solana_program::declare_id!("6ccSma8mmmmQXcSFpheSKrTnwsCe5pBuEpzDLFjrAsCF");
    }
    pub mod referrer_pc_wallet {
        solana_program::declare_id!("75KWb5XcqPTgacQyNw9P5QU2HL3xpezEVcgsFCiJgTT");
    }
    pub mod create_pool_fee_address {
        solana_program::declare_id!("3TRTX4dXUpp2eqxi3tvQDFYUV7SdDJjcPE3Y4mbtftaX");
    }
}
#[cfg(feature = "devnet")]
pub mod config_feature {
    pub mod amm_owner {
        solana_program::declare_id!("Adm29NctkKwJGaaiU8CXqdV6WDTwR81JbxV8zoxn745Y");
    }
    pub mod openbook_program {
        solana_program::declare_id!("EoTcMgcDRTJVZDMZWBoU6rhYHZfkNTVEAfz3uUJRcYGj");
    }
    pub mod referrer_pc_wallet {
        solana_program::declare_id!("4NpMfWThvJQsV9VLjUXXpn3tPv1zoQpib8wCBDc1EBzD");
    }
    pub mod create_pool_fee_address {
        solana_program::declare_id!("3XMrhbv989VxAMi3DErLV9eJht1pHppW5LbKxe9fkEFR");
    }
}
#[cfg(not(any(feature = "testnet", feature = "devnet")))]
pub mod config_feature {
    pub mod amm_owner {
        solana_program::declare_id!("GThUX1Atko4tqhN2NaiTazWSeFWMuiUvfFnyJyUghFMJ");
    }
    pub mod openbook_program {
        solana_program::declare_id!("srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX");
    }
    pub mod referrer_pc_wallet {
        solana_program::declare_id!("FCxGKqGSVeV1d3WsmAXt45A5iQdCS6kKCeJy3EUBigMG");
    }
    pub mod create_pool_fee_address {
        solana_program::declare_id!("7YttLkHDoNj9wyDur5pM1ejNaAvT9X4eqaYcHQqtj2G5");
    }
}

/// Suffix for amm authority seed
pub const AUTHORITY_AMM: &'static [u8] = b"amm authority";
/// Suffix for amm associated seed
pub const AMM_ASSOCIATED_SEED: &'static [u8] = b"amm_associated_seed";
/// Suffix for target associated seed
pub const TARGET_ASSOCIATED_SEED: &'static [u8] = b"target_associated_seed";
/// Suffix for amm open order associated seed
pub const OPEN_ORDER_ASSOCIATED_SEED: &'static [u8] = b"open_order_associated_seed";
/// Suffix for coin vault associated seed
pub const COIN_VAULT_ASSOCIATED_SEED: &'static [u8] = b"coin_vault_associated_seed";
/// Suffix for pc vault associated seed
pub const PC_VAULT_ASSOCIATED_SEED: &'static [u8] = b"pc_vault_associated_seed";
/// Suffix for lp mint associated seed
pub const LP_MINT_ASSOCIATED_SEED: &'static [u8] = b"lp_mint_associated_seed";
/// Amm config seed
pub const AMM_CONFIG_SEED: &'static [u8] = b"amm_config_account_seed";

pub fn get_associated_address_and_bump_seed(
    info_id: &Pubkey,
    market_address: &Pubkey,
    associated_seed: &[u8],
    program_id: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            &info_id.to_bytes(),
            &market_address.to_bytes(),
            &associated_seed,
        ],
        program_id,
    )
}

/// Program state handler.
pub struct Processor {}
impl Processor {
    #[inline]
    fn check_account_readonly(account_info: &AccountInfo) -> ProgramResult {
        if account_info.is_writable {
            return Err(AmmError::AccountNeedReadOnly.into());
        }
        return Ok(());
    }

    /// Unpacks a spl_token `Account`.
    #[inline]
    pub fn unpack_token_account(
        account_info: &AccountInfo,
        token_program_id: &Pubkey,
    ) -> Result<spl_token::state::Account, AmmError> {
        if account_info.owner != token_program_id {
            Err(AmmError::InvalidSplTokenProgram)
        } else {
            spl_token::state::Account::unpack(&account_info.data.borrow())
                .map_err(|_| AmmError::ExpectedAccount)
        }
    }

    /// Unpacks a spl_token `Mint`.
    #[inline]
    pub fn unpack_mint(
        account_info: &AccountInfo,
        token_program_id: &Pubkey,
    ) -> Result<spl_token::state::Mint, AmmError> {
        if account_info.owner != token_program_id {
            Err(AmmError::InvalidSplTokenProgram)
        } else {
            spl_token::state::Mint::unpack(&account_info.data.borrow())
                .map_err(|_| AmmError::ExpectedMint)
        }
    }

    // pub fn load_serum_market_order<'a>(
    //     market_acc: &AccountInfo<'a>,
    //     open_orders_acc: &AccountInfo<'a>,
    //     authority_acc: &AccountInfo<'a>,
    //     amm: &AmmInfo,
    //     // Allow for the market flag to be set to AccountFlag::Disabled
    //     allow_disabled: bool,
    // ) -> Result<(Box<MarketState>, Box<OpenOrders>), ProgramError> {
    //     let market_state = Market::load(market_acc, &amm.market_program, allow_disabled)?;
    //     let open_orders = OpenOrders::load_checked(
    //         open_orders_acc,
    //         Some(market_acc),
    //         Some(authority_acc),
    //         &amm.market_program,
    //     )?;
    //     if identity(open_orders.market) != market_acc.key.to_aligned_bytes() {
    //         return Err(AmmError::InvalidMarket.into());
    //     }
    //     if identity(open_orders.owner) != authority_acc.key.to_aligned_bytes() {
    //         return Err(AmmError::InvalidOwner.into());
    //     }
    //     if *open_orders_acc.key != amm.open_orders {
    //         return Err(AmmError::InvalidOpenOrders.into());
    //     }
    //     return Ok((
    //         Box::new(*market_state.deref()),
    //         Box::new(*open_orders.deref()),
    //     ));
    // }

    // fn _get_dex_best_price(slab: &RefMut<serum_dex::critbit::Slab>, side: Side) -> Option<u64> {
    //     if slab.is_empty() {
    //         None
    //     } else {
    //         match side {
    //             Side::Bid => {
    //                 let best_bid_h = slab.find_max().unwrap();
    //                 let best_bid_px = slab
    //                     .get(best_bid_h)
    //                     .unwrap()
    //                     .as_leaf()
    //                     .unwrap()
    //                     .price()
    //                     .get();
    //                 Some(best_bid_px)
    //             }
    //             Side::Ask => {
    //                 let best_ask_h = slab.find_min().unwrap();
    //                 let best_ask_px = slab
    //                     .get(best_ask_h)
    //                     .unwrap()
    //                     .as_leaf()
    //                     .unwrap()
    //                     .price()
    //                     .get();
    //                 Some(best_ask_px)
    //             }
    //         }
    //     }
    // }

    //fn get_amm_orders(
    //    open_orders: &OpenOrders,
    //    bids: Ref<Slab>,
    //    asks: Ref<Slab>,
    //) -> Result<(Vec<LeafNode>, Vec<LeafNode>), ProgramError> {
    //    let orders_number = open_orders.free_slot_bits.count_zeros();
    //    let mut bids_orders: Vec<LeafNode> = Vec::new();
    //    let mut asks_orders: Vec<LeafNode> = Vec::new();
    //    if orders_number != 0 {
    //        for i in 0..128 {
    //            let slot_mask = 1u128 << i;
    //            if open_orders.free_slot_bits & slot_mask != 0 {
    //                // means slot is free
    //                continue;
    //            }
    //            if open_orders.is_bid_bits & slot_mask != 0 {
    //                match bids.find_by_key(open_orders.orders[i]) {
    //                    None => continue,
    //                    Some(handle_bid) => {
    //                        let handle_bid_ref = bids.get(handle_bid).unwrap().as_leaf().unwrap();
    //                        bids_orders.push(*handle_bid_ref);
    //                    }
    //                }
    //            } else {
    //                match asks.find_by_key(open_orders.orders[i]) {
    //                    None => continue,
    //                    Some(handle_ask) => {
    //                        let handle_ask_ref = asks.get(handle_ask).unwrap().as_leaf().unwrap();
    //                        asks_orders.push(*handle_ask_ref);
    //                    }
    //                }
    //            };
    //        }
    //    }
    //    bids_orders.sort_by(|a, b| b.price().get().cmp(&a.price().get()));
    //    asks_orders.sort_by(|a, b| a.price().get().cmp(&b.price().get()));
    //    Ok((bids_orders, asks_orders))
    //}

    // pub fn get_amm_best_price(
    //     market_state: &MarketState,
    //     open_orders: &OpenOrders,
    //     bids_account: &AccountInfo,
    //     asks_account: &AccountInfo,
    // ) -> Result<(Option<u64>, Option<u64>), ProgramError> {
    //     let bids = market_state.load_bids_checked(&bids_account)?;
    //     let asks = market_state.load_asks_checked(&asks_account)?;
    //     let (bids_orders, asks_orders) = Self::get_amm_orders(open_orders, bids, asks)?;
    //     let mut bid_best_price = None;
    //     let mut ask_best_price = None;
    //     if bids_orders.len() != 0 {
    //         bid_best_price = Some(bids_orders.first().unwrap().price().get());
    //     }
    //     if asks_orders.len() != 0 {
    //         ask_best_price = Some(asks_orders.first().unwrap().price().get());
    //     }
    //     Ok((bid_best_price, ask_best_price))
    // }

    // pub fn get_amm_worst_price(
    //     market_state: &MarketState,
    //     open_orders: &OpenOrders,
    //     bids_account: &AccountInfo,
    //     asks_account: &AccountInfo,
    // ) -> Result<(Option<u64>, Option<u64>), ProgramError> {
    //     let bids = market_state.load_bids_mut(&mut bids_account)?;
    //     let asks = market_state.load_asks_mut(&mut asks_account)?;
    //     let (bids_orders, asks_orders) = Self::get_amm_orders(open_orders, bids, asks)?;
    //     let mut bid_best_price = None;
    //     let mut ask_best_price = None;
    //     if bids_orders.len() != 0 {
    //         bid_best_price = Some(bids_orders.last().unwrap().price().get());
    //     }
    //     if asks_orders.len() != 0 {
    //         ask_best_price = Some(asks_orders.last().unwrap().price().get());
    //     }
    //     Ok((bid_best_price, ask_best_price))
    // }

    /// The Detailed calculation of pnl
    /// 1. calc last_k witch dose not take pnl: last_k = calc_pnl_x * calc_pnl_y;
    /// 2. calc current price: current_price = current_x / current_y;
    /// 3. calc x after take pnl: x_after_take_pnl = sqrt(last_k * current_price);
    /// 4. calc y after take pnl: y_after_take_pnl = x_after_take_pnl / current_price;
    ///                           y_after_take_pnl = x_after_take_pnl * current_y / current_x;
    /// 5. calc pnl_x & pnl_y:  pnl_x = current_x - x_after_take_pnl;
    ///                         pnl_y = current_y - y_after_take_pnl;
    pub fn calc_take_pnl(
        target: &TargetOrders,
        amm: &mut AmmInfo,
        total_pc_without_take_pnl: &mut u64,
        total_coin_without_take_pnl: &mut u64,
        x1: U256,
        y1: U256,
    ) -> Result<(u128, u128), ProgramError> {
        // calc pnl
        let mut delta_x: u128;
        let mut delta_y: u128;
        let calc_pc_amount = Calculator::restore_decimal(
            target.calc_pnl_x.into(),
            amm.pc_decimals,
            amm.sys_decimal_value,
        );
        let calc_coin_amount = Calculator::restore_decimal(
            target.calc_pnl_y.into(),
            amm.coin_decimals,
            amm.sys_decimal_value,
        );
        let pool_pc_amount = U128::from(*total_pc_without_take_pnl);
        let pool_coin_amount = U128::from(*total_coin_without_take_pnl);
        if pool_pc_amount.checked_mul(pool_coin_amount).unwrap()
            >= (calc_pc_amount).checked_mul(calc_coin_amount).unwrap()
        {
            // last k is
            // let last_k: u128 = (target.calc_pnl_x as u128).checked_mul(target.calc_pnl_y as u128).unwrap();
            // current k is
            // let current_k: u128 = (x1 as u128).checked_mul(y1 as u128).unwrap();
            // current p is
            // let current_p: u128 = (x1 as u128).checked_div(y1 as u128).unwrap();
            let x2_power = Calculator::calc_x_power(
                target.calc_pnl_x.into(),
                target.calc_pnl_y.into(),
                x1,
                y1,
            );
            // let x2 = Calculator::sqrt(x2_power).unwrap();
            let x2 = x2_power.integer_sqrt();
            // msg!(arrform!(LOG_SIZE, "calc_take_pnl x2_power:{}, x2:{}", x2_power, x2).as_str());
            let y2 = x2.checked_mul(y1).unwrap().checked_div(x1).unwrap();
            // msg!(arrform!(LOG_SIZE, "calc_take_pnl y2:{}", y2).as_str());

            // transfer to token_coin_pnl and token_pc_pnl
            // (x1 -x2) * pnl / sys_decimal_value
            let diff_x = U128::from(x1.checked_sub(x2).unwrap().as_u128());
            let diff_y = U128::from(y1.checked_sub(y2).unwrap().as_u128());
            delta_x = diff_x
                .checked_mul(amm.fees.pnl_numerator.into())
                .unwrap()
                .checked_div(amm.fees.pnl_denominator.into())
                .unwrap()
                .as_u128();
            delta_y = diff_y
                .checked_mul(amm.fees.pnl_numerator.into())
                .unwrap()
                .checked_div(amm.fees.pnl_denominator.into())
                .unwrap()
                .as_u128();

            let diff_pc_pnl_amount =
                Calculator::restore_decimal(diff_x, amm.pc_decimals, amm.sys_decimal_value);
            let diff_coin_pnl_amount =
                Calculator::restore_decimal(diff_y, amm.coin_decimals, amm.sys_decimal_value);
            let pc_pnl_amount = diff_pc_pnl_amount
                .checked_mul(amm.fees.pnl_numerator.into())
                .unwrap()
                .checked_div(amm.fees.pnl_denominator.into())
                .unwrap()
                .as_u64();
            let coin_pnl_amount = diff_coin_pnl_amount
                .checked_mul(amm.fees.pnl_numerator.into())
                .unwrap()
                .checked_div(amm.fees.pnl_denominator.into())
                .unwrap()
                .as_u64();
            if pc_pnl_amount != 0 && coin_pnl_amount != 0 {
                // step2: save total_pnl_pc & total_pnl_coin
                amm.state_data.total_pnl_pc = amm
                    .state_data
                    .total_pnl_pc
                    .checked_add(diff_pc_pnl_amount.as_u64())
                    .unwrap();
                amm.state_data.total_pnl_coin = amm
                    .state_data
                    .total_pnl_coin
                    .checked_add(diff_coin_pnl_amount.as_u64())
                    .unwrap();
                amm.state_data.need_take_pnl_pc = amm
                    .state_data
                    .need_take_pnl_pc
                    .checked_add(pc_pnl_amount)
                    .unwrap();
                amm.state_data.need_take_pnl_coin = amm
                    .state_data
                    .need_take_pnl_coin
                    .checked_add(coin_pnl_amount)
                    .unwrap();

                // step3: update total_coin and total_pc without pnl
                *total_pc_without_take_pnl = (*total_pc_without_take_pnl)
                    .checked_sub(pc_pnl_amount)
                    .unwrap();
                *total_coin_without_take_pnl = (*total_coin_without_take_pnl)
                    .checked_sub(coin_pnl_amount)
                    .unwrap();
            } else {
                delta_x = 0;
                delta_y = 0;
            }
        } else {
            msg!(
                arrform!(
                    LOG_SIZE,
                    "calc_take_pnl error x:{}, y:{}, calc_pnl_x:{}, calc_pnl_y:{}",
                    x1,
                    y1,
                    identity(target.calc_pnl_x),
                    identity(target.calc_pnl_y)
                )
                .as_str()
            );
            return Err(AmmError::CalcPnlError.into());
        }

        Ok((delta_x, delta_y))
    }

    /// Calculates the authority id by generating a program address.
    pub fn authority_id(
        program_id: &Pubkey,
        amm_seed: &[u8],
        nonce: u8,
    ) -> Result<Pubkey, AmmError> {
        Pubkey::create_program_address(&[amm_seed, &[nonce]], program_id)
            .map_err(|_| AmmError::InvalidProgramAddress.into())
    }

    #[allow(clippy::too_many_arguments)]
    fn check_accounts(
        program_id: &Pubkey,
        amm: &AmmInfo,
        amm_info: &AccountInfo,
        token_program_info: &AccountInfo,
        clock_info: &AccountInfo,
        market_program_info: &AccountInfo,
        amm_authority_info: &AccountInfo,
        market_info: &AccountInfo,
        amm_open_orders_info: &AccountInfo,
        amm_coin_vault_info: &AccountInfo,
        amm_pc_vault_info: &AccountInfo,
        amm_target_orders_info: &AccountInfo,
        srm_token_info: Option<&AccountInfo>,
        referrer_pc_info: Option<&AccountInfo>,
    ) -> ProgramResult {
        if amm.status == AmmStatus::Uninitialized.into_u64() {
            return Err(AmmError::InvalidStatus.into());
        }
        if *amm_authority_info.key
            != Self::authority_id(program_id, AUTHORITY_AMM, amm.nonce as u8)?
        {
            return Err(AmmError::InvalidProgramAddress.into());
        }
        check_assert_eq!(
            *amm_info.owner,
            *program_id,
            "amm_owner",
            AmmError::InvalidOwner
        );
        check_assert_eq!(
            *token_program_info.key,
            spl_token::id(),
            "spl_token_program",
            AmmError::InvalidSplTokenProgram
        );
        let token_program_id = token_program_info.key;
        check_assert_eq!(
            *clock_info.key,
            clock::id(),
            "clock",
            AmmError::InvalidProgramAddress
        );
        check_assert_eq!(
            *market_program_info.key,
            amm.market_program,
            "market_program",
            AmmError::InvalidMarketProgram
        );
        check_assert_eq!(
            *market_info.key,
            amm.market,
            "market_info",
            AmmError::InvalidMarket
        );
        check_assert_eq!(
            *amm_open_orders_info.key,
            amm.open_orders,
            "open_order",
            AmmError::InvalidOpenOrders
        );
        check_assert_eq!(
            *amm_coin_vault_info.key,
            amm.coin_vault,
            "coin_vault",
            AmmError::InvalidCoinVault
        );
        check_assert_eq!(
            *amm_pc_vault_info.key,
            amm.pc_vault,
            "pc_vault",
            AmmError::InvalidPCVault
        );
        check_assert_eq!(
            *amm_target_orders_info.key,
            amm.target_orders,
            "target_orders",
            AmmError::InvalidTargetOrders
        );
        if let Some(srm_token_account) = srm_token_info {
            let srm_token = Self::unpack_token_account(&srm_token_account, token_program_id)?;
            check_assert_eq!(
                srm_token.owner,
                *amm_authority_info.key,
                "srm_token_owner",
                AmmError::InvalidOwner
            );
            if srm_token.mint != srm_token::id() && srm_token.mint != msrm_token::id() {
                return Err(AmmError::InvalidSrmMint.into());
            }
        }
        if let Some(referrer_pc_account) = referrer_pc_info {
            let referrer_pc_token =
                Self::unpack_token_account(&referrer_pc_account, token_program_id)?;
            check_assert_eq!(
                referrer_pc_token.owner,
                config_feature::referrer_pc_wallet::id(),
                "referrer_pc_owner",
                AmmError::InvalidOwner
            );
            check_assert_eq!(
                referrer_pc_token.mint,
                amm.pc_vault_mint,
                "referrer_pc",
                AmmError::InvalidReferPCMint
            );
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn generate_amm_associated_spl_token<'a, 'b: 'a>(
        program_id: &Pubkey,
        spl_token_program_id: &Pubkey,
        market_account: &'a AccountInfo<'b>,
        associated_token_account: &'a AccountInfo<'b>,
        token_mint_account: &'a AccountInfo<'b>,
        user_wallet_account: &'a AccountInfo<'b>,
        system_program_account: &'a AccountInfo<'b>,
        rent_sysvar_account: &'a AccountInfo<'b>,
        spl_token_program_account: &'a AccountInfo<'b>,
        associated_owner_account: &'a AccountInfo<'b>,
        associated_seed: &[u8],
    ) -> ProgramResult {
        let (associated_token_address, bump_seed) = get_associated_address_and_bump_seed(
            program_id,
            &market_account.key,
            associated_seed,
            program_id,
        );
        if associated_token_address != *associated_token_account.key {
            msg!("Error: Associated token address does not match seed derivation");
            return Err(AmmError::ExpectedAccount.into());
        }
        if associated_token_account.owner == system_program_account.key {
            let associated_account_signer_seeds: &[&[_]] = &[
                &program_id.to_bytes(),
                &market_account.key.to_bytes(),
                associated_seed,
                &[bump_seed],
            ];
            let rent = &Rent::from_account_info(rent_sysvar_account)?;
            let required_lamports = rent
                .minimum_balance(spl_token::state::Account::LEN)
                .max(1)
                .saturating_sub(associated_token_account.lamports());
            if required_lamports > 0 {
                invoke(
                    &system_instruction::transfer(
                        user_wallet_account.key,
                        associated_token_account.key,
                        required_lamports,
                    ),
                    &[
                        user_wallet_account.clone(),
                        associated_token_account.clone(),
                        system_program_account.clone(),
                    ],
                )?;
            }
            invoke_signed(
                &system_instruction::allocate(
                    associated_token_account.key,
                    spl_token::state::Account::LEN as u64,
                ),
                &[
                    associated_token_account.clone(),
                    system_program_account.clone(),
                ],
                &[&associated_account_signer_seeds],
            )?;
            invoke_signed(
                &system_instruction::assign(associated_token_account.key, spl_token_program_id),
                &[
                    associated_token_account.clone(),
                    system_program_account.clone(),
                ],
                &[&associated_account_signer_seeds],
            )?;

            invoke(
                &spl_token::instruction::initialize_account(
                    spl_token_program_id,
                    associated_token_account.key,
                    token_mint_account.key,
                    associated_owner_account.key,
                )?,
                &[
                    associated_token_account.clone(),
                    token_mint_account.clone(),
                    associated_owner_account.clone(),
                    rent_sysvar_account.clone(),
                    spl_token_program_account.clone(),
                ],
            )?;
        } else {
            associated_token_address.log();
            return Err(AmmError::RepeatCreateAmm.into());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn generate_amm_associated_spl_mint<'a, 'b: 'a>(
        program_id: &Pubkey,
        spl_token_program_id: &Pubkey,
        market_account: &'a AccountInfo<'b>,
        associated_token_account: &'a AccountInfo<'b>,
        user_wallet_account: &'a AccountInfo<'b>,
        system_program_account: &'a AccountInfo<'b>,
        rent_sysvar_account: &'a AccountInfo<'b>,
        spl_token_program_account: &'a AccountInfo<'b>,
        associated_owner_account: &'a AccountInfo<'b>,
        associated_seed: &[u8],
        mint_decimals: u8,
    ) -> ProgramResult {
        let (associated_token_address, bump_seed) = get_associated_address_and_bump_seed(
            program_id,
            &market_account.key,
            associated_seed,
            program_id,
        );
        if associated_token_address != *associated_token_account.key {
            msg!("Error: Associated mint address does not match seed derivation");
            return Err(AmmError::ExpectedMint.into());
        }
        if associated_token_account.owner == system_program_account.key {
            let associated_account_signer_seeds: &[&[_]] = &[
                &program_id.to_bytes(),
                &market_account.key.to_bytes(),
                associated_seed,
                &[bump_seed],
            ];
            let rent = &Rent::from_account_info(rent_sysvar_account)?;
            let required_lamports = rent
                .minimum_balance(spl_token::state::Mint::LEN)
                .max(1)
                .saturating_sub(associated_token_account.lamports());
            if required_lamports > 0 {
                invoke(
                    &system_instruction::transfer(
                        user_wallet_account.key,
                        associated_token_account.key,
                        required_lamports,
                    ),
                    &[
                        user_wallet_account.clone(),
                        associated_token_account.clone(),
                        system_program_account.clone(),
                    ],
                )?;
            }
            invoke_signed(
                &system_instruction::allocate(
                    associated_token_account.key,
                    spl_token::state::Mint::LEN as u64,
                ),
                &[
                    associated_token_account.clone(),
                    system_program_account.clone(),
                ],
                &[&associated_account_signer_seeds],
            )?;
            invoke_signed(
                &system_instruction::assign(associated_token_account.key, spl_token_program_id),
                &[
                    associated_token_account.clone(),
                    system_program_account.clone(),
                ],
                &[&associated_account_signer_seeds],
            )?;

            invoke(
                &spl_token::instruction::initialize_mint(
                    spl_token_program_id,
                    associated_token_account.key,
                    associated_owner_account.key,
                    None,
                    mint_decimals,
                )?,
                &[
                    associated_token_account.clone(),
                    associated_owner_account.clone(),
                    rent_sysvar_account.clone(),
                    spl_token_program_account.clone(),
                ],
            )?;
        } else {
            associated_token_address.log();
            return Err(AmmError::RepeatCreateAmm.into());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn generate_amm_associated_account<'a, 'b: 'a>(
        program_id: &Pubkey,
        assign_to: &Pubkey,
        market_account: &'a AccountInfo<'b>,
        associated_token_account: &'a AccountInfo<'b>,
        user_wallet_account: &'a AccountInfo<'b>,
        system_program_account: &'a AccountInfo<'b>,
        rent_sysvar_account: &'a AccountInfo<'b>,
        associated_seed: &[u8],
        data_size: usize,
    ) -> ProgramResult {
        let (associated_token_address, bump_seed) = get_associated_address_and_bump_seed(
            &program_id,
            &market_account.key,
            associated_seed,
            program_id,
        );
        if associated_token_address != *associated_token_account.key {
            msg!("Error: Associated token address does not match seed derivation");
            return Err(AmmError::ExpectedAccount.into());
        }
        if associated_token_account.owner == system_program_account.key {
            let associated_account_signer_seeds: &[&[_]] = &[
                &program_id.to_bytes(),
                &market_account.key.to_bytes(),
                associated_seed,
                &[bump_seed],
            ];
            let rent = &Rent::from_account_info(rent_sysvar_account)?;
            let required_lamports = rent
                .minimum_balance(data_size)
                .max(1)
                .saturating_sub(associated_token_account.lamports());
            if required_lamports > 0 {
                invoke(
                    &system_instruction::transfer(
                        user_wallet_account.key,
                        associated_token_account.key,
                        required_lamports,
                    ),
                    &[
                        user_wallet_account.clone(),
                        associated_token_account.clone(),
                        system_program_account.clone(),
                    ],
                )?;
            }
            invoke_signed(
                &system_instruction::allocate(associated_token_account.key, data_size as u64),
                &[
                    associated_token_account.clone(),
                    system_program_account.clone(),
                ],
                &[&associated_account_signer_seeds],
            )?;
            invoke_signed(
                &system_instruction::assign(associated_token_account.key, assign_to),
                &[
                    associated_token_account.clone(),
                    system_program_account.clone(),
                ],
                &[&associated_account_signer_seeds],
            )?;
        } else {
            associated_token_address.log();
            return Err(AmmError::RepeatCreateAmm.into());
        }
        Ok(())
    }

    ///// Processes an [Initialize](enum.Instruction.html).
    //pub fn process_initialize2(
    //    program_id: &Pubkey,
    //    accounts: &[AccountInfo],
    //    init: InitializeInstruction2,
    //) -> ProgramResult {
    //    let account_info_iter = &mut accounts.iter();
    //    let token_program_info = next_account_info(account_info_iter)?;
    //    let ata_token_program_info = next_account_info(account_info_iter)?;
    //    let system_program_info = next_account_info(account_info_iter)?;
    //    let rent_sysvar_info = next_account_info(account_info_iter)?;
    //    let amm_info = next_account_info(account_info_iter)?;
    //    let amm_authority_info = next_account_info(account_info_iter)?;
    //    let amm_open_orders_info = next_account_info(account_info_iter)?;
    //    let amm_lp_mint_info = next_account_info(account_info_iter)?;
    //    let amm_coin_mint_info = next_account_info(account_info_iter)?;
    //    let amm_pc_mint_info = next_account_info(account_info_iter)?;
    //    let amm_coin_vault_info = next_account_info(account_info_iter)?;
    //    let amm_pc_vault_info = next_account_info(account_info_iter)?;
    //    let amm_target_orders_info = next_account_info(account_info_iter)?;
    //    let amm_config_info = next_account_info(account_info_iter)?;
    //    let create_fee_destination_info = next_account_info(account_info_iter)?;

    //    let market_program_info = next_account_info(account_info_iter)?;
    //    let market_info = next_account_info(account_info_iter)?;

    //    let user_wallet_info = next_account_info(account_info_iter)?;
    //    let user_token_coin_info = next_account_info(account_info_iter)?;
    //    let user_token_pc_info = next_account_info(account_info_iter)?;
    //    let user_token_lp_info = next_account_info(account_info_iter)?;

    //    let (pda, _) = Pubkey::find_program_address(&[&AMM_CONFIG_SEED], program_id);
    //    if pda != *amm_config_info.key || amm_config_info.owner != program_id {
    //        return Err(AmmError::InvalidConfigAccount.into());
    //    }

    //    msg!(arrform!(LOG_SIZE, "initialize2: {:?}", init).as_str());
    //    if !user_wallet_info.is_signer {
    //        return Err(AmmError::InvalidSignAccount.into());
    //    }
    //    check_assert_eq!(
    //        *token_program_info.key,
    //        spl_token::id(),
    //        "spl_token_program",
    //        AmmError::InvalidSplTokenProgram
    //    );
    //    let spl_token_program_id = token_program_info.key;
    //    check_assert_eq!(
    //        *ata_token_program_info.key,
    //        spl_associated_token_account::id(),
    //        "spl_associated_token_account",
    //        AmmError::InvalidSplTokenProgram
    //    );
    //    check_assert_eq!(
    //        *market_program_info.key,
    //        config_feature::openbook_program::id(),
    //        "market_program",
    //        AmmError::InvalidMarketProgram
    //    );
    //    check_assert_eq!(
    //        *system_program_info.key,
    //        solana_program::system_program::id(),
    //        "sys_program",
    //        AmmError::InvalidSysProgramAddress
    //    );
    //    let (expect_amm_authority, expect_nonce) =
    //        Pubkey::find_program_address(&[&AUTHORITY_AMM], program_id);
    //    if *amm_authority_info.key != expect_amm_authority || init.nonce != expect_nonce {
    //        return Err(AmmError::InvalidProgramAddress.into());
    //    }
    //    if *create_fee_destination_info.key != config_feature::create_pool_fee_address::id() {
    //        return Err(AmmError::InvalidFee.into());
    //    }
    //    let amm_config = AmmConfig::load_checked(&amm_config_info, program_id)?;
    //    // Charge the fee to create a pool
    //    if amm_config.create_pool_fee != 0 {
    //        invoke(
    //            &system_instruction::transfer(
    //                user_wallet_info.key,
    //                create_fee_destination_info.key,
    //                amm_config.create_pool_fee,
    //            ),
    //            &[
    //                user_wallet_info.clone(),
    //                create_fee_destination_info.clone(),
    //                system_program_info.clone(),
    //            ],
    //        )?;
    //        invoke(
    //            &spl_token::instruction::sync_native(
    //                token_program_info.key,
    //                create_fee_destination_info.key,
    //            )?,
    //            &[
    //                token_program_info.clone(),
    //                create_fee_destination_info.clone(),
    //            ],
    //        )?;
    //    }

    //    // unpack and check coin_mint
    //    let coin_mint = Self::unpack_mint(&amm_coin_mint_info, spl_token_program_id)?;
    //    // unpack and check pc_mint
    //    let pc_mint = Self::unpack_mint(&amm_pc_mint_info, spl_token_program_id)?;

    //    // create target_order account
    //    Self::generate_amm_associated_account(
    //        program_id,
    //        program_id,
    //        market_info,
    //        amm_target_orders_info,
    //        user_wallet_info,
    //        system_program_info,
    //        rent_sysvar_info,
    //        TARGET_ASSOCIATED_SEED,
    //        size_of::<TargetOrders>(),
    //    )?;

    //    // create lp mint account
    //    let lp_decimals = coin_mint.decimals;
    //    Self::generate_amm_associated_spl_mint(
    //        program_id,
    //        spl_token_program_id,
    //        market_info,
    //        amm_lp_mint_info,
    //        user_wallet_info,
    //        system_program_info,
    //        rent_sysvar_info,
    //        token_program_info,
    //        amm_authority_info,
    //        LP_MINT_ASSOCIATED_SEED,
    //        lp_decimals,
    //    )?;
    //    // create coin vault account
    //    Self::generate_amm_associated_spl_token(
    //        program_id,
    //        spl_token_program_id,
    //        market_info,
    //        amm_coin_vault_info,
    //        amm_coin_mint_info,
    //        user_wallet_info,
    //        system_program_info,
    //        rent_sysvar_info,
    //        token_program_info,
    //        amm_authority_info,
    //        COIN_VAULT_ASSOCIATED_SEED,
    //    )?;
    //    // create pc vault account
    //    Self::generate_amm_associated_spl_token(
    //        program_id,
    //        spl_token_program_id,
    //        market_info,
    //        amm_pc_vault_info,
    //        amm_pc_mint_info,
    //        user_wallet_info,
    //        system_program_info,
    //        rent_sysvar_info,
    //        token_program_info,
    //        amm_authority_info,
    //        PC_VAULT_ASSOCIATED_SEED,
    //    )?;
    //    // create amm account
    //    Self::generate_amm_associated_account(
    //        program_id,
    //        program_id,
    //        market_info,
    //        amm_info,
    //        user_wallet_info,
    //        system_program_info,
    //        rent_sysvar_info,
    //        AMM_ASSOCIATED_SEED,
    //        size_of::<AmmInfo>(),
    //    )?;

    //    // create amm open order account
    //    Self::generate_amm_associated_account(
    //        program_id,
    //        market_program_info.key,
    //        market_info,
    //        amm_open_orders_info,
    //        user_wallet_info,
    //        system_program_info,
    //        rent_sysvar_info,
    //        OPEN_ORDER_ASSOCIATED_SEED,
    //        size_of::<serum_dex::state::OpenOrders>() + 12,
    //    )?;
    //    // init open orders account
    //    Invokers::invoke_dex_init_open_orders(
    //        market_program_info.clone(),
    //        amm_open_orders_info.clone(),
    //        amm_authority_info.clone(),
    //        market_info.clone(),
    //        rent_sysvar_info.clone(),
    //        AUTHORITY_AMM,
    //        init.nonce as u8,
    //    )?;

    //    // create user ata lp token
    //    Invokers::create_ata_spl_token(
    //        user_token_lp_info.clone(),
    //        user_wallet_info.clone(),
    //        user_wallet_info.clone(),
    //        amm_lp_mint_info.clone(),
    //        token_program_info.clone(),
    //        ata_token_program_info.clone(),
    //        system_program_info.clone(),
    //    )?;

    //    // transfer user tokens to vault
    //    Invokers::token_transfer(
    //        token_program_info.clone(),
    //        user_token_coin_info.clone(),
    //        amm_coin_vault_info.clone(),
    //        user_wallet_info.clone(),
    //        init.init_coin_amount,
    //    )?;
    //    Invokers::token_transfer(
    //        token_program_info.clone(),
    //        user_token_pc_info.clone(),
    //        amm_pc_vault_info.clone(),
    //        user_wallet_info.clone(),
    //        init.init_pc_amount,
    //    )?;

    //    // load AmmInfo
    //    let mut amm = AmmInfo::load_mut(&amm_info)?;
    //    if amm.status != AmmStatus::Uninitialized.into_u64() {
    //        return Err(AmmError::AlreadyInUse.into());
    //    }

    //    // unpack and check token_coin
    //    let amm_coin_vault =
    //        Self::unpack_token_account(&amm_coin_vault_info, spl_token_program_id)?;
    //    check_assert_eq!(
    //        amm_coin_vault.owner,
    //        *amm_authority_info.key,
    //        "coin_vault_owner",
    //        AmmError::InvalidOwner
    //    );
    //    if amm_coin_vault.amount == 0 {
    //        return Err(AmmError::InvalidSupply.into());
    //    }
    //    if amm_coin_vault.delegate.is_some() {
    //        return Err(AmmError::InvalidDelegate.into());
    //    }
    //    if amm_coin_vault.close_authority.is_some() {
    //        return Err(AmmError::InvalidCloseAuthority.into());
    //    }
    //    check_assert_eq!(
    //        *amm_coin_mint_info.key,
    //        amm_coin_vault.mint,
    //        "coin_mint",
    //        AmmError::InvalidCoinMint
    //    );
    //    // unpack and check token_pc
    //    let amm_pc_vault = Self::unpack_token_account(&amm_pc_vault_info, spl_token_program_id)?;
    //    check_assert_eq!(
    //        amm_pc_vault.owner,
    //        *amm_authority_info.key,
    //        "pc_vault_owner",
    //        AmmError::InvalidOwner
    //    );
    //    if amm_pc_vault.amount == 0 {
    //        return Err(AmmError::InvalidSupply.into());
    //    }
    //    if amm_pc_vault.delegate.is_some() {
    //        return Err(AmmError::InvalidDelegate.into());
    //    }
    //    if amm_pc_vault.close_authority.is_some() {
    //        return Err(AmmError::InvalidCloseAuthority.into());
    //    }
    //    check_assert_eq!(
    //        *amm_pc_mint_info.key,
    //        amm_pc_vault.mint,
    //        "pc_mint",
    //        AmmError::InvalidPCMint
    //    );

    //    // load and check market
    //    let market_state =
    //        Market::load_checked(market_info, &config_feature::openbook_program::id(), false)?;
    //    if identity(market_state.coin_mint) != amm_coin_vault.mint.to_aligned_bytes()
    //        || identity(market_state.coin_mint) != (*amm_coin_mint_info.key).to_aligned_bytes()
    //    {
    //        return Err(AmmError::InvalidCoinMint.into());
    //    }
    //    if identity(market_state.pc_mint) != amm_pc_vault.mint.to_aligned_bytes()
    //        || identity(market_state.pc_mint) != (*amm_pc_mint_info.key).to_aligned_bytes()
    //    {
    //        return Err(AmmError::InvalidPCMint.into());
    //    }
    //    if market_state.pc_lot_size == 0 || market_state.coin_lot_size == 0 {
    //        msg!(
    //            "pc_lot_size:{}, coin_lot_size:{}",
    //            identity(market_state.pc_lot_size),
    //            identity(market_state.coin_lot_size)
    //        );
    //        return Err(AmmError::InvalidMarket.into());
    //    }

    //    let lp_mint = Self::unpack_mint(&amm_lp_mint_info, spl_token_program_id)?;
    //    if lp_mint.supply != 0 {
    //        return Err(AmmError::InvalidSupply.into());
    //    }
    //    if COption::Some(*amm_authority_info.key) != lp_mint.mint_authority {
    //        return Err(AmmError::InvalidOwner.into());
    //    }
    //    if lp_mint.freeze_authority.is_some() {
    //        return Err(AmmError::InvalidFreezeAuthority.into());
    //    }

    //    let liquidity = Calculator::to_u64(
    //        U128::from(amm_pc_vault.amount)
    //            .checked_mul(amm_coin_vault.amount.into())
    //            .unwrap()
    //            .integer_sqrt()
    //            .as_u128(),
    //    )?;
    //    let user_lp_amount = liquidity
    //        .checked_sub((10u64).checked_pow(lp_mint.decimals.into()).unwrap())
    //        .ok_or(AmmError::InitLpAmountTooLess)?;

    //    // liquidity is measured in terms of token_a's value since both sides of
    //    // the pool are equal
    //    Invokers::token_mint_to(
    //        token_program_info.clone(),
    //        amm_lp_mint_info.clone(),
    //        user_token_lp_info.clone(),
    //        amm_authority_info.clone(),
    //        AUTHORITY_AMM,
    //        init.nonce,
    //        user_lp_amount,
    //    )?;

    //    amm.initialize(
    //        init.nonce,
    //        init.open_time,
    //        coin_mint.decimals,
    //        pc_mint.decimals,
    //        market_state.coin_lot_size,
    //        market_state.pc_lot_size,
    //    )?;
    //    encode_ray_log(InitLog {
    //        log_type: LogType::Init.into_u8(),
    //        time: init.open_time,
    //        pc_decimals: amm.pc_decimals as u8,
    //        coin_decimals: amm.coin_decimals as u8,
    //        pc_lot_size: market_state.pc_lot_size,
    //        coin_lot_size: market_state.coin_lot_size,
    //        pc_amount: amm_pc_vault.amount,
    //        coin_amount: amm_coin_vault.amount,
    //        market: *market_info.key,
    //    });
    //    let x = Calculator::normalize_decimal_v2(
    //        amm_pc_vault.amount,
    //        amm.pc_decimals,
    //        amm.sys_decimal_value,
    //    );
    //    let y = Calculator::normalize_decimal_v2(
    //        amm_coin_vault.amount,
    //        amm.coin_decimals,
    //        amm.sys_decimal_value,
    //    );
    //    // check and init target orders account
    //    if amm_target_orders_info.owner != program_id {
    //        return Err(AmmError::InvalidProgramAddress.into());
    //    }
    //    let mut target_order = TargetOrders::load_mut(amm_target_orders_info)?;
    //    target_order.check_init(x.as_u128(), y.as_u128(), amm_info.key)?;

    //    amm.coin_vault = *amm_coin_vault_info.key;
    //    amm.pc_vault = *amm_pc_vault_info.key;
    //    amm.coin_vault_mint = *amm_coin_mint_info.key;
    //    amm.pc_vault_mint = *amm_pc_mint_info.key;
    //    amm.lp_mint = *amm_lp_mint_info.key;
    //    amm.open_orders = *amm_open_orders_info.key;
    //    amm.market = *market_info.key;
    //    amm.market_program = *market_program_info.key;
    //    amm.target_orders = *amm_target_orders_info.key;
    //    amm.amm_owner = config_feature::amm_owner::ID;
    //    amm.lp_amount = liquidity;
    //    amm.status = if init.open_time > (Clock::get()?.unix_timestamp as u64) {
    //        AmmStatus::WaitingTrade.into_u64()
    //    } else {
    //        AmmStatus::SwapOnly.into_u64()
    //    };
    //    amm.reset_flag = AmmResetFlag::ResetYes.into_u64();

    //    Ok(())
    //}

    //   pub fn do_place_orders(args: account_parser::PlaceOrdersArgs) -> ProgramResult {
    //       let account_parser::PlaceOrdersArgs {
    //           program_id: _,
    //           limit,

    //           amm_authority_info,
    //           amm_open_orders_info,
    //           market_program_info,
    //           market_info,
    //           market_request_queue_info,
    //           amm_coin_vault_info,
    //           amm_pc_vault_info,
    //           market_coin_vault_info,
    //           market_pc_vault_info,
    //           token_program_info,
    //           rent_info,

    //           market_event_queue_info,
    //           market_bids_info,
    //           market_asks_info,

    //           srm_token_account,

    //           amm,
    //           open_orders,
    //           bids,
    //           asks,
    //           target,

    //           total_coin_without_take_pnl,
    //           total_pc_without_take_pnl,

    //           coin_vault_amount,
    //           pc_vault_amount,
    //       } = args;

    //       if open_orders.free_slot_bits.count_zeros() > 100 {
    //           // TooManyOpenOrders
    //           amm.state = AmmState::CancelAllOrdersState.into_u64();
    //           return Ok(());
    //       }

    //       let x = Calculator::normalize_decimal_v2(
    //           total_pc_without_take_pnl,
    //           amm.pc_decimals,
    //           amm.sys_decimal_value,
    //       );

    //       let y = Calculator::normalize_decimal_v2(
    //           total_coin_without_take_pnl,
    //           amm.coin_decimals,
    //           amm.sys_decimal_value,
    //       );

    //       if x.is_zero() || y.is_zero() {
    //           return Err(AmmError::CheckedEmptyFunds.into());
    //       }

    //       if x != target.target_x.into() || y != target.target_y.into() {
    //           amm.state = AmmState::IdleState.into_u64();
    //           return Ok(());
    //       }

    //       let place_orders_num;
    //       let place_orders_cur = target.place_orders_cur;
    //       let target_orders_buy = target.buy_orders;
    //       let target_orders_sell = target.sell_orders;
    //       if place_orders_cur + limit as u64 > amm.order_num {
    //           place_orders_num = amm.order_num
    //       } else {
    //           place_orders_num = place_orders_cur + limit as u64;
    //       }

    //       // msg!(arrform!(LOG_SIZE, "do_place num:{}, cur:{}, limit:{}, buy_len:{}, sell_len:{}", place_orders_num, place_orders_cur, limit, bids.len(), asks.len()).as_str());
    //       let mut bids_client_ids: VecDeque<u64> = bids
    //           .into_iter()
    //           .map(|item| item.client_order_id())
    //           .collect();

    //       let mut asks_client_ids: VecDeque<u64> = asks
    //           .into_iter()
    //           .map(|item| item.client_order_id())
    //           .collect();

    //       bids_client_ids.retain(|x| !identity(target.replace_buy_client_id).contains(&x));
    //       asks_client_ids.retain(|x| !identity(target.replace_sell_client_id).contains(&x));

    //       let mut pc_avaliable = pc_vault_amount
    //           .checked_add(open_orders.native_pc_free)
    //           .unwrap()
    //           .checked_sub(amm.state_data.need_take_pnl_pc)
    //           .unwrap();

    //       let mut coin_avaliable = coin_vault_amount
    //           .checked_add(open_orders.native_coin_free)
    //           .unwrap()
    //           .checked_sub(amm.state_data.need_take_pnl_coin)
    //           .unwrap();

    //       for i in place_orders_cur..place_orders_num {
    //           let payer = amm_pc_vault_info.clone();
    //           let side = Side::Bid;
    //           let limit_price_u64 =
    //               Calculator::convert_price_out(target_orders_buy[i as usize].price, amm.pc_lot_size);
    //           let max_coin_qty = Calculator::convert_vol_out(
    //               target_orders_buy[i as usize].vol,
    //               amm.coin_decimals,
    //               amm.coin_lot_size,
    //               amm.sys_decimal_value,
    //           );
    //           let order_type = OrderType::Limit;

    //           let out_pc_lot_size = Calculator::convert_out_pc_lot_size(
    //               amm.pc_decimals as u8,
    //               amm.coin_decimals as u8,
    //               amm.pc_lot_size,
    //               amm.coin_lot_size,
    //               amm.sys_decimal_value,
    //           );

    //           let max_native_pc_qty_including_fees = max_coin_qty
    //               .checked_mul(limit_price_u64)
    //               .unwrap()
    //               .checked_mul(out_pc_lot_size)
    //               .unwrap();

    //           if max_native_pc_qty_including_fees > pc_avaliable {
    //               // pc amount is InsufficientFunds
    //               amm.state = AmmState::CancelAllOrdersState.into_u64();
    //               return Ok(());
    //           }
    //           if limit_price_u64 != 0 && max_coin_qty != 0 {
    //               if bids_client_ids.is_empty() {
    //                   let client_order_id = amm.incr_client_order_id();
    //                   // msg!("new buy order:{}, {}, {}", client_order_id, limit_price_u64, max_coin_qty);
    //                   target.replace_buy_client_id[target.valid_buy_order_num as usize] =
    //                       client_order_id;
    //                   Invokers::invoke_dex_new_order_v3(
    //                       market_program_info.clone(),
    //                       market_info.clone(),
    //                       amm_open_orders_info.clone(),
    //                       market_request_queue_info.clone(),
    //                       market_event_queue_info.clone(),
    //                       market_bids_info.clone(),
    //                       market_asks_info.clone(),
    //                       payer,
    //                       amm_authority_info.clone(),
    //                       market_coin_vault_info.clone(),
    //                       market_pc_vault_info.clone(),
    //                       token_program_info.clone(),
    //                       rent_info.clone(),
    //                       srm_token_account.clone(),
    //                       AUTHORITY_AMM,
    //                       amm.nonce as u8,
    //                       side,
    //                       NonZeroU64::new(limit_price_u64).unwrap(),
    //                       NonZeroU64::new(max_coin_qty).unwrap(),
    //                       NonZeroU64::new(max_native_pc_qty_including_fees).unwrap(),
    //                       order_type,
    //                       client_order_id,
    //                       std::u16::MAX,
    //                   )?;
    //               } else {
    //                   let client_order_id = if target.valid_buy_order_num == 0 {
    //                       bids_client_ids.pop_back().unwrap()
    //                   } else {
    //                       bids_client_ids.pop_front().unwrap()
    //                   };
    //                   target.replace_buy_client_id[target.valid_buy_order_num as usize] =
    //                       client_order_id;
    //                   // msg!("replace buy order:{}, {}, {}", client_order_id, limit_price_u64, max_coin_qty);
    //                   Invokers::invoke_dex_replace_order_by_client_id(
    //                       market_program_info.clone(),
    //                       market_info.clone(),
    //                       amm_open_orders_info.clone(),
    //                       market_request_queue_info.clone(),
    //                       market_event_queue_info.clone(),
    //                       market_bids_info.clone(),
    //                       market_asks_info.clone(),
    //                       payer,
    //                       amm_authority_info.clone(),
    //                       market_coin_vault_info.clone(),
    //                       market_pc_vault_info.clone(),
    //                       token_program_info.clone(),
    //                       rent_info.clone(),
    //                       srm_token_account.clone(),
    //                       AUTHORITY_AMM,
    //                       amm.nonce as u8,
    //                       side,
    //                       NonZeroU64::new(limit_price_u64).unwrap(),
    //                       NonZeroU64::new(max_coin_qty).unwrap(),
    //                       NonZeroU64::new(max_native_pc_qty_including_fees).unwrap(),
    //                       order_type,
    //                       client_order_id,
    //                       std::u16::MAX,
    //                   )?;
    //               }
    //               target.valid_buy_order_num += 1;
    //           }

    //           let payer = amm_coin_vault_info.clone();

    //           let side = Side::Ask;

    //           let limit_price_u64 = Calculator::convert_price_out(
    //               target_orders_sell[i as usize].price,
    //               amm.pc_lot_size,
    //           );

    //           let max_coin_qty = Calculator::convert_vol_out(
    //               target_orders_sell[i as usize].vol,
    //               amm.coin_decimals,
    //               amm.coin_lot_size,
    //               amm.sys_decimal_value,
    //           );

    //           let order_type = OrderType::Limit;

    //           if max_coin_qty.checked_mul(amm.coin_lot_size).unwrap() > coin_avaliable {
    //               // coin amount is InsufficientFunds
    //               amm.state = AmmState::CancelAllOrdersState.into_u64();
    //               return Ok(());
    //           }

    //           let max_native_pc_qty_including_fees = 1u64;

    //           if limit_price_u64 != 0 && max_coin_qty != 0 {
    //               if asks_client_ids.is_empty() {
    //                   let client_order_id = amm.incr_client_order_id();
    //                   target.replace_sell_client_id[target.valid_sell_order_num as usize] =
    //                       client_order_id;
    //                   // msg!("new sell order:{}, {}, {}", client_order_id, limit_price_u64, max_coin_qty);
    //                   Invokers::invoke_dex_new_order_v3(
    //                       market_program_info.clone(),
    //                       market_info.clone(),
    //                       amm_open_orders_info.clone(),
    //                       market_request_queue_info.clone(),
    //                       market_event_queue_info.clone(),
    //                       market_bids_info.clone(),
    //                       market_asks_info.clone(),
    //                       payer,
    //                       amm_authority_info.clone(),
    //                       market_coin_vault_info.clone(),
    //                       market_pc_vault_info.clone(),
    //                       token_program_info.clone(),
    //                       rent_info.clone(),
    //                       srm_token_account.clone(),
    //                       AUTHORITY_AMM,
    //                       amm.nonce as u8,
    //                       side,
    //                       NonZeroU64::new(limit_price_u64).unwrap(),
    //                       NonZeroU64::new(max_coin_qty).unwrap(),
    //                       NonZeroU64::new(max_native_pc_qty_including_fees).unwrap(),
    //                       order_type,
    //                       client_order_id,
    //                       std::u16::MAX,
    //                   )?;
    //               } else {
    //                   let client_order_id = if target.valid_sell_order_num == 0 {
    //                       asks_client_ids.pop_back().unwrap()
    //                   } else {
    //                       asks_client_ids.pop_front().unwrap()
    //                   };
    //                   target.replace_sell_client_id[target.valid_sell_order_num as usize] =
    //                       client_order_id;
    //                   // msg!("replace sell order:{}, {}, {}", client_order_id, limit_price_u64, max_coin_qty);
    //                   Invokers::invoke_dex_replace_order_by_client_id(
    //                       market_program_info.clone(),
    //                       market_info.clone(),
    //                       amm_open_orders_info.clone(),
    //                       market_request_queue_info.clone(),
    //                       market_event_queue_info.clone(),
    //                       market_bids_info.clone(),
    //                       market_asks_info.clone(),
    //                       payer,
    //                       amm_authority_info.clone(),
    //                       market_coin_vault_info.clone(),
    //                       market_pc_vault_info.clone(),
    //                       token_program_info.clone(),
    //                       rent_info.clone(),
    //                       srm_token_account.clone(),
    //                       AUTHORITY_AMM,
    //                       amm.nonce as u8,
    //                       side,
    //                       NonZeroU64::new(limit_price_u64).unwrap(),
    //                       NonZeroU64::new(max_coin_qty).unwrap(),
    //                       NonZeroU64::new(max_native_pc_qty_including_fees).unwrap(),
    //                       order_type,
    //                       client_order_id,
    //                       std::u16::MAX,
    //                   )?;
    //               }
    //               target.valid_sell_order_num += 1;
    //           }

    //           // update place_orders_cur
    //           target.place_orders_cur += 1;
    //           {
    //               // reload account data to check avaliable pc and avaliable coin is enough to place order
    //               let open_orders = OpenOrders::load_checked(
    //                   amm_open_orders_info,
    //                   Some(market_info),
    //                   Some(amm_authority_info),
    //                   &amm.market_program,
    //               )?;
    //               let pc_vault = Self::unpack_token_account(&amm_pc_vault_info, &spl_token::id())?;
    //               let coin_vault = Self::unpack_token_account(&amm_pc_vault_info, &spl_token::id())?;
    //               pc_avaliable = pc_vault
    //                   .amount
    //                   .checked_add(open_orders.native_pc_free)
    //                   .unwrap()
    //                   .checked_sub(amm.state_data.need_take_pnl_pc)
    //                   .unwrap();
    //               coin_avaliable = coin_vault
    //                   .amount
    //                   .checked_add(open_orders.native_coin_free)
    //                   .unwrap()
    //                   .checked_sub(amm.state_data.need_take_pnl_coin)
    //                   .unwrap();
    //           }
    //       }
    //       if target.place_orders_cur == amm.order_num {
    //           // save placed_x & placed_y
    //           target.placed_x = x.as_u128();
    //           target.placed_y = y.as_u128();
    //       }
    //       if target.place_orders_cur >= amm.order_num {
    //           amm.state = AmmState::PurgeOrderState.into_u64();
    //           return Ok(());
    //       }
    //       Ok(())
    //   }

    //pub fn do_purge_orders(args: account_parser::PurgeOrderArgs) -> ProgramResult {
    //    let account_parser::PurgeOrderArgs {
    //        program_id: _,
    //        limit: _,
    //        market_program_info,
    //        market_info,
    //        amm_open_orders_info,
    //        amm_authority_info,
    //        market_event_queue_info,
    //        market_bids_info,
    //        market_asks_info,
    //        amm,
    //        target,
    //        bids,
    //        asks,
    //    } = args;

    //    if bids.len() <= target.valid_buy_order_num as usize
    //        && asks.len() <= target.valid_sell_order_num as usize
    //    {
    //        // no order to purge
    //        amm.state = AmmState::IdleState.into_u64();
    //    } else {
    //        let mut cancel_client_order_ids = Vec::new();
    //        for i in target.valid_buy_order_num as usize..bids.len() {
    //            cancel_client_order_ids.push(bids[i].client_order_id());
    //        }
    //        for i in target.valid_sell_order_num as usize..asks.len() {
    //            cancel_client_order_ids.push(asks[i].client_order_id());
    //        }

    //        let mut order_ids_vec = Vec::new();
    //        let mut order_ids = [0u64; 8];
    //        let mut count = 0;
    //        for i in 0..cancel_client_order_ids.len() {
    //            order_ids[count] = cancel_client_order_ids[i];
    //            count += 1;
    //            if count == 8 {
    //                order_ids_vec.push(order_ids);
    //                order_ids = [0u64; 8];
    //                count = 0;
    //            }
    //        }
    //        if count != 0 {
    //            order_ids_vec.push(order_ids);
    //        }
    //        for ids in order_ids_vec.iter() {
    //            Invokers::invoke_dex_cancel_orders_by_client_order_ids(
    //                market_program_info.clone(),
    //                market_info.clone(),
    //                market_bids_info.clone(),
    //                market_asks_info.clone(),
    //                amm_open_orders_info.clone(),
    //                amm_authority_info.clone(),
    //                market_event_queue_info.clone(),
    //                AUTHORITY_AMM,
    //                amm.nonce as u8,
    //                *ids,
    //            )?;
    //        }
    //    }
    //    msg!("do_purge to_state {}", identity(amm.state));
    //    Ok(())
    //}

    //pub fn do_cancel_all_orders_state(args: account_parser::CancelAllOrdersArgs) -> ProgramResult {
    //    let account_parser::CancelAllOrdersArgs {
    //        program_id: _,
    //        limit: _,
    //        market_program_info,
    //        market_info,
    //        amm_open_orders_info,
    //        amm_authority_info,
    //        market_event_queue_info,
    //        market_coin_vault_info,
    //        market_pc_vault_info,
    //        market_bids_info,
    //        market_asks_info,
    //        market_vault_signer,
    //        token_program_info,
    //        amm_coin_vault_info,
    //        amm_pc_vault_info,
    //        referrer_pc_account,
    //        amm,
    //        open_orders,
    //        target,
    //        bids,
    //        asks,
    //    } = args;

    //    msg!(
    //        arrform!(
    //            LOG_SIZE,
    //            "do_cancel_all o_bits:{:x}",
    //            identity(open_orders.free_slot_bits)
    //        )
    //        .as_str()
    //    );
    //    // to decide whether all orders are really canceled
    //    if open_orders.free_slot_bits.count_zeros() > 100 && bids.is_empty() && asks.is_empty() {
    //        msg!(
    //            "do_cancel_all to state :{}, count_zeros:{}",
    //            identity(amm.state),
    //            open_orders.free_slot_bits.count_zeros()
    //        );
    //        return Ok(());
    //    } else {
    //        let mut amm_order_ids_vec = Vec::new();
    //        let mut order_ids = [0u64; 8];
    //        let mut count = 0;
    //        for i in 0..std::cmp::max(bids.len(), asks.len()) {
    //            if i < bids.len() {
    //                order_ids[count] = bids[i].client_order_id();
    //                count += 1;
    //            }
    //            if i < asks.len() {
    //                order_ids[count] = asks[i].client_order_id();
    //                count += 1;
    //            }
    //            if count == 8 {
    //                amm_order_ids_vec.push(order_ids);
    //                order_ids = [0u64; 8];
    //                count = 0;
    //            }
    //        }
    //        if count != 0 {
    //            amm_order_ids_vec.push(order_ids);
    //        }
    //        for ids in amm_order_ids_vec.iter() {
    //            Invokers::invoke_dex_cancel_orders_by_client_order_ids(
    //                market_program_info.clone(),
    //                market_info.clone(),
    //                market_bids_info.clone(),
    //                market_asks_info.clone(),
    //                amm_open_orders_info.clone(),
    //                amm_authority_info.clone(),
    //                market_event_queue_info.clone(),
    //                AUTHORITY_AMM,
    //                amm.nonce as u8,
    //                *ids,
    //            )?;
    //        }
    //        if open_orders.native_coin_total != 0 || open_orders.native_pc_total != 0 {
    //            Invokers::invoke_dex_settle_funds(
    //                market_program_info.clone(),
    //                market_info.clone(),
    //                amm_open_orders_info.clone(),
    //                amm_authority_info.clone(),
    //                market_coin_vault_info.clone(),
    //                market_pc_vault_info.clone(),
    //                amm_coin_vault_info.clone(),
    //                amm_pc_vault_info.clone(),
    //                market_vault_signer.clone(),
    //                token_program_info.clone(),
    //                referrer_pc_account,
    //                AUTHORITY_AMM,
    //                amm.nonce as u8,
    //            )?;
    //        }
    //        if amm.reset_flag == AmmResetFlag::ResetYes.into_u64() {
    //            amm.reset_flag = AmmResetFlag::ResetNo.into_u64();
    //        }
    //        amm.state = AmmState::IdleState.into_u64();
    //        target.plan_orders_cur = 0;
    //        target.place_orders_cur = 0;
    //    }
    //    Ok(())
    //}

    //pub fn process_set_params(
    //    program_id: &Pubkey,
    //    accounts: &[AccountInfo],
    //    setparams: SetParamsInstruction,
    //) -> ProgramResult {
    //    let account_info_iter = &mut accounts.iter();
    //    let token_program_info = next_account_info(account_info_iter)?;

    //    let amm_info = next_account_info(account_info_iter)?;
    //    let amm_authority_info = next_account_info(account_info_iter)?;
    //    let amm_open_orders_info = next_account_info(account_info_iter)?;
    //    let amm_target_orders_info = next_account_info(account_info_iter)?;
    //    let amm_coin_vault_info = next_account_info(account_info_iter)?;
    //    let amm_pc_vault_info = next_account_info(account_info_iter)?;

    //    let market_program_info = next_account_info(account_info_iter)?;
    //    let market_info = next_account_info(account_info_iter)?;
    //    let market_coin_vault_info = next_account_info(account_info_iter)?;
    //    let market_pc_vault_info = next_account_info(account_info_iter)?;
    //    let market_vault_signer = next_account_info(account_info_iter)?;
    //    let market_event_q_info = next_account_info(account_info_iter)?;
    //    let market_bids_info = next_account_info(account_info_iter)?;
    //    let market_asks_info = next_account_info(account_info_iter)?;

    //    let amm_owner_info = next_account_info(account_info_iter)?;

    //    if *token_program_info.key != spl_token::ID {
    //        return Err(AmmError::InvalidSplTokenProgram.into());
    //    }
    //    let mut amm = AmmInfo::load_mut_checked(&amm_info, program_id)?;
    //    if *amm_authority_info.key
    //        != Self::authority_id(program_id, AUTHORITY_AMM, amm.nonce as u8)?
    //    {
    //        return Err(AmmError::InvalidProgramAddress.into());
    //    }
    //    if amm_info.owner != program_id {
    //        return Err(AmmError::InvalidOwner.into());
    //    }
    //    if !amm_owner_info.is_signer || *amm_owner_info.key != config_feature::amm_owner::ID {
    //        return Err(AmmError::InvalidSignAccount.into());
    //    }
    //    if *market_program_info.key != amm.market_program {
    //        return Err(AmmError::InvalidMarketProgram.into());
    //    }
    //    if *market_info.key != amm.market {
    //        return Err(AmmError::InvalidMarket.into());
    //    }
    //    if *amm_open_orders_info.key != amm.open_orders {
    //        return Err(AmmError::InvalidOpenOrders.into());
    //    }
    //    if amm.coin_vault != *amm_coin_vault_info.key {
    //        return Err(AmmError::InvalidCoinVault.into());
    //    }
    //    if amm.pc_vault != *amm_pc_vault_info.key {
    //        return Err(AmmError::InvalidPCVault.into());
    //    }
    //    if amm.target_orders != *amm_target_orders_info.key {
    //        return Err(AmmError::InvalidTargetOrders.into());
    //    }
    //    // cancel amm orders in openbook
    //    Self::do_cancel_amm_orders(
    //        &amm,
    //        amm_authority_info,
    //        amm_open_orders_info,
    //        market_program_info,
    //        market_info,
    //        market_bids_info,
    //        market_asks_info,
    //        market_event_q_info,
    //        AUTHORITY_AMM,
    //    )?;
    //    Invokers::invoke_dex_settle_funds(
    //        market_program_info.clone(),
    //        market_info.clone(),
    //        amm_open_orders_info.clone(),
    //        amm_authority_info.clone(),
    //        market_coin_vault_info.clone(),
    //        market_pc_vault_info.clone(),
    //        amm_coin_vault_info.clone(),
    //        amm_pc_vault_info.clone(),
    //        market_vault_signer.clone(),
    //        token_program_info.clone(),
    //        None,
    //        AUTHORITY_AMM,
    //        amm.nonce as u8,
    //    )?;

    //    let param = setparams.param;
    //    let mut set_valid = false;
    //    match AmmParams::from_u64(param as u64) {
    //        AmmParams::Status => {
    //            {
    //                let (market_state, open_orders) = Processor::load_serum_market_order(
    //                    market_info,
    //                    amm_open_orders_info,
    //                    amm_authority_info,
    //                    &amm,
    //                    false,
    //                )?;
    //                let (shared_pc, shared_coin) = Calculator::calc_exact_vault_in_serum(
    //                    &open_orders,
    //                    &market_state,
    //                    market_event_q_info,
    //                    amm_open_orders_info,
    //                )
    //                .unwrap();
    //                if shared_pc != 0 || shared_coin != 0 {
    //                    msg!("shared_pc:{}, shared_coin:{}", shared_pc, shared_coin);
    //                    return Err(AmmError::InvalidInput.into());
    //                }
    //            }
    //            let value = match setparams.value {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            if AmmStatus::valid_status(value) {
    //                amm.status = value as u64;
    //                set_valid = true;
    //            }
    //        }
    //        AmmParams::State => {
    //            let value = match setparams.value {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            if AmmState::valid_state(value) {
    //                amm.state = value as u64;
    //                set_valid = true;
    //            }
    //        }
    //        AmmParams::OrderNum => {
    //            let value = match setparams.value {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            if value > MAX_ORDER_LIMIT as u64 {
    //                return Err(AmmError::InvalidInput.into());
    //            }
    //            amm.order_num = value as u64;
    //            set_valid = true;
    //        }
    //        AmmParams::Depth => {
    //            let value = match setparams.value {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            if value > 0 && value < 100 {
    //                amm.depth = value as u64;
    //                set_valid = true;
    //            }
    //        }
    //        AmmParams::AmountWave => {
    //            let value = match setparams.value {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            amm.amount_wave = value;
    //            set_valid = true;
    //        }
    //        AmmParams::MinPriceMultiplier => {
    //            let value = match setparams.value {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            if value < amm.max_price_multiplier {
    //                amm.min_price_multiplier = value;
    //                set_valid = true;
    //            }
    //        }
    //        AmmParams::MaxPriceMultiplier => {
    //            let value = match setparams.value {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            if value > amm.max_price_multiplier {
    //                amm.max_price_multiplier = value;
    //                set_valid = true;
    //            }
    //        }
    //        AmmParams::VolMaxCutRatio => {
    //            let value = match setparams.value {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            if value <= TEN_THOUSAND {
    //                amm.vol_max_cut_ratio = value;
    //                set_valid = true;
    //            }
    //        }
    //        AmmParams::Seperate => {
    //            let value = match setparams.value {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            if value <= TEN_THOUSAND {
    //                amm.fees.min_separate_numerator = value;
    //                set_valid = true;
    //            }
    //        }
    //        AmmParams::Fees => {
    //            let fees = match setparams.fees {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            fees.validate()?;
    //            amm.fees = fees;
    //            set_valid = true;
    //        }
    //        AmmParams::AmmOwner => {
    //            let new_pubkey = match setparams.new_pubkey {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            amm.amm_owner = new_pubkey;
    //            set_valid = true;
    //        }
    //        AmmParams::SetOpenTime => {
    //            let value = match setparams.value {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            amm.state_data.pool_open_time = value as u64;
    //            set_valid = true;
    //        }
    //        AmmParams::LastOrderDistance => {
    //            let mut target = TargetOrders::load_mut_checked(
    //                &amm_target_orders_info,
    //                program_id,
    //                amm_info.key,
    //            )?;
    //            let distance = match setparams.last_order_distance {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            target.last_order_numerator = distance.last_order_numerator;
    //            target.last_order_denominator = distance.last_order_denominator;
    //            set_valid = true;
    //        }
    //        AmmParams::InitOrderDepth => {
    //            amm.order_num = 7u64;
    //            amm.depth = 3u64;
    //            set_valid = true;
    //        }
    //        AmmParams::SetSwitchTime => {
    //            let value = match setparams.value {
    //                Some(a) => a,
    //                None => return Err(AmmError::InvalidInput.into()),
    //            };
    //            amm.state_data.orderbook_to_init_time = value as u64;
    //            set_valid = true;
    //        }
    //        AmmParams::ClearOpenTime => {
    //            amm.state_data.pool_open_time = 0;
    //            amm.state_data.orderbook_to_init_time = 0;
    //            set_valid = true;
    //        }
    //        AmmParams::UpdateOpenOrder => {
    //            let new_open_orders_info = next_account_info(account_info_iter)?;
    //            amm.open_orders = *new_open_orders_info.key;
    //            let (_market_state, _open_orders) = Processor::load_serum_market_order(
    //                market_info,
    //                new_open_orders_info,
    //                amm_authority_info,
    //                &amm,
    //                false,
    //            )?;
    //            set_valid = true;
    //        }
    //        _ => {
    //            return Err(AmmError::InvalidInput.into());
    //        }
    //    }
    //    if set_valid {
    //        amm.state = AmmState::CancelAllOrdersState.into_u64();
    //        amm.reset_flag = AmmResetFlag::ResetYes.into_u64();
    //    } else {
    //        return Err(AmmError::InvalidParamsSet.into());
    //    }
    //    amm.recent_epoch = Clock::get()?.epoch;
    //    Ok(())
    //}

    //pub fn process_monitor_step(
    //    program_id: &Pubkey,
    //    accounts: &[AccountInfo],
    //    monitor: MonitorStepInstruction,
    //) -> ProgramResult {
    //    const MIN_ACCOUNTS: usize = 18;
    //    let input_account_len = accounts.len();
    //    if input_account_len != MIN_ACCOUNTS
    //        && input_account_len != MIN_ACCOUNTS + 1
    //        && input_account_len != MIN_ACCOUNTS + 2
    //    {
    //        return Err(AmmError::WrongAccountsNumber.into());
    //    }
    //    let (fixed_accounts, extra_token_accounts) = array_refs![accounts, MIN_ACCOUNTS; .. ;];
    //    let [
    //        token_program_info,
    //        rent_info,
    //        clock_info,
    //        amm_info,
    //        amm_authority_info,
    //        amm_open_orders_info,
    //        amm_target_orders_info,
    //        amm_coin_vault_info,
    //        amm_pc_vault_info,
    //        market_program_info,
    //        market_info,
    //        market_coin_vault_info,
    //        market_pc_vault_info,
    //        market_vault_signer,
    //        market_request_queue_info,
    //        market_event_queue_info,
    //        market_bids_info,
    //        market_asks_info,
    //    ] = fixed_accounts;
    //    let mut srm_token_info = None;
    //    let mut referrer_pc_info = None;
    //    if input_account_len == MIN_ACCOUNTS + 1 {
    //        if let [srm_token_acc] = extra_token_accounts {
    //            srm_token_info = Some(srm_token_acc);
    //        }
    //    } else if input_account_len == MIN_ACCOUNTS + 2 {
    //        if let [srm_token_acc, referrer_pc_acc] = extra_token_accounts {
    //            srm_token_info = Some(srm_token_acc);
    //            referrer_pc_info = Some(referrer_pc_acc);
    //        }
    //    }

    //    let mut amm = AmmInfo::load_mut_checked(amm_info, program_id)?;
    //    Self::check_accounts(
    //        program_id,
    //        &amm,
    //        amm_info,
    //        token_program_info,
    //        clock_info,
    //        market_program_info,
    //        amm_authority_info,
    //        market_info,
    //        amm_open_orders_info,
    //        amm_coin_vault_info,
    //        amm_pc_vault_info,
    //        amm_target_orders_info,
    //        srm_token_info,
    //        referrer_pc_info,
    //    )?;
    //    check_assert_eq!(
    //        *market_program_info.key,
    //        config_feature::openbook_program::id(),
    //        "market_program",
    //        AmmError::InvalidMarketProgram
    //    );
    //    if amm_info.owner != program_id {
    //        return Err(AmmError::InvalidOwner.into());
    //    }
    //    if amm.min_size == 0 {
    //        return Err(AmmError::MarketLotSizeIsTooLarge.into());
    //    }
    //    let amm_status = AmmStatus::from_u64(amm.status);
    //    let amm_state = AmmState::from_u64(amm.state);
    //    let spl_token_program_id = token_program_info.key;

    //    if amm.reset_flag == AmmResetFlag::ResetYes.into_u64() {
    //        msg!("monitor_step: ResetYes");
    //        let cancel_all_orders_accounts: &[AccountInfo] = &[
    //            amm_info.clone(),
    //            market_program_info.clone(),
    //            market_info.clone(),
    //            amm_open_orders_info.clone(),
    //            amm_authority_info.clone(),
    //            market_event_queue_info.clone(),
    //            market_coin_vault_info.clone(),
    //            market_pc_vault_info.clone(),
    //            market_bids_info.clone(),
    //            market_asks_info.clone(),
    //            market_vault_signer.clone(),
    //            token_program_info.clone(),
    //            amm_coin_vault_info.clone(),
    //            amm_pc_vault_info.clone(),
    //            amm_target_orders_info.clone(),
    //        ];
    //        account_parser::CancelAllOrdersArgs::with_parsed_args(
    //            program_id,
    //            monitor.cancel_order_limit,
    //            &mut amm,
    //            cancel_all_orders_accounts,
    //            referrer_pc_info,
    //            Self::do_cancel_all_orders_state,
    //        )
    //        .unwrap();
    //    } else if amm.order_num == 0 {
    //        msg!(
    //            arrform!(
    //                LOG_SIZE,
    //                "monitor_step Status:{}, order_num:{}",
    //                identity(amm.status),
    //                identity(amm.order_num)
    //            )
    //            .as_str()
    //        );
    //    } else {
    //        match amm_status {
    //            AmmStatus::Uninitialized
    //            | AmmStatus::Disabled
    //            | AmmStatus::WithdrawOnly
    //            | AmmStatus::LiquidityOnly
    //            | AmmStatus::SwapOnly
    //            | AmmStatus::WaitingTrade => {
    //                msg!("monitor_step: AmmStatus:{}", identity(amm.status));
    //                return Err(AmmError::InvalidStatus.into());
    //            }
    //            AmmStatus::Initialized | AmmStatus::OrderBookOnly => match amm_state {
    //                AmmState::IdleState => {
    //                    msg!("monitor_step IdleState:{}", identity(amm.state));
    //                    let idle_accounts: &[AccountInfo] = &[
    //                        amm_info.clone(),
    //                        market_program_info.clone(),
    //                        market_info.clone(),
    //                        market_bids_info.clone(),
    //                        market_asks_info.clone(),
    //                        market_event_queue_info.clone(),
    //                        amm_authority_info.clone(),
    //                        amm_open_orders_info.clone(),
    //                        amm_coin_vault_info.clone(),
    //                        amm_pc_vault_info.clone(),
    //                        amm_target_orders_info.clone(),
    //                    ];
    //                    account_parser::IdleArgs::with_parsed_args(
    //                        program_id,
    //                        spl_token_program_id,
    //                        &mut amm,
    //                        idle_accounts,
    //                        Self::do_idle_state,
    //                    )
    //                    .unwrap();
    //                }
    //                AmmState::CancelAllOrdersState => {
    //                    msg!("monitor_step CancelAllOrdersState:{}", identity(amm.state));
    //                    let cancel_all_orders_accounts: &[AccountInfo] = &[
    //                        amm_info.clone(),
    //                        market_program_info.clone(),
    //                        market_info.clone(),
    //                        amm_open_orders_info.clone(),
    //                        amm_authority_info.clone(),
    //                        market_event_queue_info.clone(),
    //                        market_coin_vault_info.clone(),
    //                        market_pc_vault_info.clone(),
    //                        market_bids_info.clone(),
    //                        market_asks_info.clone(),
    //                        market_vault_signer.clone(),
    //                        token_program_info.clone(),
    //                        amm_coin_vault_info.clone(),
    //                        amm_pc_vault_info.clone(),
    //                        amm_target_orders_info.clone(),
    //                    ];
    //                    account_parser::CancelAllOrdersArgs::with_parsed_args(
    //                        program_id,
    //                        monitor.cancel_order_limit,
    //                        &mut amm,
    //                        cancel_all_orders_accounts,
    //                        referrer_pc_info,
    //                        Self::do_cancel_all_orders_state,
    //                    )
    //                    .unwrap();
    //                }
    //                AmmState::PlanOrdersState => {
    //                    msg!("monitor_step PlanOrdersState:{}", identity(amm.state));
    //                    let plan_buy_accounts: &[AccountInfo] = &[
    //                        amm_info.clone(),
    //                        market_info.clone(),
    //                        market_event_queue_info.clone(),
    //                        amm_authority_info.clone(),
    //                        amm_open_orders_info.clone(),
    //                        amm_coin_vault_info.clone(),
    //                        amm_pc_vault_info.clone(),
    //                        amm_target_orders_info.clone(),
    //                    ];
    //                    account_parser::PlanOrderBookArgs::with_parsed_args(
    //                        program_id,
    //                        spl_token_program_id,
    //                        monitor.plan_order_limit,
    //                        &mut amm,
    //                        plan_buy_accounts,
    //                        Self::do_plan_orderbook,
    //                    )
    //                    .unwrap();
    //                }
    //                AmmState::CancelOrderState => {
    //                    msg!("monitor_step CancelOrderState:{}", identity(amm.state));
    //                    let cancel_order_accounts: &[AccountInfo] = &[
    //                        amm_info.clone(),
    //                        market_program_info.clone(),
    //                        market_info.clone(),
    //                        amm_open_orders_info.clone(),
    //                        amm_authority_info.clone(),
    //                        market_event_queue_info.clone(),
    //                        market_bids_info.clone(),
    //                        market_asks_info.clone(),
    //                        amm_coin_vault_info.clone(),
    //                        amm_pc_vault_info.clone(),
    //                        amm_target_orders_info.clone(),
    //                    ];
    //                    account_parser::CancelOrderArgs::with_parsed_args(
    //                        program_id,
    //                        spl_token_program_id,
    //                        monitor.cancel_order_limit,
    //                        &mut amm,
    //                        cancel_order_accounts,
    //                        Self::do_cancel_order,
    //                    )
    //                    .unwrap();
    //                }
    //                AmmState::PlaceOrdersState => {
    //                    msg!("monitor_step PlaceOrdersState:{}", identity(amm.state));
    //                    let place_order_accounts: &[AccountInfo] = &[
    //                        amm_info.clone(),
    //                        amm_authority_info.clone(),
    //                        amm_open_orders_info.clone(),
    //                        market_program_info.clone(),
    //                        market_info.clone(),
    //                        market_request_queue_info.clone(),
    //                        amm_coin_vault_info.clone(),
    //                        amm_pc_vault_info.clone(),
    //                        market_coin_vault_info.clone(),
    //                        market_pc_vault_info.clone(),
    //                        token_program_info.clone(),
    //                        rent_info.clone(),
    //                        market_event_queue_info.clone(),
    //                        market_bids_info.clone(),
    //                        market_asks_info.clone(),
    //                        amm_target_orders_info.clone(),
    //                        clock_info.clone(),
    //                    ];
    //                    account_parser::PlaceOrdersArgs::with_parsed_args(
    //                        program_id,
    //                        spl_token_program_id,
    //                        monitor.place_order_limit,
    //                        &mut amm,
    //                        place_order_accounts,
    //                        srm_token_info,
    //                        Self::do_place_orders,
    //                    )
    //                    .unwrap();
    //                }
    //                AmmState::PurgeOrderState => {
    //                    msg!("monitor_step PurgeOrderState:{}", identity(amm.state));
    //                    let purge_orders_accounts: &[AccountInfo] = &[
    //                        amm_info.clone(),
    //                        market_program_info.clone(),
    //                        market_info.clone(),
    //                        amm_open_orders_info.clone(),
    //                        amm_authority_info.clone(),
    //                        market_event_queue_info.clone(),
    //                        market_bids_info.clone(),
    //                        market_asks_info.clone(),
    //                        amm_target_orders_info.clone(),
    //                    ];
    //                    account_parser::PurgeOrderArgs::with_parsed_args(
    //                        program_id,
    //                        spl_token_program_id,
    //                        monitor.cancel_order_limit,
    //                        &mut amm,
    //                        purge_orders_accounts,
    //                        Self::do_purge_orders,
    //                    )
    //                    .unwrap();
    //                }
    //                _ => {
    //                    msg!("monitor_step InvalidState:{}", identity(amm.state));
    //                }
    //            },
    //        }
    //    }
    //    msg!("monitor_step end");
    //    Ok(())
    //}

    //fn do_cancel_amm_orders<'a>(
    //    amm: &AmmInfo,
    //    amm_authority_info: &AccountInfo<'a>,
    //    amm_open_orders_info: &AccountInfo<'a>,
    //    market_program_info: &AccountInfo<'a>,
    //    market_info: &AccountInfo<'a>,
    //    market_bids_info: &AccountInfo<'a>,
    //    market_asks_info: &AccountInfo<'a>,
    //    market_event_queue_info: &AccountInfo<'a>,
    //    amm_seed: &[u8],
    //) -> ProgramResult {
    //    let (market_state, open_orders) = Processor::load_serum_market_order(
    //        market_info,
    //        amm_open_orders_info,
    //        amm_authority_info,
    //        amm,
    //        false,
    //    )?;
    //    let bids_orders = market_state.load_bids_checked(market_bids_info)?;
    //    let asks_orders = market_state.load_asks_checked(market_asks_info)?;
    //    let (bids, asks) = Processor::get_amm_orders(&open_orders, bids_orders, asks_orders)?;

    //    let mut amm_order_ids_vec = Vec::new();
    //    let mut order_ids = [0u64; 8];
    //    let mut count = 0;
    //    for i in 0..std::cmp::max(bids.len(), asks.len()) {
    //        if i < bids.len() {
    //            order_ids[count] = bids[i].client_order_id();
    //            count += 1;
    //        }
    //        if i < asks.len() {
    //            order_ids[count] = asks[i].client_order_id();
    //            count += 1;
    //        }
    //        if count == 8 {
    //            amm_order_ids_vec.push(order_ids);
    //            order_ids = [0u64; 8];
    //            count = 0;
    //        }
    //    }
    //    if count != 0 {
    //        amm_order_ids_vec.push(order_ids);
    //    }
    //    for ids in amm_order_ids_vec.iter() {
    //        Invokers::invoke_dex_cancel_orders_by_client_order_ids(
    //            market_program_info.clone(),
    //            market_info.clone(),
    //            market_bids_info.clone(),
    //            market_asks_info.clone(),
    //            amm_open_orders_info.clone(),
    //            amm_authority_info.clone(),
    //            market_event_queue_info.clone(),
    //            amm_seed,
    //            amm.nonce as u8,
    //            *ids,
    //        )?;
    //    }

    //    Ok(())
    //}

    //pub fn process_admin_cancel_orders(
    //    program_id: &Pubkey,
    //    accounts: &[AccountInfo],
    //    _cancel: AdminCancelOrdersInstruction,
    //) -> ProgramResult {
    //    const MIN_ACCOUNTS: usize = 17;
    //    let input_account_len = accounts.len();
    //    if input_account_len != MIN_ACCOUNTS
    //        && input_account_len != MIN_ACCOUNTS + 1
    //        && input_account_len != MIN_ACCOUNTS + 2
    //    {
    //        return Err(AmmError::WrongAccountsNumber.into());
    //    }

    //    let account_info_iter = &mut accounts.iter();
    //    let token_program_info = next_account_info(account_info_iter)?;

    //    let amm_info = next_account_info(account_info_iter)?;
    //    let amm_authority_info = next_account_info(account_info_iter)?;
    //    let amm_open_orders_info = next_account_info(account_info_iter)?;
    //    let amm_target_orders_info = next_account_info(account_info_iter)?;
    //    let amm_coin_vault_info = next_account_info(account_info_iter)?;
    //    let amm_pc_vault_info = next_account_info(account_info_iter)?;
    //    let amm_owner_info = next_account_info(account_info_iter)?;
    //    let amm_config_info = next_account_info(account_info_iter)?;

    //    let market_program_info = next_account_info(account_info_iter)?;
    //    let market_info = next_account_info(account_info_iter)?;
    //    let market_coin_vault_info = next_account_info(account_info_iter)?;
    //    let market_pc_vault_info = next_account_info(account_info_iter)?;
    //    let market_vault_signer_info = next_account_info(account_info_iter)?;
    //    let market_event_queue_info = next_account_info(account_info_iter)?;
    //    let market_bids_info = next_account_info(account_info_iter)?;
    //    let market_asks_info = next_account_info(account_info_iter)?;

    //    let mut srm_token_account = None;
    //    let mut referrer_pc_wallet = None;
    //    if input_account_len == MIN_ACCOUNTS + 1 {
    //        let srm_token_info = next_account_info(account_info_iter)?;
    //        srm_token_account = Some(srm_token_info);
    //    } else if input_account_len == MIN_ACCOUNTS + 2 {
    //        let srm_token_info = next_account_info(account_info_iter)?;
    //        let referrer_pc_info = next_account_info(account_info_iter)?;
    //        srm_token_account = Some(srm_token_info);
    //        referrer_pc_wallet = Some(referrer_pc_info);
    //    }

    //    if *token_program_info.key != spl_token::ID {
    //        return Err(AmmError::InvalidSplTokenProgram.into());
    //    }
    //    let amm = AmmInfo::load_checked(&amm_info, program_id)?;
    //    if *amm_authority_info.key
    //        != Self::authority_id(program_id, AUTHORITY_AMM, amm.nonce as u8)?
    //    {
    //        return Err(AmmError::InvalidProgramAddress.into());
    //    }
    //    if amm_info.owner != program_id {
    //        return Err(AmmError::InvalidOwner.into());
    //    }
    //    let (pda, _) = Pubkey::find_program_address(&[&AMM_CONFIG_SEED], program_id);
    //    if pda != *amm_config_info.key {
    //        return Err(AmmError::InvalidConfigAccount.into());
    //    }
    //    let amm_config = AmmConfig::load_checked(&amm_config_info, program_id)?;
    //    if !amm_owner_info.is_signer
    //        || (*amm_owner_info.key != config_feature::amm_owner::ID
    //            && *amm_owner_info.key != amm_config.cancel_owner)
    //    {
    //        return Err(AmmError::InvalidSignAccount.into());
    //    }

    //    if amm.status == AmmStatus::Uninitialized.into_u64() {
    //        return Err(AmmError::InvalidStatus.into());
    //    }
    //    if *market_program_info.key != amm.market_program {
    //        return Err(AmmError::InvalidMarketProgram.into());
    //    }
    //    if *market_info.key != amm.market {
    //        return Err(AmmError::InvalidMarket.into());
    //    }
    //    if *amm_open_orders_info.key != amm.open_orders {
    //        return Err(AmmError::InvalidOpenOrders.into());
    //    }
    //    if amm.coin_vault != *amm_coin_vault_info.key {
    //        return Err(AmmError::InvalidCoinVault.into());
    //    }
    //    if amm.pc_vault != *amm_pc_vault_info.key {
    //        return Err(AmmError::InvalidPCVault.into());
    //    }
    //    if amm.target_orders != *amm_target_orders_info.key {
    //        return Err(AmmError::InvalidTargetOrders.into());
    //    }
    //    if let Some(srm_token_info) = srm_token_account {
    //        let srm_token = Self::unpack_token_account(&srm_token_info, &spl_token::id())?;
    //        if *amm_authority_info.key != srm_token.owner {
    //            return Err(AmmError::InvalidOwner.into());
    //        }
    //        if srm_token.mint != srm_token::ID && srm_token.mint != msrm_token::ID {
    //            return Err(AmmError::InvalidSrmMint.into());
    //        }
    //    }
    //    if let Some(referrer_pc_info) = referrer_pc_wallet {
    //        let referrer_pc_token =
    //            Self::unpack_token_account(&referrer_pc_info, &spl_token::id())?;
    //        if referrer_pc_token.owner != config_feature::referrer_pc_wallet::id() {
    //            return Err(AmmError::InvalidOwner.into());
    //        }
    //        if referrer_pc_token.mint != amm.pc_vault_mint {
    //            return Err(AmmError::InvalidReferPCMint.into());
    //        }
    //    }
    //    Self::do_cancel_amm_orders(
    //        &amm,
    //        amm_authority_info,
    //        amm_open_orders_info,
    //        market_program_info,
    //        market_info,
    //        market_bids_info,
    //        market_asks_info,
    //        market_event_queue_info,
    //        AUTHORITY_AMM,
    //    )?;
    //    let (open_order_coin_free, open_order_pc_free) = {
    //        let (_market_state, open_orders) = Processor::load_serum_market_order(
    //            market_info,
    //            amm_open_orders_info,
    //            amm_authority_info,
    //            &amm,
    //            true,
    //        )?;
    //        (open_orders.native_coin_free, open_orders.native_pc_free)
    //    };
    //    if open_order_coin_free == 0 && open_order_pc_free == 0 {
    //        return Ok(());
    //    }
    //    Invokers::invoke_dex_settle_funds(
    //        market_program_info.clone(),
    //        market_info.clone(),
    //        amm_open_orders_info.clone(),
    //        amm_authority_info.clone(),
    //        market_coin_vault_info.clone(),
    //        market_pc_vault_info.clone(),
    //        amm_coin_vault_info.clone(),
    //        amm_pc_vault_info.clone(),
    //        market_vault_signer_info.clone(),
    //        token_program_info.clone(),
    //        referrer_pc_wallet,
    //        AUTHORITY_AMM,
    //        amm.nonce as u8,
    //    )?;
    //    Ok(())
    //}

    ///// Processes `process_create_config` instruction.
    //pub fn process_create_config(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    //    let account_info_iter = &mut accounts.iter();
    //    let admin_info = next_account_info(account_info_iter)?;
    //    let amm_config_info = next_account_info(account_info_iter)?;
    //    let pnl_owner_info = next_account_info(account_info_iter)?;
    //    let system_program_info = next_account_info(account_info_iter)?;
    //    let rent_sysvar_info = next_account_info(account_info_iter)?;

    //    if !admin_info.is_signer || config_feature::amm_owner::id() != *admin_info.key {
    //        return Err(AmmError::InvalidSignAccount.into());
    //    }
    //    if *system_program_info.key != solana_program::system_program::id() {
    //        return Err(AmmError::InvalidSysProgramAddress.into());
    //    }

    //    let (pda, bump_seed) = Pubkey::find_program_address(&[&&AMM_CONFIG_SEED], program_id);
    //    if pda != *amm_config_info.key {
    //        return Err(AmmError::InvalidConfigAccount.into());
    //    }
    //    if amm_config_info.owner != system_program_info.key {
    //        return Err(AmmError::RepeatCreateConfigAccount.into());
    //    }
    //    let pda_signer_seeds: &[&[_]] = &[&AMM_CONFIG_SEED, &[bump_seed]];
    //    let rent = &Rent::from_account_info(rent_sysvar_info)?;
    //    let data_size = size_of::<AmmConfig>();
    //    let required_lamports = rent
    //        .minimum_balance(data_size)
    //        .max(1)
    //        .saturating_sub(amm_config_info.lamports());
    //    if required_lamports > 0 {
    //        invoke(
    //            &system_instruction::transfer(
    //                admin_info.key,
    //                amm_config_info.key,
    //                required_lamports,
    //            ),
    //            &[
    //                admin_info.clone(),
    //                amm_config_info.clone(),
    //                system_program_info.clone(),
    //            ],
    //        )?;
    //    }
    //    invoke_signed(
    //        &system_instruction::allocate(amm_config_info.key, data_size as u64),
    //        &[amm_config_info.clone(), system_program_info.clone()],
    //        &[&pda_signer_seeds],
    //    )?;
    //    invoke_signed(
    //        &system_instruction::assign(amm_config_info.key, &program_id),
    //        &[amm_config_info.clone(), system_program_info.clone()],
    //        &[&pda_signer_seeds],
    //    )?;

    //    let mut amm_config = AmmConfig::load_mut_checked(&amm_config_info, program_id)?;
    //    amm_config.pnl_owner = *pnl_owner_info.key;
    //    amm_config.create_pool_fee = 0;

    //    Ok(())
    //}

    ///// Processes `process_update_config` instruction.
    //pub fn process_update_config(
    //    program_id: &Pubkey,
    //    accounts: &[AccountInfo],
    //    config_args: ConfigArgs,
    //) -> ProgramResult {
    //    let account_info_iter = &mut accounts.iter();
    //    let admin_info = next_account_info(account_info_iter)?;
    //    let amm_config_info = next_account_info(account_info_iter)?;
    //    if !admin_info.is_signer || config_feature::amm_owner::id() != *admin_info.key {
    //        return Err(AmmError::InvalidSignAccount.into());
    //    }
    //    let (pda, _) = Pubkey::find_program_address(&[&AMM_CONFIG_SEED], program_id);
    //    if pda != *amm_config_info.key || amm_config_info.owner != program_id {
    //        return Err(AmmError::InvalidConfigAccount.into());
    //    }

    //    let mut amm_config = AmmConfig::load_mut_checked(&amm_config_info, program_id)?;
    //    match config_args.param {
    //        0 => {
    //            let pnl_owner = config_args.owner.unwrap();
    //            if pnl_owner == Pubkey::default() {
    //                return Err(AmmError::InvalidInput.into());
    //            }
    //            amm_config.pnl_owner = pnl_owner;
    //        }
    //        1 => {
    //            let cancel_owner = config_args.owner.unwrap();
    //            if cancel_owner == Pubkey::default() {
    //                return Err(AmmError::InvalidInput.into());
    //            }
    //            amm_config.cancel_owner = cancel_owner;
    //        }
    //        2 => {
    //            let create_pool_fee = config_args.create_pool_fee.unwrap();
    //            amm_config.create_pool_fee = create_pool_fee;
    //        }
    //        _ => {
    //            return Err(AmmError::InvalidInput.into());
    //        }
    //    }

    //    return Ok(());
    //}

    ///// Processes an [Instruction](enum.Instruction.html).
    //pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
    //    let instruction = AmmInstruction::unpack(input)?;
    //    match instruction {
    //        AmmInstruction::PreInitialize(_init_arg) => {
    //            unimplemented!("This instruction is not supported, please use Initialize2")
    //        }
    //        AmmInstruction::Initialize(_init1) => {
    //            unimplemented!("This instruction is not supported, please use Initialize2")
    //        }
    //        AmmInstruction::Initialize2(init2) => {
    //            Self::process_initialize2(program_id, accounts, init2)
    //        }
    //        AmmInstruction::MonitorStep(monitor) => {
    //            Self::process_monitor_step(program_id, accounts, monitor)
    //        }
    //        AmmInstruction::Deposit(deposit) => {
    //            Self::process_deposit(program_id, accounts, deposit)
    //        }
    //        AmmInstruction::Withdraw(withdraw) => {
    //            Self::process_withdraw(program_id, accounts, withdraw)
    //        }
    //        AmmInstruction::MigrateToOpenBook => {
    //            Self::process_migrate_to_openbook(program_id, accounts)
    //        }
    //        AmmInstruction::SetParams(setparams) => {
    //            Self::process_set_params(program_id, accounts, setparams)
    //        }
    //        AmmInstruction::WithdrawPnl => Self::process_withdrawpnl(program_id, accounts),
    //        AmmInstruction::WithdrawSrm(withdrawsrm) => {
    //            Self::process_withdraw_srm(program_id, accounts, withdrawsrm)
    //        }
    //        AmmInstruction::SwapBaseIn(swap) => {
    //            Self::process_swap_base_in(program_id, accounts, swap)
    //        }
    //        AmmInstruction::SwapBaseOut(swap) => {
    //            Self::process_swap_base_out(program_id, accounts, swap)
    //        }
    //        AmmInstruction::SimulateInfo(simulate) => {
    //            Self::process_simulate_info(program_id, accounts, simulate)
    //        }
    //        AmmInstruction::AdminCancelOrders(cancel) => {
    //            Self::process_admin_cancel_orders(program_id, accounts, cancel)
    //        }
    //        AmmInstruction::CreateConfigAccount => {
    //            Self::process_create_config(program_id, accounts)
    //        }
    //        AmmInstruction::UpdateConfigAccount(config_args) => {
    //            Self::process_update_config(program_id, accounts, config_args)
    //        }
    //    }
    //}

    pub fn process_swap_base_in(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        swap: SwapInstructionBaseIn,
    ) -> ProgramResult {
        const ACCOUNT_LEN: usize = 17;
        let input_account_len = accounts.len();
        if input_account_len != ACCOUNT_LEN && input_account_len != ACCOUNT_LEN + 1 {
            return Err(AmmError::WrongAccountsNumber.into());
        }
        let account_info_iter = &mut accounts.iter();
        let token_program_info = next_account_info(account_info_iter)?;

        let amm_info = next_account_info(account_info_iter)?;
        let amm_authority_info = next_account_info(account_info_iter)?;
        let amm_open_orders_info = next_account_info(account_info_iter)?;
        if input_account_len == ACCOUNT_LEN + 1 {
            let _amm_target_orders_info = next_account_info(account_info_iter)?;
        }
        let amm_coin_vault_info = next_account_info(account_info_iter)?;
        let amm_pc_vault_info = next_account_info(account_info_iter)?;

        let market_program_info = next_account_info(account_info_iter)?;

        let mut amm = AmmInfo::load_mut_checked(&amm_info, program_id)?;
        let enable_orderbook;
        if AmmStatus::from_u64(amm.status).orderbook_permission() {
            enable_orderbook = true;
        } else {
            enable_orderbook = false;
        }
        let market_info = next_account_info(account_info_iter)?;
        let market_bids_info = next_account_info(account_info_iter)?;
        let market_asks_info = next_account_info(account_info_iter)?;
        let market_event_queue_info = next_account_info(account_info_iter)?;
        let market_coin_vault_info = next_account_info(account_info_iter)?;
        let market_pc_vault_info = next_account_info(account_info_iter)?;
        let market_vault_signer = next_account_info(account_info_iter)?;

        let user_source_info = next_account_info(account_info_iter)?;
        let user_destination_info = next_account_info(account_info_iter)?;
        let user_source_owner = next_account_info(account_info_iter)?;
        if !user_source_owner.is_signer {
            return Err(AmmError::InvalidSignAccount.into());
        }
        check_assert_eq!(
            *token_program_info.key,
            spl_token::id(),
            "spl_token_program",
            AmmError::InvalidSplTokenProgram
        );
        let spl_token_program_id = token_program_info.key;
        if *amm_authority_info.key
            != Self::authority_id(program_id, AUTHORITY_AMM, amm.nonce as u8)?
        {
            return Err(AmmError::InvalidProgramAddress.into());
        }
        check_assert_eq!(
            *amm_coin_vault_info.key,
            amm.coin_vault,
            "coin_vault",
            AmmError::InvalidCoinVault
        );
        check_assert_eq!(
            *amm_pc_vault_info.key,
            amm.pc_vault,
            "pc_vault",
            AmmError::InvalidPCVault
        );

        if *user_source_info.key == amm.pc_vault || *user_source_info.key == amm.coin_vault {
            return Err(AmmError::InvalidUserToken.into());
        }
        if *user_destination_info.key == amm.pc_vault
            || *user_destination_info.key == amm.coin_vault
        {
            return Err(AmmError::InvalidUserToken.into());
        }

        let amm_coin_vault =
            Self::unpack_token_account(&amm_coin_vault_info, spl_token_program_id)?;
        let amm_pc_vault = Self::unpack_token_account(&amm_pc_vault_info, spl_token_program_id)?;

        let user_source = Self::unpack_token_account(&user_source_info, spl_token_program_id)?;
        let user_destination =
            Self::unpack_token_account(&user_destination_info, spl_token_program_id)?;

        if !AmmStatus::from_u64(amm.status).swap_permission() {
            msg!(&format!("swap_base_in: status {}", identity(amm.status)));
            let clock = Clock::get()?;
            if amm.status == AmmStatus::OrderBookOnly.into_u64()
                && (clock.unix_timestamp as u64) >= amm.state_data.orderbook_to_init_time
            {
                amm.status = AmmStatus::Initialized.into_u64();
                msg!("swap_base_in: OrderBook to Initialized");
            } else {
                return Err(AmmError::InvalidStatus.into());
            }
        } else if amm.status == AmmStatus::WaitingTrade.into_u64() {
            let clock = Clock::get()?;
            if (clock.unix_timestamp as u64) < amm.state_data.pool_open_time {
                return Err(AmmError::InvalidStatus.into());
            } else {
                amm.status = AmmStatus::SwapOnly.into_u64();
                msg!("swap_base_in: WaitingTrade to SwapOnly");
            }
        }

        let total_pc_without_take_pnl;
        let total_coin_without_take_pnl;
        let mut bids: Vec<LeafNode> = Vec::new();
        let mut asks: Vec<LeafNode> = Vec::new();
        if enable_orderbook {
            check_assert_eq!(
                *amm_open_orders_info.key,
                amm.open_orders,
                "open_orders",
                AmmError::InvalidOpenOrders
            );
            check_assert_eq!(
                *market_program_info.key,
                amm.market_program,
                "market_program",
                AmmError::InvalidMarketProgram
            );
            check_assert_eq!(
                *market_info.key,
                amm.market,
                "market",
                AmmError::InvalidMarket
            );
            //let (market_state, open_orders) = Processor::load_serum_market_order(
            //    market_info,
            //    amm_open_orders_info,
            //    amm_authority_info,
            //    &amm,
            //    false,
            //)?;
            //let bids_orders = market_state.load_bids_checked(&market_bids_info)?;
            //let asks_orders = market_state.load_asks_checked(&market_asks_info)?;
            //(bids, asks) = Self::get_amm_orders(&open_orders, bids_orders, asks_orders)?;
            //(total_pc_without_take_pnl, total_coin_without_take_pnl) =
            //    Calculator::calc_total_without_take_pnl(
            //        amm_pc_vault.amount,
            //        amm_coin_vault.amount,
            //        &open_orders,
            //        &amm,
            //        &market_state,
            //        &market_event_queue_info,
            //        &amm_open_orders_info,
            //    )?;
        } else {
            (total_pc_without_take_pnl, total_coin_without_take_pnl) =
                Calculator::calc_total_without_take_pnl_no_orderbook(
                    amm_pc_vault.amount,
                    amm_coin_vault.amount,
                    &amm,
                )?;
        }

        let swap_direction;
        if user_source.mint == amm_coin_vault.mint && user_destination.mint == amm_pc_vault.mint {
            swap_direction = SwapDirection::Coin2PC
        } else if user_source.mint == amm_pc_vault.mint
            && user_destination.mint == amm_coin_vault.mint
        {
            swap_direction = SwapDirection::PC2Coin
        } else {
            return Err(AmmError::InvalidUserToken.into());
        }
        if user_source.amount < swap.amount_in {
            encode_ray_log(SwapBaseInLog {
                log_type: LogType::SwapBaseIn.into_u8(),
                amount_in: swap.amount_in,
                minimum_out: swap.minimum_amount_out,
                direction: swap_direction as u64,
                user_source: user_source.amount,
                //pool_coin: total_coin_without_take_pnl,
                pool_coin: 9234u64,
                //pool_pc: total_pc_without_take_pnl,
                pool_pc: 99u64,
                out_amount: 0,
            });
            return Err(AmmError::InsufficientFunds.into());
        }
        let swap_fee = U128::from(swap.amount_in)
            .checked_mul(amm.fees.swap_fee_numerator.into())
            .unwrap()
            .checked_ceil_div(amm.fees.swap_fee_denominator.into())
            .unwrap()
            .0;
        let swap_in_after_deduct_fee = U128::from(swap.amount_in).checked_sub(swap_fee).unwrap();
        let swap_amount_out = 100u64;
        //let swap_amount_out = Calculator::swap_token_amount_base_in(
        //    swap_in_after_deduct_fee,
        //    total_pc_without_take_pnl.into(),
        //    total_coin_without_take_pnl.into(),
        //    swap_direction,
        //)
        //.as_u64();
        encode_ray_log(SwapBaseInLog {
            log_type: LogType::SwapBaseIn.into_u8(),
            amount_in: swap.amount_in,
            minimum_out: swap.minimum_amount_out,
            direction: swap_direction as u64,
            user_source: user_source.amount,
            //pool_coin: total_coin_without_take_pnl,
            pool_coin: 134u64,
            //pool_pc: total_pc_without_take_pnl,
            pool_pc: 468u64,
            out_amount: swap_amount_out,
        });
        if swap_amount_out < swap.minimum_amount_out {
            return Err(AmmError::ExceededSlippage.into());
        }
        if swap_amount_out == 0 || swap.amount_in == 0 {
            return Err(AmmError::InvalidInput.into());
        }

        match swap_direction {
            SwapDirection::Coin2PC => {
                //if swap_amount_out >= total_pc_without_take_pnl {
                //    return Err(AmmError::InsufficientFunds.into());
                //}

                if enable_orderbook {
                    // coin -> pc, need cancel buy order
                    if !bids.is_empty() {
                        let mut amm_order_ids_vec = Vec::new();
                        let mut order_ids = [0u64; 8];
                        let mut count = 0;
                        // fetch cancel order ids{
                        for order in bids.into_iter() {
                            order_ids[count] = order.client_order_id();
                            count += 1;
                            if count == 8 {
                                amm_order_ids_vec.push(order_ids);
                                order_ids = [0u64; 8];
                                count = 0;
                            }
                        }
                        if count != 0 {
                            amm_order_ids_vec.push(order_ids);
                        }
                        //for ids in amm_order_ids_vec.iter() {
                        //    Invokers::invoke_dex_cancel_orders_by_client_order_ids(
                        //        market_program_info.clone(),
                        //        market_info.clone(),
                        //        market_bids_info.clone(),
                        //        market_asks_info.clone(),
                        //        amm_open_orders_info.clone(),
                        //        amm_authority_info.clone(),
                        //        market_event_queue_info.clone(),
                        //        AUTHORITY_AMM,
                        //        amm.nonce as u8,
                        //        *ids,
                        //    )?;
                        //}
                    }

                    if swap_amount_out > amm_pc_vault.amount {
                        // need settle funds
                        //Invokers::invoke_dex_settle_funds(
                        //    market_program_info.clone(),
                        //    market_info.clone(),
                        //    amm_open_orders_info.clone(),
                        //    amm_authority_info.clone(),
                        //    market_coin_vault_info.clone(),
                        //    market_pc_vault_info.clone(),
                        //    amm_coin_vault_info.clone(),
                        //    amm_pc_vault_info.clone(),
                        //    market_vault_signer.clone(),
                        //    token_program_info.clone(),
                        //    Some(&amm_pc_vault_info.clone()),
                        //    AUTHORITY_AMM,
                        //    amm.nonce as u8,
                        //)?;
                    }
                }
                // deposit source coin to amm_coin_vault
                Invokers::token_transfer(
                    token_program_info.clone(),
                    user_source_info.clone(),
                    amm_coin_vault_info.clone(),
                    user_source_owner.clone(),
                    swap.amount_in,
                )?;
                // withdraw amm_pc_vault to destination pc
                Invokers::token_transfer_with_authority(
                    token_program_info.clone(),
                    amm_pc_vault_info.clone(),
                    user_destination_info.clone(),
                    amm_authority_info.clone(),
                    AUTHORITY_AMM,
                    amm.nonce as u8,
                    swap_amount_out,
                )?;
                // update state_data data
                amm.state_data.swap_coin_in_amount = amm
                    .state_data
                    .swap_coin_in_amount
                    .checked_add(swap.amount_in.into())
                    .unwrap();
                amm.state_data.swap_pc_out_amount = amm
                    .state_data
                    .swap_pc_out_amount
                    .checked_add(swap_amount_out.into())
                    .unwrap();
                // charge coin as swap fee
                amm.state_data.swap_acc_coin_fee = amm
                    .state_data
                    .swap_acc_coin_fee
                    .checked_add(swap_fee.as_u64())
                    .unwrap();
            }
            SwapDirection::PC2Coin => {
                //if swap_amount_out >= total_coin_without_take_pnl {
                //    return Err(AmmError::InsufficientFunds.into());
                //}

                if enable_orderbook {
                    // pc -> coin, need cancel sell order
                    if !asks.is_empty() {
                        let mut amm_order_ids_vec = Vec::new();
                        let mut order_ids = [0u64; 8];
                        let mut count = 0;
                        // fetch cancel order ids{
                        for order in asks.into_iter() {
                            order_ids[count] = order.client_order_id();
                            count += 1;
                            if count == 8 {
                                amm_order_ids_vec.push(order_ids);
                                order_ids = [0u64; 8];
                                count = 0;
                            }
                        }
                        if count != 0 {
                            amm_order_ids_vec.push(order_ids);
                        }
                        //for ids in amm_order_ids_vec.iter() {
                        //    Invokers::invoke_dex_cancel_orders_by_client_order_ids(
                        //        market_program_info.clone(),
                        //        market_info.clone(),
                        //        market_bids_info.clone(),
                        //        market_asks_info.clone(),
                        //        amm_open_orders_info.clone(),
                        //        amm_authority_info.clone(),
                        //        market_event_queue_info.clone(),
                        //        AUTHORITY_AMM,
                        //        amm.nonce as u8,
                        //        *ids,
                        //    )?;
                        //}
                    }

                    if swap_amount_out > amm_coin_vault.amount {
                        //Invokers::invoke_dex_settle_funds(
                        //    market_program_info.clone(),
                        //    market_info.clone(),
                        //    amm_open_orders_info.clone(),
                        //    amm_authority_info.clone(),
                        //    market_coin_vault_info.clone(),
                        //    market_pc_vault_info.clone(),
                        //    amm_coin_vault_info.clone(),
                        //    amm_pc_vault_info.clone(),
                        //    market_vault_signer.clone(),
                        //    token_program_info.clone(),
                        //    Some(&amm_pc_vault_info.clone()),
                        //    AUTHORITY_AMM,
                        //    amm.nonce as u8,
                        //)?;
                    }
                }
                // deposit source pc to amm_pc_vault
                Invokers::token_transfer(
                    token_program_info.clone(),
                    user_source_info.clone(),
                    amm_pc_vault_info.clone(),
                    user_source_owner.clone(),
                    swap.amount_in,
                )?;
                // withdraw amm_coin_vault to destination coin
                Invokers::token_transfer_with_authority(
                    token_program_info.clone(),
                    amm_coin_vault_info.clone(),
                    user_destination_info.clone(),
                    amm_authority_info.clone(),
                    AUTHORITY_AMM,
                    amm.nonce as u8,
                    swap_amount_out,
                )?;
                // update state_data data
                amm.state_data.swap_pc_in_amount = amm
                    .state_data
                    .swap_pc_in_amount
                    .checked_add(swap.amount_in.into())
                    .unwrap();
                amm.state_data.swap_coin_out_amount = amm
                    .state_data
                    .swap_coin_out_amount
                    .checked_add(swap_amount_out.into())
                    .unwrap();
                // charge pc as swap fee
                amm.state_data.swap_acc_pc_fee = amm
                    .state_data
                    .swap_acc_pc_fee
                    .checked_add(swap_fee.as_u64())
                    .unwrap();
            }
        };
        amm.recent_epoch = Clock::get()?.epoch;

        Ok(())
    }
}

pub mod account_parser {
    use super::*;
    pub struct IdleArgs<'a> {
        pub program_id: &'a Pubkey,
        pub total_coin_without_take_pnl: &'a mut u64,
        pub total_pc_without_take_pnl: &'a mut u64,
        pub amm: &'a mut AmmInfo,
        pub bids: &'a Vec<LeafNode>,
        pub asks: &'a Vec<LeafNode>,
        pub target: &'a mut TargetOrders,
    }

    pub struct CancelAllOrdersArgs<'a, 'b: 'a> {
        pub program_id: &'a Pubkey,
        pub limit: u16,
        pub market_program_info: &'a AccountInfo<'b>,
        pub market_info: &'a AccountInfo<'b>,
        pub amm_open_orders_info: &'a AccountInfo<'b>,
        pub amm_authority_info: &'a AccountInfo<'b>,
        pub market_event_queue_info: &'a AccountInfo<'b>,
        pub market_coin_vault_info: &'a AccountInfo<'b>,
        pub market_pc_vault_info: &'a AccountInfo<'b>,
        pub market_bids_info: &'a AccountInfo<'b>,
        pub market_asks_info: &'a AccountInfo<'b>,
        pub market_vault_signer: &'a AccountInfo<'b>,
        pub token_program_info: &'a AccountInfo<'b>,
        pub amm_coin_vault_info: &'a AccountInfo<'b>,
        pub amm_pc_vault_info: &'a AccountInfo<'b>,

        pub referrer_pc_account: Option<&'a AccountInfo<'b>>,

        pub amm: &'a mut AmmInfo,
        pub open_orders: &'a OpenOrders,
        pub target: &'a mut TargetOrders,
        pub bids: &'a Vec<LeafNode>,
        pub asks: &'a Vec<LeafNode>,
    }
    pub struct PlanOrderBookArgs<'a> {
        pub program_id: &'a Pubkey,
        pub limit: u16,
        pub total_coin_without_take_pnl: u64,
        pub total_pc_without_take_pnl: u64,
        pub amm: &'a mut AmmInfo,
        pub target: &'a mut TargetOrders,
    }
    pub struct PlaceOrdersArgs<'a, 'b: 'a> {
        pub program_id: &'a Pubkey,
        pub limit: u16,

        pub amm_authority_info: &'a AccountInfo<'b>,
        pub amm_open_orders_info: &'a AccountInfo<'b>,
        pub market_program_info: &'a AccountInfo<'b>,
        pub market_info: &'a AccountInfo<'b>,
        pub market_request_queue_info: &'a AccountInfo<'b>,
        pub amm_coin_vault_info: &'a AccountInfo<'b>,
        pub amm_pc_vault_info: &'a AccountInfo<'b>,
        pub market_coin_vault_info: &'a AccountInfo<'b>,
        pub market_pc_vault_info: &'a AccountInfo<'b>,
        pub token_program_info: &'a AccountInfo<'b>,
        pub rent_info: &'a AccountInfo<'b>,

        pub market_event_queue_info: &'a AccountInfo<'b>,
        pub market_bids_info: &'a AccountInfo<'b>,
        pub market_asks_info: &'a AccountInfo<'b>,

        pub srm_token_account: Option<&'a AccountInfo<'b>>,

        pub amm: &'a mut AmmInfo,
        pub open_orders: &'a OpenOrders,
        pub bids: &'a Vec<LeafNode>,
        pub asks: &'a Vec<LeafNode>,
        pub target: &'a mut TargetOrders,

        pub total_coin_without_take_pnl: u64,
        pub total_pc_without_take_pnl: u64,
        pub coin_vault_amount: u64,
        pub pc_vault_amount: u64,
    }
    pub struct PurgeOrderArgs<'a, 'b: 'a> {
        pub program_id: &'a Pubkey,
        pub limit: u16,
        pub market_program_info: &'a AccountInfo<'b>,
        pub market_info: &'a AccountInfo<'b>,
        pub amm_open_orders_info: &'a AccountInfo<'b>,
        pub amm_authority_info: &'a AccountInfo<'b>,
        pub market_event_queue_info: &'a AccountInfo<'b>,
        pub market_bids_info: &'a AccountInfo<'b>,
        pub market_asks_info: &'a AccountInfo<'b>,
        pub amm: &'a mut AmmInfo,
        pub target: &'a TargetOrders,
        pub bids: &'a Vec<LeafNode>,
        pub asks: &'a Vec<LeafNode>,
    }
    pub struct CancelOrderArgs<'a, 'b: 'a> {
        pub program_id: &'a Pubkey,
        pub limit: u16,
        pub market_program_info: &'a AccountInfo<'b>,
        pub market_info: &'a AccountInfo<'b>,
        pub amm_open_orders_info: &'a AccountInfo<'b>,
        pub amm_authority_info: &'a AccountInfo<'b>,
        pub market_event_queue_info: &'a AccountInfo<'b>,
        pub market_bids_info: &'a AccountInfo<'b>,
        pub market_asks_info: &'a AccountInfo<'b>,
        pub amm: &'a mut AmmInfo,
        pub open_orders: &'a OpenOrders,
        pub target: &'a mut TargetOrders,
        pub bids: &'a Vec<LeafNode>,
        pub asks: &'a Vec<LeafNode>,

        pub coin_amount: u64,
        pub pc_amount: u64,
    }
}
