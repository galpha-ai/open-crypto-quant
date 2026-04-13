#!/usr/bin/env node

/**
 * Generate TypeScript index file with convenient exports
 */

const fs = require('fs');
const path = require('path');

const indexContent = `// Auto-generated index file for @galpha/proto
// This file provides convenient access to all protobuf types

import * as proto from './proto';

// Re-export all namespaces
export { proto };
export import ledger = proto.ledger;
export import position_manager = proto.position_manager;
export import user_service = proto.user_service;

// Type aliases for commonly used types
export type GetBalanceRequest = proto.ledger.v1.GetBalanceRequest;
export type GetBalanceResponse = proto.ledger.v1.GetBalanceResponse;
export type StakeRequest = proto.ledger.v1.StakeRequest;
export type StakeResponse = proto.ledger.v1.StakeResponse;
export type CreateUnstakingRequest = proto.ledger.v1.CreateUnstakingRequest;
export type CreateUnstakingResponse = proto.ledger.v1.CreateUnstakingResponse;
export type CreateWithdrawalRequest = proto.ledger.v1.CreateWithdrawalRequest;
export type CreateWithdrawalResponse = proto.ledger.v1.CreateWithdrawalResponse;
export type Transaction = proto.ledger.v1.Transaction;
export type GetUserTransactionsRequest = proto.ledger.v1.GetUserTransactionsRequest;
export type GetUserTransactionsResponse = proto.ledger.v1.GetUserTransactionsResponse;
export type PaginationInfo = proto.ledger.v1.PaginationInfo;
export type CreatePendingDepositRequest = proto.ledger.v1.CreatePendingDepositRequest;
export type CreatePendingDepositResponse = proto.ledger.v1.CreatePendingDepositResponse;
export type PendingDeposit = proto.ledger.v1.PendingDeposit;
export type GetPendingDepositsRequest = proto.ledger.v1.GetPendingDepositsRequest;
export type GetPendingDepositsResponse = proto.ledger.v1.GetPendingDepositsResponse;
export type GetQuoteRequest = proto.ledger.v1.GetQuoteRequest;
export type GetQuoteResponse = proto.ledger.v1.GetQuoteResponse;
export type GetTvlRequest = proto.ledger.v1.GetTvlRequest;
export type GetTvlResponse = proto.ledger.v1.GetTvlResponse;
export type ErrorResponse = proto.ledger.v1.ErrorResponse;

export type GetPositionsRequest = proto.position_manager.v1.GetPositionsRequest;
export type GetPositionsResponse = proto.position_manager.v1.GetPositionsResponse;
export type Position = proto.position_manager.v1.Position;

export type GetCustodialAddressResponse = proto.user_service.v1.GetCustodialAddressResponse;
export type CustodialAddress = proto.user_service.v1.CustodialAddress;
export type TradingAccount = proto.user_service.v1.TradingAccount;
export type RefreshTradingAccountsRequest = proto.user_service.v1.RefreshTradingAccountsRequest;
export type RefreshTradingAccountsResponse = proto.user_service.v1.RefreshTradingAccountsResponse;
export type UserInfo = proto.user_service.v1.UserInfo;

// Utility functions for JSON serialization/deserialization
export const util = {
  /**
   * Encode message to JSON
   */
  toJSON<T>(message: T): any {
    if (!message || typeof (message as any).toJSON !== 'function') {
      throw new Error('Message does not have toJSON method');
    }
    return (message as any).toJSON();
  },

  /**
   * Decode message from JSON
   */
  fromJSON<T>(MessageType: any, json: any): T {
    if (!MessageType || typeof MessageType.fromObject !== 'function') {
      throw new Error('MessageType does not have fromObject method');
    }
    return MessageType.fromObject(json);
  },

  /**
   * Encode message to buffer
   */
  encode<T>(message: T): Uint8Array {
    if (!message || typeof (message as any).constructor?.encode !== 'function') {
      throw new Error('Message type does not have encode method');
    }
    const MessageType = (message as any).constructor;
    return MessageType.encode(message).finish();
  },

  /**
   * Decode message from buffer
   */
  decode<T>(MessageType: any, buffer: Uint8Array): T {
    if (!MessageType || typeof MessageType.decode !== 'function') {
      throw new Error('MessageType does not have decode method');
    }
    return MessageType.decode(buffer);
  }
};
`;

const outputPath = path.join(__dirname, '..', 'dist', 'index.ts');
fs.writeFileSync(outputPath, indexContent);
console.log('Generated index.ts');
