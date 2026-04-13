use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::keypair::read_keypair_file};
use std::{collections::HashSet, fs, path::Path, str::FromStr};

use crate::execution::{SimulationMode, UnknownOrderCancelPolicy};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub redis: RedisConfig,
    pub solana: SolanaConfig,
    pub wallet: WalletConfig,
    pub position: PositionConfig,
    pub execution: ExecutionConfig,
    pub notifier: Option<NotifierConfig>,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    /// API server bind address
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
    /// Metrics server port
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
    /// Maximum event latency in milliseconds before dropping stale market data events
    #[serde(default = "default_max_latency_ms")]
    pub max_latency_ms: i64,
}

/// Redis subscriber type configuration for choosing between List, Stream, or Pubsub patterns.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum RedisSubscriberType {
    /// Redis LIST-based subscriber using BRPOP (default, current behavior)
    List {
        /// Timeout in seconds for BRPOP (0 = block forever)
        #[serde(default)]
        timeout_secs: Option<u64>,
    },
    /// Redis STREAM-based subscriber using XREADGROUP with consumer groups
    Stream {
        /// Consumer group name (required)
        consumer_group: String,
        /// Consumer name within the group (defaults to hostname-pid)
        #[serde(default)]
        consumer_name: Option<String>,
        /// Block timeout in milliseconds (default: 5000)
        #[serde(default)]
        block_ms: Option<u64>,
        /// Maximum number of entries to read at once (default: 10)
        #[serde(default)]
        count: Option<usize>,
    },
    /// Redis PUBSUB-based subscriber for real-time broadcast
    Pubsub {
        /// Channels to subscribe to
        channels: Vec<String>,
    },
}

impl Default for RedisSubscriberType {
    fn default() -> Self {
        Self::List { timeout_secs: None }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedisConfig {
    /// Redis connection URL
    pub url: String,
    /// Token events queue key (used as queue name for List, stream name for Stream)
    #[serde(default = "default_token_events_key")]
    pub token_events_key: String,
    /// Signal persistence queue key (optional)
    pub signal_persistence_key: Option<String>,
    /// Subscriber type configuration (defaults to List mode for backward compatibility)
    #[serde(default)]
    pub subscriber: RedisSubscriberType,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SolanaConfig {
    /// Solana RPC URL
    pub rpc_url: String,
    /// Optional websocket URL for subscriptions
    pub ws_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WalletConfig {
    /// Path to keypair file
    pub keypair_path: String,
}

impl WalletConfig {
    pub fn load_keypair(&self) -> Result<Keypair> {
        read_keypair_file(&self.keypair_path).map_err(|e| {
            anyhow::anyhow!("Failed to read keypair from {}: {}", self.keypair_path, e)
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PositionConfig {
    /// Amount of SOL to use for each trade
    pub trade_amount_sol: f64,
    /// Initial SOL balance for tracking
    pub initial_sol: f64,
    /// Maximum holding period in seconds
    pub max_holding_period_secs: i64,
    /// Maximum number of open positions
    pub max_open_positions: u32,
    /// Exit strategy configuration
    pub exit_strategy: ExitStrategyConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitStrategyType {
    Configurable,
    Noop,
}

impl Default for ExitStrategyType {
    fn default() -> Self {
        Self::Configurable
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExitStrategyConfig {
    /// Strategy type: configurable or noop
    #[serde(default)]
    pub strategy_type: ExitStrategyType,
    /// Take profit percentage (only used when strategy_type = configurable)
    pub take_profit_pct: Option<f64>,
    /// Stop loss percentage (only used when strategy_type = configurable)
    pub stop_loss_pct: Option<f64>,
    /// Maximum time to hold position in seconds (only used when strategy_type = configurable)
    pub max_hold_time_secs: Option<i64>,
    /// Maximum number of sell failures before force exit
    #[serde(default = "default_max_sell_failures")]
    pub max_sell_failures: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutionConfig {
    /// Execution mode: live, paper_trading, or backtest
    /// Default: Live
    #[serde(default)]
    pub execution_mode: ExecutionModeConfig,
    /// Simulation mode: pure, rpc_based, or disabled
    #[serde(default)]
    pub simulation_mode: SimulationModeConfig,
    /// Buy slippage (e.g., 0.05 for 5%)
    #[serde(default = "default_slippage")]
    pub buy_slippage: f64,
    /// Sell slippage
    #[serde(default = "default_slippage")]
    pub sell_slippage: f64,
    /// Simulated buy slippage (for simulation mode)
    #[serde(default = "default_sim_slippage")]
    pub sim_buy_slippage: f64,
    /// Simulated sell slippage (for simulation mode)
    #[serde(default = "default_sim_slippage")]
    pub sim_sell_slippage: f64,
    /// Skip simulation for buy orders
    #[serde(default)]
    pub skip_simulation_for_buy: bool,
    /// Skip simulation for sell orders
    #[serde(default)]
    pub skip_simulation_when_sell: bool,
    /// Maximum number of real trades (for safety)
    pub max_real_trades: Option<u32>,
    /// Compute unit price in micro-lamports
    #[serde(default = "default_compute_unit_price")]
    pub compute_unit_price: u64,
    /// Compute unit limit
    #[serde(default = "default_compute_unit_limit")]
    pub compute_unit_limit: u32,
    /// Transaction maker service configuration
    pub txn_maker: TxnMakerConfig,
    /// Transaction submitter configuration
    pub submitter: SubmitterConfig,
    /// Leader monitor configuration (optional)
    pub leader_monitor: Option<LeaderMonitorConfig>,
    /// Shared order lifecycle behavior configuration.
    #[serde(default)]
    pub lifecycle: LifecycleConfig,
}

/// Shared lifecycle engine behavior knobs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LifecycleConfig {
    /// Maximum wait for cancellation confirmation evidence before timeout policy applies.
    #[serde(default = "default_cancel_confirmation_timeout_ms")]
    pub cancel_confirmation_timeout_ms: u64,
    /// Emit optional non-terminal CancelRequested events for observability.
    #[serde(default)]
    pub emit_cancel_requested_events: bool,
    /// Policy used when cancel is requested for an unknown order ID.
    #[serde(default)]
    pub unknown_order_cancel_policy: UnknownOrderCancelPolicy,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            cancel_confirmation_timeout_ms: default_cancel_confirmation_timeout_ms(),
            emit_cancel_requested_events: false,
            unknown_order_cancel_policy: UnknownOrderCancelPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SimulationModeConfig {
    Pure,
    RpcBased,
    #[default]
    Disabled,
}

impl From<SimulationModeConfig> for SimulationMode {
    fn from(config: SimulationModeConfig) -> Self {
        match config {
            SimulationModeConfig::Pure => SimulationMode::Pure,
            SimulationModeConfig::RpcBased => SimulationMode::RpcBased,
            SimulationModeConfig::Disabled => SimulationMode::Disabled,
        }
    }
}

/// Execution mode for the trading system.
///
/// Determines how orders are executed:
/// - `Live`: Real execution on venues (default)
/// - `PaperTrading`: Simulated execution against live market data
/// - `Backtest`: Historical data replay with simulated execution
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionModeConfig {
    /// Real order execution on venues
    #[default]
    Live,
    /// Paper trading with live market data
    PaperTrading(PaperTradingConfig),
    /// Backtest mode (typically configured separately)
    Backtest,
}

/// Configuration for paper trading mode.
///
/// Paper trading runs against live market data but simulates order fills
/// instead of executing real trades. This allows strategy validation
/// without risking capital.
///
/// Note: Paper trading is designed for orderbook/CLOB venues where limit orders
/// are placed at specific prices. Orders are filled at their limit price when
/// trades cross that price level - there is no slippage model since limit orders
/// guarantee the execution price.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PaperTradingConfig {
    /// Whether to enforce inventory constraints on sell orders.
    /// When true, sell orders can only fill if there's positive inventory.
    /// Default: true (no naked shorting)
    #[serde(default = "default_true")]
    pub enforce_inventory: bool,

    /// Event persistence configuration (optional).
    /// When enabled, trading events are published to Redis for downstream consumption.
    #[serde(default)]
    pub event_persistence: Option<EventPersistenceConfig>,

    /// Latency simulation configuration (optional).
    /// When enabled, orders are not eligible for fills until after simulated placement latency,
    /// and cancellations take effect after simulated cancel latency.
    #[serde(default)]
    pub latency: Option<LatencySimulationConfig>,
}

impl Default for PaperTradingConfig {
    fn default() -> Self {
        Self {
            enforce_inventory: true,
            event_persistence: None,
            latency: None,
        }
    }
}

/// Configuration for event persistence to Redis.
///
/// Events are published to Redis in real-time during paper trading,
/// allowing downstream systems to consume and analyze trading activity.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventPersistenceConfig {
    /// Redis URL for publishing events
    pub redis_url: String,

    /// Publishing mode: list, stream, or pubsub
    #[serde(default)]
    pub mode: EventPersistenceMode,

    /// Key/channel name for publishing.
    /// Default: "paper_trading_events" (a fixed, deterministic queue name)
    #[serde(default = "default_event_key")]
    pub key: String,

    /// Maximum length for list/stream modes (trimming).
    /// Default: 100000
    #[serde(default = "default_max_length")]
    pub max_length: usize,

    /// Event filtering configuration
    #[serde(default)]
    pub filter: EventFilterConfig,

    /// Channel buffer size for async publishing.
    /// Events are dropped if buffer is full.
    /// Default: 10000
    #[serde(default = "default_channel_buffer_size")]
    pub channel_buffer_size: usize,

    /// Optional explicit session ID for event correlation.
    /// If not set, a unique session ID is auto-generated.
    /// The session_id is included in each event for downstream consumers
    /// to differentiate events from different trading sessions.
    #[serde(default)]
    pub session_id: Option<String>,
}

impl Default for EventPersistenceConfig {
    fn default() -> Self {
        Self {
            redis_url: "redis://localhost:6379".to_string(),
            mode: EventPersistenceMode::default(),
            key: default_event_key(),
            max_length: default_max_length(),
            filter: EventFilterConfig::default(),
            channel_buffer_size: default_channel_buffer_size(),
            session_id: None,
        }
    }
}

/// Publishing mode for event persistence.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EventPersistenceMode {
    /// Redis LIST-based publishing (LPUSH with LTRIM)
    #[default]
    List,
    /// Redis STREAM-based publishing (XADD with MAXLEN)
    Stream,
    /// Redis PUB/SUB-based publishing (PUBLISH)
    Pubsub,
}

/// Event filter configuration for persistence.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventFilterConfig {
    /// Exclude orderbook snapshot events (large, ~30KB each)
    #[serde(default = "default_true")]
    pub exclude_orderbook_snapshots: bool,

    /// Exclude orderbook update events
    #[serde(default = "default_true")]
    pub exclude_orderbook_updates: bool,

    /// Exclude Polymarket trade events (high volume)
    #[serde(default = "default_true")]
    pub exclude_polymarket_trades: bool,

    /// Exclude timer events
    #[serde(default = "default_true")]
    pub exclude_timer_events: bool,

    /// Exclude token events (Solana token trades)
    #[serde(default = "default_true")]
    pub exclude_token_events: bool,

    /// Exclude spot price events
    #[serde(default = "default_true")]
    pub exclude_spot_price_events: bool,
}

impl Default for EventFilterConfig {
    fn default() -> Self {
        Self {
            exclude_orderbook_snapshots: true,
            exclude_orderbook_updates: true,
            exclude_polymarket_trades: true,
            exclude_timer_events: true,
            exclude_token_events: true,
            exclude_spot_price_events: true,
        }
    }
}

/// Latency simulation configuration for realistic fill modeling.
///
/// In production, order management has real placement and cancellation latency.
/// This configuration allows backtests and paper trading to simulate these latencies,
/// preventing overly optimistic fill assumptions.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LatencySimulationConfig {
    /// Minimum order placement latency in milliseconds.
    /// Default: 150
    #[serde(default = "default_min_place_latency_ms")]
    pub min_place_latency_ms: u64,

    /// Maximum order placement latency in milliseconds.
    /// Default: 500
    #[serde(default = "default_max_place_latency_ms")]
    pub max_place_latency_ms: u64,

    /// Optional RNG seed for deterministic latency sampling.
    ///
    /// When set, backtests/paper-trading runs become reproducible across executions by
    /// sampling placement/cancellation latency from a deterministic RNG stream.
    #[serde(default)]
    pub seed: Option<u64>,

    /// Minimum order cancellation latency in milliseconds.
    /// Defaults to placement latency if not specified.
    #[serde(default)]
    pub min_cancel_latency_ms: Option<u64>,

    /// Maximum order cancellation latency in milliseconds.
    /// Defaults to placement latency if not specified.
    #[serde(default)]
    pub max_cancel_latency_ms: Option<u64>,
    /// Minimum quote-update debounce window in milliseconds.
    /// While this window is active, transient intent quote updates are held in
    /// reconciliation state instead of immediately emitting cancel/replace orders.
    /// Cancel-all intents still bypass debounce and execute immediately.
    /// Default: None (no quote-update debouncing)
    #[serde(default)]
    pub min_quote_lifetime_ms: Option<u64>,
}

impl Default for LatencySimulationConfig {
    fn default() -> Self {
        Self {
            min_place_latency_ms: default_min_place_latency_ms(),
            max_place_latency_ms: default_max_place_latency_ms(),
            seed: None,
            min_cancel_latency_ms: None,
            max_cancel_latency_ms: None,
            min_quote_lifetime_ms: None,
        }
    }
}

fn default_min_place_latency_ms() -> u64 {
    150
}

fn default_max_place_latency_ms() -> u64 {
    500
}

fn default_cancel_confirmation_timeout_ms() -> u64 {
    5_000
}

fn default_event_key() -> String {
    "paper_trading_events".to_string()
}

fn default_max_length() -> usize {
    100_000
}

fn default_channel_buffer_size() -> usize {
    10_000
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TxnMakerConfig {
    /// Transaction maker service URL
    pub url: String,
    /// Connection type: tcp, unix, or shadow
    #[serde(default = "default_connection_type")]
    pub connection_type: String,
    /// Unix socket path (if using unix connection)
    pub unix_socket_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubmitterConfig {
    /// Submitter type: solana, jito, bloxroute, or zeroslot
    #[serde(default = "default_submitter_type")]
    pub submitter_type: String,
    /// Maximum slot latency for leader check
    pub max_slot_latency: Option<u64>,
    /// Confirmation timeout in seconds
    #[serde(default = "default_confirmation_timeout_secs")]
    pub confirmation_timeout_secs: u64,
    /// Jito configuration (if using jito submitter)
    pub jito: Option<JitoConfig>,
    /// BloxRoute configuration (if using bloxroute submitter)
    pub bloxroute: Option<BloxRouteConfig>,
    /// ZeroSlot configuration (if using zeroslot submitter)
    pub zeroslot: Option<ZeroSlotConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JitoConfig {
    /// Jito block engine URL
    pub block_engine_url: String,
    /// Jito API key
    pub api_key: String,
    /// Tip amount for buy orders (in lamports)
    pub buy_tip: Option<u64>,
    /// Tip amount for sell orders (in lamports)
    pub sell_tip: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BloxRouteConfig {
    /// BloxRoute API URL
    pub api_url: String,
    /// BloxRoute auth header
    pub auth_header: String,
    /// Tip amount (in lamports)
    pub tip: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ZeroSlotConfig {
    /// ZeroSlot API URL
    pub api_url: String,
    /// ZeroSlot API key
    pub api_key: String,
    /// Tip amount (in lamports)
    pub tip: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LeaderMonitorConfig {
    /// Enable leader monitoring
    #[serde(default)]
    pub enabled: bool,
    /// Bad validator public keys (blacklist)
    pub bad_validators: Vec<String>,
    /// Refresh interval in seconds
    #[serde(default = "default_leader_refresh_interval")]
    pub refresh_interval_secs: u64,
}

impl LeaderMonitorConfig {
    pub fn parse_validators(&self) -> Result<HashSet<Pubkey>> {
        self.bad_validators
            .iter()
            .map(|s| {
                Pubkey::from_str(s).with_context(|| format!("Invalid validator pubkey: {}", s))
            })
            .collect()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotifierConfig {
    /// Telegram bot configuration
    pub telegram: Option<TelegramConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelegramConfig {
    /// Telegram bot token
    pub bot_token: String,
    /// Chat ID to send notifications to
    pub chat_id: i64,
    /// Enable position notifications
    #[serde(default = "default_true")]
    pub notify_positions: bool,
    /// Enable signal notifications
    #[serde(default)]
    pub notify_signals: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    /// Log level: trace, debug, info, warn, error
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Log format: text or json
    #[serde(default = "default_log_format")]
    pub format: String,
}

// Default values
fn default_bind_address() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_metrics_port() -> u16 {
    9090
}

fn default_max_latency_ms() -> i64 {
    30
}

fn default_token_events_key() -> String {
    "token_events".to_string()
}

fn default_slippage() -> f64 {
    0.05 // 5%
}

fn default_sim_slippage() -> f64 {
    0.02 // 2%
}

fn default_compute_unit_price() -> u64 {
    100_000 // 100k micro-lamports
}

fn default_compute_unit_limit() -> u32 {
    200_000
}

fn default_connection_type() -> String {
    "tcp".to_string()
}

fn default_submitter_type() -> String {
    "solana".to_string()
}

fn default_confirmation_timeout_secs() -> u64 {
    60
}

fn default_leader_refresh_interval() -> u64 {
    60
}

fn default_max_sell_failures() -> u32 {
    3
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "text".to_string()
}

fn default_true() -> bool {
    true
}

impl Config {
    /// Load configuration from YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config file: {:?}", path.as_ref()))?;

        let config: Config =
            serde_yaml::from_str(&content).context("Failed to parse config file")?;

        config.validate()?;

        Ok(config)
    }

    /// Validate configuration
    fn validate(&self) -> Result<()> {
        // Validate position config
        if self.position.trade_amount_sol <= 0.0 {
            anyhow::bail!("trade_amount_sol must be positive");
        }

        if self.position.initial_sol < self.position.trade_amount_sol {
            anyhow::bail!("initial_sol must be >= trade_amount_sol");
        }

        if self.position.max_open_positions == 0 {
            anyhow::bail!("max_open_positions must be > 0");
        }

        // Validate slippage
        if self.execution.buy_slippage < 0.0 || self.execution.buy_slippage > 1.0 {
            anyhow::bail!("buy_slippage must be between 0 and 1");
        }

        if self.execution.sell_slippage < 0.0 || self.execution.sell_slippage > 1.0 {
            anyhow::bail!("sell_slippage must be between 0 and 1");
        }

        if self.execution.lifecycle.cancel_confirmation_timeout_ms == 0 {
            anyhow::bail!("cancel_confirmation_timeout_ms must be > 0");
        }

        // Validate leader monitor validators if enabled
        if let Some(leader_monitor) = &self.execution.leader_monitor {
            if leader_monitor.enabled {
                leader_monitor.parse_validators()?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        let mut config = Config {
            server: ServerConfig {
                bind_address: "0.0.0.0:8080".to_string(),
                metrics_port: 9090,
                max_latency_ms: 30,
            },
            redis: RedisConfig {
                url: "redis://localhost:6379".to_string(),
                token_events_key: "token_events".to_string(),
                signal_persistence_key: None,
                subscriber: RedisSubscriberType::default(),
            },
            solana: SolanaConfig {
                rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
                ws_url: None,
            },
            wallet: WalletConfig {
                keypair_path: "/path/to/keypair.json".to_string(),
            },
            position: PositionConfig {
                trade_amount_sol: 0.1,
                initial_sol: 1.0,
                max_holding_period_secs: 3600,
                max_open_positions: 10,
                exit_strategy: ExitStrategyConfig {
                    strategy_type: ExitStrategyType::Configurable,
                    take_profit_pct: Some(0.2),
                    stop_loss_pct: Some(-0.1),
                    max_hold_time_secs: Some(3600),
                    max_sell_failures: 3,
                },
            },
            execution: ExecutionConfig {
                execution_mode: ExecutionModeConfig::Live,
                simulation_mode: SimulationModeConfig::Pure,
                buy_slippage: 0.05,
                sell_slippage: 0.05,
                sim_buy_slippage: 0.02,
                sim_sell_slippage: 0.02,
                skip_simulation_for_buy: false,
                skip_simulation_when_sell: false,
                max_real_trades: Some(100),
                compute_unit_price: 100_000,
                compute_unit_limit: 200_000,
                txn_maker: TxnMakerConfig {
                    url: "http://localhost:8081".to_string(),
                    connection_type: "tcp".to_string(),
                    unix_socket_path: None,
                },
                submitter: SubmitterConfig {
                    submitter_type: "solana".to_string(),
                    max_slot_latency: Some(10),
                    confirmation_timeout_secs: 60,
                    jito: None,
                    bloxroute: None,
                    zeroslot: None,
                },
                leader_monitor: None,
                lifecycle: LifecycleConfig::default(),
            },
            notifier: None,
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "text".to_string(),
            },
        };

        assert!(config.validate().is_ok());

        // Test invalid trade amount
        config.position.trade_amount_sol = -0.1;
        assert!(config.validate().is_err());

        config.position.trade_amount_sol = 0.1;

        // Test invalid slippage
        config.execution.buy_slippage = 1.5;
        assert!(config.validate().is_err());

        config.execution.buy_slippage = 0.05;

        // Test invalid lifecycle timeout
        config.execution.lifecycle.cancel_confirmation_timeout_ms = 0;
        assert!(config.validate().is_err());
    }
}
