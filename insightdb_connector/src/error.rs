use std::fmt;
use thiserror::Error;

/// InsightDB 统一连接层错误
///
/// 所有来自连接、查询、取消的错误都转换为该枚举，确保上层（Bridge/UI）不依赖具体数据库驱动错误。
#[derive(Error, Debug)]
pub enum ConnectorError {
    /// 连接配置解析无效
    #[error("无效的连接配置: {message}")]
    InvalidConfig {
        message: String,
        suggestion: Option<String>,
    },

    /// 连接数据库失败（网络、凭据、权限等）
    #[error("连接数据库失败: {message}")]
    ConnectionFailed {
        message: String,
        suggestion: Option<String>,
        retryable: bool,
        source: Option<String>,
    },

    /// 查询执行失败（语法错误、权限不足等）
    #[error("查询执行失败: {message}")]
    QueryExecutionFailed {
        message: String,
        suggestion: Option<String>,
        retryable: bool,
        source: Option<String>,
    },

    /// 查询取消失败（后端不支持、连接丢失等）
    #[error("查询取消失败: {message}")]
    CancelFailed {
        message: String,
        suggestion: Option<String>,
        retryable: bool,
        source: Option<String>,
    },

    /// 查询超时
    #[error("查询超时 (已耗时 {elapsed_secs} 秒)")]
    Timeout {
        elapsed_secs: u64,
        suggestion: Option<String>,
    },

    /// 查询中断（用户主动取消）
    #[error("查询已被用户取消")]
    Cancelled,

    /// 内部错误（不应出现的 bug）
    #[error("内部错误: {message}")]
    Internal {
        message: String,
        source: Option<String>,
    },
}

impl ConnectorError {
    /// 统一的错误代码字符串，供 Bridge 层映射
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfig { .. } => "INVALID_CONFIG",
            Self::ConnectionFailed { .. } => "CONNECTION_FAILED",
            Self::QueryExecutionFailed { .. } => "QUERY_EXECUTION_FAILED",
            Self::CancelFailed { .. } => "CANCEL_FAILED",
            Self::Timeout { .. } => "TIMEOUT",
            Self::Cancelled => "CANCELLED",
            Self::Internal { .. } => "INTERNAL_ERROR",
        }
    }

    /// 获取 `suggestion` 字段，用于 UI 提示
    pub fn suggestion(&self) -> Option<&str> {
        match self {
            Self::InvalidConfig { suggestion, .. }
            | Self::ConnectionFailed { suggestion, .. }
            | Self::QueryExecutionFailed { suggestion, .. }
            | Self::CancelFailed { suggestion, .. } => suggestion.as_deref(),
            Self::Timeout { suggestion, .. } => suggestion.as_deref(),
            _ => None,
        }
    }

    /// 是否可重试
    pub fn retryable(&self) -> bool {
        match self {
            Self::ConnectionFailed { retryable, .. }
            | Self::QueryExecutionFailed { retryable, .. }
            | Self::CancelFailed { retryable, .. } => *retryable,
            Self::Timeout { .. } => true,
            _ => false,
        }
    }

    /// 原始错误来源（仅供日志 debug 使用）
    pub fn source_error(&self) -> Option<&str> {
        match self {
            Self::ConnectionFailed { source, .. }
            | Self::QueryExecutionFailed { source, .. }
            | Self::CancelFailed { source, .. }
            | Self::Internal { source, .. } => source.as_deref(),
            _ => None,
        }
    }
}

// 实现从 sqlx::Error 转换为 ConnectorError
impl From<sqlx::Error> for ConnectorError {
    fn from(err: sqlx::Error) -> Self {
        let message = err.to_string();
        // 根据 sqlx 错误的具体变体做简单分类
        if matches!(&err, sqlx::Error::PoolClosed | sqlx::Error::PoolTimedOut) {
            ConnectorError::ConnectionFailed {
                message,
                suggestion: Some("请检查数据库是否可访问，并稍后重试".to_string()),
                retryable: true,
                source: Some(format!("{err:?}")),
            }
        } else if matches!(&err, sqlx::Error::Database(_)) {
            ConnectorError::QueryExecutionFailed {
                message,
                suggestion: None,
                retryable: false,
                source: Some(format!("{err:?}")),
            }
        } else if matches!(&err, sqlx::Error::Io(_)) {
            ConnectorError::ConnectionFailed {
                message,
                suggestion: Some("网络连接异常，请确认数据库主机和端口是否正确".to_string()),
                retryable: true,
                source: Some(format!("{err:?}")),
            }
        } else {
            ConnectorError::Internal {
                message,
                source: Some(format!("{err:?}")),
            }
        }
    }
}

/// 方便从字符串创建 InvalidConfig 错误
impl ConnectorError {
    pub fn invalid_config(msg: impl Into<String>, suggestion: Option<String>) -> Self {
        Self::InvalidConfig {
            message: msg.into(),
            suggestion,
        }
    }
}

impl fmt::Display for ConnectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // thiserror 已经生成 Display，这里保留空实现避免冲突
        write!(f, "{}", self)
    }
}
