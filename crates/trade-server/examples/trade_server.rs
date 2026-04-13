use anyhow::{Context, Result};
use clap::Parser;
use prometheus::Registry;
use std::sync::Arc;
use tracing::{info, warn};
use trade_server::{
    config::{Config, ExecutionModeConfig, ExitStrategyType},
    event_coordinator::create_event_coordinator_from_config,
    execution::SolanaOrderExecutorBuilder,
    position::{ConfigurableExitStrategy, ExitStrategy, InMemoryPositionManager, NoopExitStrategy},
    trade_server::TradeServer,
};

#[derive(Parser, Debug)]
#[command(name = "trade-server")]
#[command(about = "Solana automated trading server", long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config/config.yaml")]
    config: String,
}

fn setup_logging(config: &Config) {
    let level = config.logging.level.clone();

    match config.logging.format.as_str() {
        "json" => {
            let subscriber = tracing_subscriber::fmt()
                .json()
                .with_env_filter(tracing_subscriber::EnvFilter::new(level))
                .finish();

            tracing::subscriber::set_global_default(subscriber)
                .expect("Failed to set tracing subscriber");
        }
        _ => {
            let subscriber = tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::new(level))
                .finish();

            tracing::subscriber::set_global_default(subscriber)
                .expect("Failed to set tracing subscriber");
        }
    };
}

async fn setup_metrics_server(port: u16, registry: Arc<Registry>) -> Result<()> {
    use warp::Filter;

    let metrics_route = warp::path("metrics").map(move || {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        warp::reply::with_header(buffer, "content-type", encoder.format_type())
    });

    info!("Starting metrics server on 0.0.0.0:{}", port);
    tokio::spawn(async move {
        warp::serve(metrics_route).run(([0, 0, 0, 0], port)).await;
    });

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration
    info!("Loading configuration from: {}", args.config);
    let config = Config::from_file(&args.config)
        .with_context(|| format!("Failed to load config from {}", args.config))?;

    // Setup logging
    setup_logging(&config);

    info!("Starting Trade Server...");
    info!("Configuration loaded successfully");
    info!("Simulation mode: {:?}", config.execution.simulation_mode);
    info!("Max open positions: {}", config.position.max_open_positions);
    info!("Trade amount: {} SOL", config.position.trade_amount_sol);

    // Create Prometheus registry
    let registry = Arc::new(Registry::new());

    // Setup metrics server
    setup_metrics_server(config.server.metrics_port, registry.clone()).await?;

    // Load wallet keypair
    info!("Loading wallet from: {}", config.wallet.keypair_path);
    let keypair = config
        .wallet
        .load_keypair()
        .context("Failed to load wallet keypair")?;
    use solana_sdk::signature::Signer;
    info!("Wallet address: {}", keypair.pubkey());

    // Create event coordinator from config (supports List, Stream, and Pubsub modes)
    info!("Connecting to Redis: {}", config.redis.url);
    info!("Redis subscriber mode: {:?}", config.redis.subscriber);
    let event_coordinator = Arc::new(
        create_event_coordinator_from_config(
            &config.redis,
            chrono::Duration::seconds(60), // Timer event every 60 seconds
        )
        .await
        .context("Failed to create event coordinator")?,
    );

    // Create exit strategy based on configuration
    let exit_strategy: Arc<dyn ExitStrategy> = match config.position.exit_strategy.strategy_type {
        ExitStrategyType::Noop => {
            info!("Using NoopExitStrategy - positions will NEVER auto-exit");
            info!(
                "Exit strategy: Max sell failures {}",
                config.position.exit_strategy.max_sell_failures
            );
            Arc::new(NoopExitStrategy::new())
        }
        ExitStrategyType::Configurable => {
            let take_profit = config.position.exit_strategy.take_profit_pct.unwrap_or(0.2);
            let stop_loss = config
                .position
                .exit_strategy
                .stop_loss_pct
                .unwrap_or(-0.1)
                .abs();
            let max_hold_secs = config
                .position
                .exit_strategy
                .max_hold_time_secs
                .unwrap_or(config.position.max_holding_period_secs);

            info!("Using ConfigurableExitStrategy");
            info!("Exit strategy: Take profit at {}%", take_profit * 100.0);
            info!("Exit strategy: Stop loss at {}%", stop_loss * 100.0);
            info!("Exit strategy: Max hold time {} seconds", max_hold_secs);
            info!(
                "Exit strategy: Max sell failures {}",
                config.position.exit_strategy.max_sell_failures
            );

            Arc::new(ConfigurableExitStrategy::new(
                take_profit,
                stop_loss,
                chrono::Duration::seconds(max_hold_secs),
                config.position.exit_strategy.max_sell_failures,
            ))
        }
    };

    let min_quote_lifetime_ms = match &config.execution.execution_mode {
        ExecutionModeConfig::PaperTrading(paper_config) => paper_config
            .latency
            .as_ref()
            .and_then(|latency| latency.min_quote_lifetime_ms),
        _ => None,
    };

    // Create position manager (paper mode can wire quote-lifetime debounce)
    let position_manager = Arc::new(InMemoryPositionManager::new_with_quote_lifetime(
        config.position.trade_amount_sol,
        config.position.initial_sol,
        chrono::Duration::seconds(config.position.max_holding_period_secs),
        config.position.max_open_positions,
        min_quote_lifetime_ms,
        &registry,
        exit_strategy.clone(),
    ));

    // Create order executor builder
    let mut executor_builder =
        SolanaOrderExecutorBuilder::new(keypair, config.solana.rpc_url.clone())
            .with_slippage(
                config.execution.buy_slippage,
                config.execution.sell_slippage,
            )
            .with_simulation_slippage(
                config.execution.sim_buy_slippage,
                config.execution.sim_sell_slippage,
            )
            .with_simulation_mode(config.execution.simulation_mode.clone().into())
            .with_skip_simulation_for_buy(config.execution.skip_simulation_for_buy)
            .skip_simulation_when_sell(config.execution.skip_simulation_when_sell)
            .with_compute_unit_price(config.execution.compute_unit_price)
            .with_compute_unit_limit(config.execution.compute_unit_limit);

    // Add max real trades limit if configured
    if let Some(max_trades) = config.execution.max_real_trades {
        if matches!(
            config.execution.simulation_mode,
            trade_server::config::SimulationModeConfig::Disabled
        ) {
            warn!("Max real trades limit enabled: {}", max_trades);
            executor_builder = executor_builder.with_max_real_trades(max_trades);
        }
    }

    // Configure transaction maker
    executor_builder = executor_builder.with_txn_maker_config(
        config.execution.txn_maker.connection_type.clone(),
        Some(config.execution.txn_maker.url.clone()),
        config.execution.txn_maker.unix_socket_path.clone(),
    );

    // Configure transaction submitter
    executor_builder = executor_builder
        .with_transaction_submitter_type(Some(config.execution.submitter.submitter_type.clone()));

    match config.execution.submitter.submitter_type.as_str() {
        "jito" => {
            if let Some(jito_config) = &config.execution.submitter.jito {
                info!("Using Jito submitter");
                executor_builder = executor_builder.with_jito_config(
                    Some(jito_config.block_engine_url.clone()),
                    Some(jito_config.api_key.clone()),
                );
                if let Some(buy_tip) = jito_config.buy_tip {
                    executor_builder = executor_builder.with_jito_buy_tip(buy_tip);
                }
                if let Some(sell_tip) = jito_config.sell_tip {
                    executor_builder = executor_builder.with_jito_sell_tip(sell_tip);
                }
            } else {
                anyhow::bail!("Jito config required for jito submitter type");
            }
        }
        "bloxroute" => {
            if let Some(bloxroute_config) = &config.execution.submitter.bloxroute {
                info!("Using BloxRoute submitter");
                executor_builder = executor_builder.with_bloxroute_config(
                    Some(bloxroute_config.api_url.clone()),
                    Some(bloxroute_config.auth_header.clone()),
                );
                if let Some(tip) = bloxroute_config.tip {
                    executor_builder = executor_builder.with_bloxroute_tip(tip);
                }
            } else {
                anyhow::bail!("BloxRoute config required for bloxroute submitter type");
            }
        }
        "zeroslot" => {
            if let Some(zeroslot_config) = &config.execution.submitter.zeroslot {
                info!("Using ZeroSlot submitter");
                executor_builder = executor_builder.with_zeroslot_config(
                    Some(zeroslot_config.api_url.clone()),
                    Some(zeroslot_config.api_key.clone()),
                );
                if let Some(tip) = zeroslot_config.tip {
                    executor_builder = executor_builder.with_zeroslot_tip(tip);
                }
            } else {
                anyhow::bail!("ZeroSlot config required for zeroslot submitter type");
            }
        }
        "solana" => {
            info!("Using standard Solana RPC submitter");
        }
        _ => {
            anyhow::bail!(
                "Unknown submitter type: {}",
                config.execution.submitter.submitter_type
            );
        }
    }

    // Configure leader monitor if enabled
    if let Some(leader_config) = &config.execution.leader_monitor {
        if leader_config.enabled {
            info!(
                "Leader monitor enabled with {} bad validators",
                leader_config.bad_validators.len()
            );
            let bad_validators = leader_config.parse_validators()?;
            executor_builder = executor_builder.with_leader_monitor_config(
                Some(bad_validators),
                Some(leader_config.refresh_interval_secs),
            );
        }
    }

    // Set max slot latency if configured
    if let Some(max_slot_latency) = config.execution.submitter.max_slot_latency {
        executor_builder = executor_builder.with_max_slot_latency(max_slot_latency);
    }

    // Set confirmation timeout
    executor_builder = executor_builder
        .with_confirmation_timeout(Some(config.execution.submitter.confirmation_timeout_secs));

    // Build order executor
    info!("Building order executor...");
    let order_executor = Arc::new(
        executor_builder
            .build(&registry)
            .await
            .context("Failed to build order executor")?,
    );

    // Create notifier (optional)
    let notification_registry = trade_server::notifier::NotificationRegistry::new();
    let notifier: Arc<dyn trade_server::notifier::Notifier> =
        if let Some(notifier_config) = &config.notifier {
            if let Some(_telegram_config) = &notifier_config.telegram {
                info!("Telegram notifications enabled");
                // TODO: Create telegram notifier
                // Arc::new(TelegramNotifier::new(...))
                Arc::new(trade_server::notifier::ConsoleNotifier::new(
                    notification_registry.clone(),
                ))
            } else {
                Arc::new(trade_server::notifier::ConsoleNotifier::new(
                    notification_registry.clone(),
                ))
            }
        } else {
            Arc::new(trade_server::notifier::ConsoleNotifier::new(
                notification_registry,
            ))
        };

    // Create signal persistence (optional)
    let persistence = if let Some(signal_key) = &config.redis.signal_persistence_key {
        info!("Signal persistence enabled to key: {}", signal_key);
        // TODO: Create Redis persistence
        // Some(Arc::new(RedisPersistence::new(...)))
        None
    } else {
        None
    };

    // Create trade server
    info!("Creating trade server...");
    let mut trade_server = if let Some(persistence) = persistence {
        TradeServer::with_persistence(
            event_coordinator,
            vec![], // Signal generators - to be implemented
            notifier,
            position_manager,
            order_executor,
            exit_strategy.clone(),
            config.position.exit_strategy.max_sell_failures,
            config.server.max_latency_ms,
            (*registry).clone(),
            Some(persistence),
        )
    } else {
        TradeServer::new(
            event_coordinator,
            vec![], // Signal generators - to be implemented
            notifier,
            position_manager,
            order_executor,
            exit_strategy,
            config.position.exit_strategy.max_sell_failures,
            config.server.max_latency_ms,
            (*registry).clone(),
        )
    };

    // Start the server
    info!("🚀 Trade Server started successfully!");
    info!("Press Ctrl+C to stop");

    trade_server.run().await.context("Trade server error")?;

    info!("Trade Server stopped");
    Ok(())
}
