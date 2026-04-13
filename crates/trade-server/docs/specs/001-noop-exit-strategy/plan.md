# Noop Exit Strategy 实现方案

## 目标

新增一个 `NoopExitStrategy`（无操作退出策略），用于永不自动触发退出的持仓场景。

## 动机

### 使用场景

1. **手动交易模式**
   - 用户希望完全手动控制退出时机
   - 不希望系统自动止盈/止损

2. **长期持仓**
   - 某些代币需要长期持有（如流动性挖矿）
   - 不应受时间限制自动退出

3. **测试和调试**
   - 测试持仓管理逻辑时，不希望自动退出
   - 便于观察持仓状态变化

4. **特殊策略**
   - 某些信号可能携带自己的退出逻辑
   - 不需要全局的退出策略介入

## 当前架构回顾

### ExitStrategy Trait

```rust
// src/position/exit_strategy.rs
pub trait ExitStrategy: Send + Sync {
    fn should_exit(&self, position: &Position, current_time: DateTime<Utc>) -> bool;
    fn get_exit_reason(&self, position: &Position, current_time: DateTime<Utc>) -> ExitReason;
}
```

### 当前实现

- `ConfigurableExitStrategy`: 检查止盈、止损、超时、卖出失败次数

### 退出触发点

1. **定时器触发** (`PositionManager.handle_timer`)
   - 每 60 秒遍历所有持仓
   - 调用 `exit_strategy.should_exit()`

2. **价格更新触发** (`PositionHandler.handle_position_event`)
   - 每次 `PositionUpdated` 事件
   - 调用 `exit_strategy.should_exit()`

## 实现方案

### 1. 新增 NoopExitStrategy

**文件**: `src/position/exit_strategy.rs`

```rust
/// No-operation exit strategy that never triggers automatic exits.
/// Positions using this strategy will only exit through:
/// - Manual close requests
/// - Force exit due to max sell failures (handled separately)
/// - External signals with explicit sell orders
pub struct NoopExitStrategy;

impl NoopExitStrategy {
    pub fn new() -> Self {
        tracing::info!("Creating NoopExitStrategy - positions will never auto-exit");
        Self
    }
}

impl Default for NoopExitStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl ExitStrategy for NoopExitStrategy {
    fn should_exit(&self, _position: &Position, _current_time: DateTime<Utc>) -> bool {
        // Never trigger automatic exit
        false
    }

    fn get_exit_reason(&self, _position: &Position, _current_time: DateTime<Utc>) -> ExitReason {
        // This should never be called since should_exit always returns false
        // But provide a reasonable default for safety
        ExitReason::ManualClose
    }
}
```

**优点**：
- ✅ 实现简单，逻辑清晰
- ✅ 零开销（无需检查任何条件）
- ✅ 符合 Rust 惯例（提供 `Default` trait）

### 2. 导出新策略

**文件**: `src/position/mod.rs`

```rust
pub use exit_strategy::{
    ConfigurableExitStrategy,
    ExitStrategy,
    NoopExitStrategy,  // 新增
};
```

### 3. 配置支持

**文件**: `src/config.rs`

新增枚举类型：

```rust
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
```

### 4. 启动时根据配置创建策略

**文件**: `src/bin/trade_server.rs`

```rust
// Create exit strategy based on configuration
let exit_strategy: Arc<dyn ExitStrategy> = match config.position.exit_strategy.strategy_type {
    ExitStrategyType::Noop => {
        info!("Using NoopExitStrategy - positions will not auto-exit");
        Arc::new(NoopExitStrategy::new())
    }
    ExitStrategyType::Configurable => {
        let take_profit = config.position.exit_strategy.take_profit_pct.unwrap_or(0.2);
        let stop_loss = config.position.exit_strategy.stop_loss_pct.unwrap_or(-0.1).abs();
        let max_hold_secs = config.position.exit_strategy.max_hold_time_secs
            .unwrap_or(config.position.max_holding_period_secs);

        info!("Using ConfigurableExitStrategy");
        info!("Exit strategy: Take profit at {}%", take_profit * 100.0);
        info!("Exit strategy: Stop loss at {}%", stop_loss * 100.0);
        info!("Exit strategy: Max hold time {} seconds", max_hold_secs);

        Arc::new(ConfigurableExitStrategy::new(
            take_profit,
            stop_loss,
            chrono::Duration::seconds(max_hold_secs),
            config.position.exit_strategy.max_sell_failures,
        ))
    }
};

// Create position manager with selected strategy
let position_manager = Arc::new(InMemoryPositionManager::new(
    config.position.trade_amount_sol,
    config.position.initial_sol,
    chrono::Duration::seconds(config.position.max_holding_period_secs),
    config.position.max_open_positions,
    &registry,
    exit_strategy.clone(),
));
```

### 5. 配置文件示例

**文件**: `config/config.example.yaml`

新增 noop 策略示例：

```yaml
position:
  trade_amount_sol: 0.1
  initial_sol: 10.0
  max_holding_period_secs: 3600
  max_open_positions: 10

  exit_strategy:
    # Option 1: Noop strategy (no automatic exit)
    strategy_type: noop
    max_sell_failures: 3  # Still enforce max sell failures

    # Option 2: Configurable strategy (default)
    # strategy_type: configurable
    # take_profit_pct: 0.20
    # stop_loss_pct: -0.10
    # max_hold_time_secs: 3600
    # max_sell_failures: 3
```

### 6. 单元测试

**文件**: `src/position/exit_strategy_test.rs`

```rust
#[test]
fn test_noop_strategy_never_exits() {
    let strategy = NoopExitStrategy::new();
    let now = Utc::now();

    // Test with profitable position
    let position = create_test_position(Some(100.0), now - Duration::hours(100), 0);
    assert!(!strategy.should_exit(&position, now));

    // Test with losing position
    let position = create_test_position(Some(-90.0), now - Duration::hours(100), 0);
    assert!(!strategy.should_exit(&position, now));

    // Test with very old position
    let position = create_test_position(Some(5.0), now - Duration::days(365), 0);
    assert!(!strategy.should_exit(&position, now));

    // Test with max sell failures
    let position = create_test_position(Some(5.0), now - Duration::hours(1), 999);
    assert!(!strategy.should_exit(&position, now));

    // get_exit_reason should always return ManualClose
    assert_eq!(strategy.get_exit_reason(&position, now), ExitReason::ManualClose);
}

#[test]
fn test_noop_strategy_default() {
    let strategy = NoopExitStrategy::default();
    let now = Utc::now();
    let position = create_test_position(Some(50.0), now, 0);
    assert!(!strategy.should_exit(&position, now));
}
```

## 重要注意事项

### 1. Max Sell Failures 仍然生效

`NoopExitStrategy` **不会**阻止 max sell failures 机制：

```rust
// position_handler.rs - 这个检查在退出策略之前
if position.sell_failure_count >= self.max_sell_failures {
    warn!("Skipping exit trigger due to max sell failures reached");
    return Ok(());
}
```

**原因**：
- Max sell failures 是系统级保护机制
- 防止无限循环尝试卖出失败的持仓
- 即使是 noop 策略也应该尊重这个限制

### 2. 手动平仓仍然可用

用户可以通过以下方式手动平仓：
- API 调用 `core_closePosition(mint)`
- 发送显式的 Sell 信号
- 通过外部脚本触发

### 3. 不影响信号驱动的退出

如果未来实现 RFC 中的信号级别退出计划：

```rust
// 信号可以携带自己的退出逻辑
if let Some(exit_plan) = signal.exit_plan() {
    // 即使全局策略是 noop，信号仍可触发退出
}
```

## 性能影响

### 对比

| 策略类型 | should_exit() 调用开销 | 备注 |
|---------|---------------------|------|
| ConfigurableExitStrategy | ~4 次条件检查 + 日志 | 止盈、止损、超时、失败次数 |
| NoopExitStrategy | 0 次条件检查 | 直接返回 false |

**优势**：
- ✅ 定时器触发时，跳过所有持仓的退出检查
- ✅ 价格更新时，减少不必要的计算

## 部署建议

### 1. 开发环境

```yaml
exit_strategy:
  strategy_type: noop  # 便于调试，观察持仓状态
```

### 2. 测试环境

```yaml
exit_strategy:
  strategy_type: configurable
  take_profit_pct: 0.50   # 宽松的止盈
  stop_loss_pct: -0.20    # 宽松的止损
  max_hold_time_secs: 86400  # 24 小时
```

### 3. 生产环境

```yaml
exit_strategy:
  strategy_type: configurable
  take_profit_pct: 0.20
  stop_loss_pct: -0.10
  max_hold_time_secs: 3600
```

### 4. 特殊用户（手动交易）

```yaml
exit_strategy:
  strategy_type: noop
  max_sell_failures: 5  # 允许更多重试
```

## 未来扩展

### 支持混合策略（RFC 提到的）

```rust
// 未来可以让每个持仓有自己的策略
pub struct Position {
    // ...
    pub custom_exit_strategy: Option<Arc<dyn ExitStrategy>>,
}

// PositionHandler 检查时
let strategy = position.custom_exit_strategy
    .as_ref()
    .unwrap_or(&self.default_exit_strategy);

if strategy.should_exit(position, Utc::now()) {
    // 触发退出
}
```

## 实施计划

### Phase 1: 核心实现（1 小时）
- [ ] 添加 `NoopExitStrategy` 到 `exit_strategy.rs`
- [ ] 导出到 `position/mod.rs`
- [ ] 添加单元测试

### Phase 2: 配置支持（30 分钟）
- [ ] 扩展 `ExitStrategyConfig` 添加 `strategy_type` 字段
- [ ] 更新 `trade_server.rs` 启动逻辑
- [ ] 更新 `config.example.yaml` 添加示例

### Phase 3: 文档和测试（30 分钟）
- [ ] 更新 `README.md` 说明新策略
- [ ] 添加集成测试验证 noop 策略
- [ ] 更新 `docs/architecture.md` 提及新策略

### Phase 4: 验证（30 分钟）
- [ ] 编译通过
- [ ] 所有测试通过
- [ ] 手动测试 noop 模式
- [ ] 手动测试 configurable 模式

## 验收标准

- [x] `NoopExitStrategy` 实现 `ExitStrategy` trait
- [x] `should_exit()` 永远返回 `false`
- [x] 配置文件支持 `strategy_type: noop`
- [x] 单元测试覆盖所有场景
- [x] 文档更新完整
- [x] 向后兼容（默认仍为 configurable）

## 风险评估

### 低风险
- ✅ 实现简单，不易出错
- ✅ 不影响现有功能
- ✅ 向后兼容

### 需注意
- ⚠️ 用户可能误配置为 noop，导致持仓永不退出
- ⚠️ 需在文档中明确说明 noop 的含义和风险

### 缓解措施
- 在启动时打印清晰的日志：`"Using NoopExitStrategy - positions will NEVER auto-exit"`
- 在配置文件中添加注释说明风险
- 考虑添加监控告警：持仓时间超过阈值时发出警告
