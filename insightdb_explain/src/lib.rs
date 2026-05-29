pub mod plan;
pub mod mysql_parser;
pub mod postgres_parser;
pub mod runner;

pub use plan::PlanNode;
pub use runner::{explain, explain_analyze};

/// 执行计划解析统一错误
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("JSON 解析失败: {0}")]
    JsonParse(String),

    #[error("EXPLAIN 输出格式异常: {0}")]
    InvalidFormat(String),

    #[error("执行 EXPLAIN 失败: {0}")]
    ExecutionFailed(String),
}
