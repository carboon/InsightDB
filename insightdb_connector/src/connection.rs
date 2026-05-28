use sqlx::{Column, Either, Executor, Row};
use crate::config::{ConnectorConfig, DatabaseKind};
use crate::error::ConnectorError;
use crate::query::QueryResult;
use crate::cancel::QueryCanceller;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 数据库连接封装，提供执行查询、取消等能力
#[derive(Debug, Clone)]
pub struct DatabaseConnection {
    config: ConnectorConfig,
    /// 共用内部池，通过 Arc<Mutex<>> 保证取消操作的线程安全
    /// （实际生产应考虑更精细的锁，此处为演示）
    pool: Arc<Mutex<sqlx::AnyPool>>,
    /// 当前活跃查询的后端进程ID（MySQL: 连接ID, PG: pid）
    /// 取消操作时需要该 ID
    backend_pid: Arc<Mutex<Option<u32>>>,
}

impl DatabaseConnection {
    /// 根据配置建立连接池
    pub async fn connect(config: ConnectorConfig) -> Result<Self, ConnectorError> {
        let connection_string = config.to_connection_string();
        let pool = sqlx::any::AnyPoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await
            .map_err(|e| ConnectorError::ConnectionFailed {
                message: format!("无法建立连接池: {e}"),
                suggestion: Some("请检查网络、凭据和数据库服务状态".to_string()),
                retryable: true,
                source_str: Some(format!("{e:?}")),
            })?;

        Ok(DatabaseConnection {
            config,
            pool: Arc::new(Mutex::new(pool)),
            backend_pid: Arc::new(Mutex::new(None)),
        })
    }

    /// 获取数据库类型
    pub fn database_kind(&self) -> &DatabaseKind {
        &self.config.kind
    }

    /// 执行 SQL 查询，返回行流式结果（每次最多取 fetch_size 行）
    pub async fn query(
        &self,
        sql: &str,
        fetch_size: usize,
    ) -> Result<QueryResult, ConnectorError> {
        let pool = self.pool.lock().await;
        *self.backend_pid.lock().await = Some(0u32); // 占位

        let mut conn = pool.acquire().await
            .map_err(|e| ConnectorError::QueryExecutionFailed {
                message: format!("获取连接失败: {e}"),
                suggestion: None,
                retryable: true,
                source_str: Some(format!("{e:?}")),
            })?;

        use futures::TryStreamExt;
        let mut stream = conn.fetch_many(sql);
        let mut rows = Vec::new();
        while let Some(result) = stream.try_next().await
            .map_err(|e| ConnectorError::QueryExecutionFailed {
                message: format!("流式读取失败: {e}"),
                suggestion: None,
                retryable: false,
                source_str: Some(format!("{e:?}")),
            })?
        {
            match result {
                Either::Left(_query_result) => { /* 不影响 */ }
                Either::Right(row) => {
                    if rows.len() >= fetch_size {
                        break;
                    }
                    let columns: Vec<String> = row.columns().iter().map(|c| c.name().to_string()).collect();
                    let values: Vec<Option<String>> = (0..row.len())
                        .map(|i| row.try_get::<String, _>(i).ok())
                        .collect();
                    rows.push((columns.clone(), values));
                }
            }
        }

        Ok(QueryResult {
            columns: if rows.is_empty() { Vec::new() } else { rows[0].0.clone() },
            rows: rows.into_iter().map(|(_, vals)| vals).collect(),
            affected_rows: None,
        })
    }

    /// 创建一个取消器
    pub fn canceller(&self) -> QueryCanceller {
        QueryCanceller::new(self.config.clone(), self.backend_pid.clone())
    }

    /// 测试数据库连通性：执行 SELECT 1
    pub async fn ping(&self) -> Result<(), ConnectorError> {
        let pool = self.pool.lock().await;
        pool.execute("SELECT 1")
            .await
            .map(|_| ())
            .map_err(|e| ConnectorError::ConnectionFailed {
                message: format!("ping 失败: {e}"),
                suggestion: Some("请检查数据库连接是否正常".to_string()),
                retryable: true,
                source_str: Some(format!("{e:?}")),
            })
    }
}
