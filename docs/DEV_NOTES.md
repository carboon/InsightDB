# InsightDB 开发注意事项

## SQL 安全

### 禁止裸拼接 table_name / column_name 到 SQL

`insightdb_catalog` 的采集 SQL 中，表名来自 `information_schema` 查询结果，看似安全，但仍可能因边界场景（如含特殊字符的表名）导致注入或语法错误。

**规则**：
- MySQL 表名用反引号包裹，内部反引号双写转义：`` `{}`.replace('`', '``') ``
- PostgreSQL 字符串用单引号包裹，内部单引号双写转义：`'{}'.replace('\'', '\'\'')`
- 禁止用 `format!` 直接将用户可控字符串嵌入 SQL

**根因**：设计文档 ARCHITECTURE_AND_STANDARDS.md 安全规范明确要求"危险 SQL 拦截"，防御性转义是最底线。

## sqlx Any 驱动

### Any 驱动不支持所有数据库原生类型

`sqlx::Any` 是兼容层，`try_get::<T>` 只支持 `Any` trait 已实现的类型映射。不支持的类型直接返回 `Err`。

**已知限制**：
- `u32` 不可用，需用 `i64` 替代
- MySQL `NewDecimal`（`DECIMAL` 列）不被 `Any` 驱动支持
- `Vec<u8>` 列优先尝试 UTF-8 解码，失败再 fallback 到 hex

**应对**：`format_any_value()` 采用多类型回退链：`String → i64 → i32 → f64 → bool → Vec<u8>`

### `install_default_drivers()` 必须在首次数据库操作前调用

每条需要数据库连接的测试开头必须调用 `sqlx::any::install_default_drivers()`。该调用幂等，重复执行无副作用。

### PostgreSQL EXPLAIN 需要原生驱动

`sqlx::Any` 驱动无法正确读取 PG `EXPLAIN (FORMAT JSON)` 返回的 JSON 类型列。`insightdb_explain::runner` 使用 `sqlx::PgConnection` 原生驱动执行 PG EXPLAIN。

## 连接层

### 默认只读模式通过 `after_connect` 钩子实现

连接池的每个新连接建立后自动执行：
- MySQL: `SET SESSION TRANSACTION READ ONLY`
- PostgreSQL: `SET default_transaction_read_only = on`

如需写操作，必须显式设置 `config.read_only = false`。

### 连接池不需要 Mutex

`sqlx::AnyPool` 本身是 `Clone + Send + Sync`，无需 `Arc<Mutex<>>` 包裹。后端 PID 跟踪使用 `Arc<AtomicU32>`。

### `query_stream()` 内部使用 `tokio::spawn`

流式查询通过 channel 实现，调用时必须在 tokio runtime 上下文内（不能在 `block_on` 外部创建 stream 后传入）。

## 执行计划解析

### `postgres_parser` 的 `extra` 字段只存提取的结构化数据

原始 JSON 不保留到 `extra` 字段。如需完整原始数据用于审计，应在调用侧单独保存。

### MySQL EXPLAIN JSON 结构依赖 `query_block` 嵌套

MySQL 的 JSON 输出结构因查询复杂度不同而变化：
- 单表：`query_block.table`
- ORDER BY：`query_block.ordering_operation.table`
- GROUP BY：`query_block.grouping_operation.table`
- 多表 JOIN：`query_block.nested_loop[].table`

解析器必须按优先级依次检查这些路径。

## Rust 版本与依赖

### 要求 Rust 1.86+

`home@0.5.12`、`icu_*@2.2.0`、`idna_adapter@1.2.2` 等依赖要求 Rust >= 1.86。Homebrew 安装的 Rust 1.85 不满足，需 `brew upgrade rust`。

### sqlx-postgres 0.7.4 有 future-incompat 警告

编译时会输出 "will be rejected by a future version of Rust" 警告。这是 sqlx 0.7.4 的已知问题，升级到 0.8 可解决。
