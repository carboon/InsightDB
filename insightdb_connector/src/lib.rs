//! # InsightDB Connector
//!
//! 数据库连接层，负责管理 MySQL/PostgreSQL 连接、执行查询、取消操作及标准化错误返回。
//!
//! 对外公开的模块：
//! - `error`  : 统一错误模型 `ConnectorError`
//! - `config` : 连接配置解析 `ConnectorConfig`
//! - `connection` : 连接管理 `DatabaseConnection`
//! - `query`  : 查询执行与结果流式读取
//! - `cancel` : 查询取消支持

pub mod error;
pub mod config;
pub mod connection;
pub mod query;
pub mod cancel;

// 重新导出常用类型方便外部使用
pub use error::ConnectorError;
pub use config::{ConnectorConfig, DatabaseKind};
pub use connection::DatabaseConnection;
pub use query::{QueryResult, QueryRow, QueryStream};
pub use cancel::QueryCanceller;
