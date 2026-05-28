use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use crate::config::{ConnectorConfig, DatabaseKind};
use crate::error::ConnectorError;
use sqlx::Executor;

/// 查询取消器，通过新建独立连接向后端发送取消命令
#[derive(Clone)]
pub struct QueryCanceller {
    config: ConnectorConfig,
    backend_pid: Arc<AtomicU32>,
    pool: sqlx::AnyPool,
}

impl QueryCanceller {
    pub(crate) fn new(
        config: ConnectorConfig,
        backend_pid: Arc<AtomicU32>,
        pool: sqlx::AnyPool,
    ) -> Self {
        Self { config, backend_pid, pool }
    }

    /// 取消当前正在执行的查询
    ///
    /// MySQL: 执行 `KILL QUERY <pid>`
    /// PostgreSQL: 执行 `SELECT pg_cancel_backend(<pid>)`
    pub async fn cancel(&self) -> Result<(), ConnectorError> {
        let pid = self.backend_pid.load(Ordering::SeqCst);
        if pid == 0 {
            return Err(ConnectorError::CancelFailed {
                message: "当前没有正在执行的查询".to_string(),
                suggestion: None,
                retryable: false,
                source_str: None,
            });
        }

        let cancel_sql = match self.config.kind {
            DatabaseKind::MySQL => format!("KILL QUERY {pid}"),
            DatabaseKind::PostgreSQL => format!("SELECT pg_cancel_backend({pid})"),
        };

        // 使用连接池中的一个连接发送取消命令
        self.pool.execute(&cancel_sql as &str)
            .await
            .map_err(|e| ConnectorError::CancelFailed {
                message: format!("取消查询失败 (pid={pid}): {e}"),
                suggestion: Some("可能查询已自行结束，或当前用户无权限取消该查询".to_string()),
                retryable: true,
                source_str: Some(format!("{e:?}")),
            })?;

        self.backend_pid.store(0, Ordering::SeqCst);
        Ok(())
    }
}
