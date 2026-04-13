// Core types (at root level)
pub mod events;
pub mod executor;
pub mod lifecycle;
pub mod order;

// Venue-specific implementations
pub mod backtest;
pub mod paper;
pub mod polymarket;
pub mod solana;

// Re-export core types for backward compatibility
pub use events::{
    ExecutionEvent, LimitOrderEvent, OrderSide, RedemptionEvent, SimulationMode, TimeInForce,
};
pub use executor::{NoopOrderExecutor, OrderExecutor, fill_event_to_execution_event};
pub use lifecycle::{
    LifecycleCancelConfirmed, LifecycleCancelRequest, LifecycleCancelRequestOutcome,
    LifecycleEngine, LifecycleError, LifecycleFillEvidence, LifecycleMetrics, LifecycleOrder,
    LifecycleOrderRef, LifecyclePlaceRejected, LifecyclePlaceRequest, LifecyclePlaceSuccess,
    LifecycleState, LifecycleTerminalEvidence, TerminalReason, UnknownOrderCancelPolicy,
};
pub use order::{Order, OrderStatus, OrderType};

// Re-export backtest executor
pub use backtest::BacktestOrderExecutor;

// Re-export polymarket executor
pub use polymarket::{
    PendingOrder as PolymarketPendingOrder, PolymarketConfig, PolymarketOrderExecutor,
    PolymarketOrderExecutorBuilder,
};

// Re-export paper trading executor and metrics
pub use paper::{PaperTradingMetrics, PaperTradingOrderExecutor};

// Re-export all Solana types for backward compatibility
pub use solana::{
    BloxrouteTransactionSubmitter, GrpcConfirmationMonitor, JitoMetrics, JitoTransactionSubmitter,
    LocalKeypairSigningService, RemoteSigningService, SigningService, SolanaOrderExecutor,
    SolanaOrderExecutorBuilder, SolanaTransactionSubmitter, TransactionService,
    TransactionSubmitter, ZeroSlotTransactionSubmitter,
};

// Re-export tx_constructor types
pub use solana::tx_constructor::{
    BonkRequestBuilder, DexRequestBuilder, DexType, PumpFunRequestBuilder, ShadowTxnMakerClient,
    TcpTxnMakerClient, TransactionConstructor, TxnMakerClient, TxnMakerTransactionConstructor,
    UnixSocketTxnMakerClient,
};

// Re-export utility types
pub use solana::utils::{
    TokenTransactionError, TransactionDetails, extract_token_transaction_details,
};

// Re-export redis utilities
pub use solana::utils::extract_token_transaction_details_from_redis;

// Re-export submission registry
pub use solana::submission::registry as submitter_registry;

#[cfg(test)]
pub use solana::MockTransactionSubmitter;
