use sqlx::{Column, Either, Executor, Row};
use crate::config::{ConnectorConfig, DatabaseKind};
use crate::error::ConnectorError;
use crate::query::{QueryResult, QueryStream, QueryRow};
use crate::cancel::QueryCanceller;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

/// 数据库连接封装，提供执行查询、取消等能力
pub struct DatabaseConnection {
    config: ConnectorConfig,
    pool: sqlx::AnyPool,
    /// 当前活跃查询的后端进程ID（MySQL: CONNECTION_ID(), PG: pg_backend_pid()）
    /// 0 表示无活跃查询
    backend_pid: Arc<AtomicU32>,
}

impl std::fmt::Debug for DatabaseConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseConnection")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl DatabaseConnection {
    /// 根据配置建立连接池
    pub async fn connect(config: ConnectorConfig) -> Result<Self, ConnectorError> {
        let connection_string = config.to_connection_string();

        let mut pool_opts = sqlx::any::AnyPoolOptions::new()
            .max_connections(5);

        if config.read_only {
            let kind = config.kind.clone();
            pool_opts = pool_opts.after_connect(move |conn: &mut sqlx::AnyConnection, _| {
                let kind = kind.clone();
                Box::pin(async move {
                    match kind {
                        DatabaseKind::MySQL => {
                            sqlx::query("SET SESSION TRANSACTION READ ONLY")
                                .execute(&mut *conn).await?;
                        }
                        DatabaseKind::PostgreSQL => {
                            sqlx::query("SET default_transaction_read_only = on")
                                .execute(&mut *conn).await?;
                        }
                    }
                    Ok(())
                })
            });
        }

        let pool = pool_opts
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
            pool,
            backend_pid: Arc::new(AtomicU32::new(0)),
        })
    }

    /// 获取数据库类型
    pub fn database_kind(&self) -> &DatabaseKind {
        &self.config.kind
    }

    /// 获取连接配置的引用（用于某些需要原生驱动的场景）
    pub fn config(&self) -> &ConnectorConfig {
        &self.config
    }

    /// 获取连接字符串（用于创建原生数据库连接）
    pub fn connection_string(&self) -> String {
        self.config.to_connection_string()
    }

    /// 执行 SQL 查询，返回行流式结果（每次最多取 fetch_size 行）
    pub async fn query(
        &self,
        sql: &str,
        fetch_size: usize,
    ) -> Result<QueryResult, ConnectorError> {
        let mut conn = self.pool.acquire().await
            .map_err(|e| ConnectorError::QueryExecutionFailed {
                message: format!("获取连接失败: {e}"),
                suggestion: None,
                retryable: true,
                source_str: Some(format!("{e:?}")),
            })?;

        // 获取后端 PID
        let pid = self.fetch_backend_pid(&mut conn).await.unwrap_or(0);
        self.backend_pid.store(pid, Ordering::SeqCst);

        use futures::TryStreamExt;
        let mut stream = conn.fetch_many(sql);
        let mut rows = Vec::new();
        while let Some(result) = stream.try_next().await
            .map_err(|e| {
                self.backend_pid.store(0, Ordering::SeqCst);
                ConnectorError::QueryExecutionFailed {
                    message: format!("流式读取失败: {e}"),
                    suggestion: None,
                    retryable: false,
                    source_str: Some(format!("{e:?}")),
                }
            })?
        {
            match result {
                Either::Left(_query_result) => {}
                Either::Right(row) => {
                    if rows.len() >= fetch_size {
                        break;
                    }
                    let columns: Vec<String> = row.columns().iter().map(|c| c.name().to_string()).collect();
                    let values: Vec<Option<String>> = (0..row.len())
                        .map(|i| crate::query::format_any_value(&row, i))
                        .collect();
                    rows.push((columns.clone(), values));
                }
            }
        }

        self.backend_pid.store(0, Ordering::SeqCst);

        Ok(QueryResult {
            columns: if rows.is_empty() { Vec::new() } else { rows[0].0.clone() },
            rows: rows.into_iter().map(|(_, vals)| vals).collect(),
            affected_rows: None,
        })
    }

    /// 流式查询，通过异步通道逐行返回结果，不将完整结果集加载到内存
    pub fn query_stream(&self, sql: &str) -> QueryStream {
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<QueryRow, ConnectorError>>(64);
        let columns = Arc::new(OnceLock::new());
        let cols_ref = columns.clone();
        let pool = self.pool.clone();
        let sql = sql.to_owned();

        tokio::spawn(async move {
            use futures::TryStreamExt;

            let mut stream = pool.fetch_many(sql.as_str());
            loop {
                match stream.try_next().await {
                    Ok(Some(Either::Left(_))) => {}
                    Ok(Some(Either::Right(row))) => {
                        let row_columns: Vec<String> = row.columns().iter()
                            .map(|c| c.name().to_string()).collect();
                        let _ = cols_ref.set(row_columns);
                        let values: Vec<Option<String>> = (0..row.len())
                            .map(|i| crate::query::format_any_value(&row, i))
                            .collect();
                        if tx.send(Ok(values)).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let _ = tx.send(Err(ConnectorError::from(e))).await;
                        break;
                    }
                }
            }
        });

        QueryStream::new(columns, rx)
    }

    /// 创建一个取消器
    pub fn canceller(&self) -> QueryCanceller {
        QueryCanceller::new(self.config.clone(), self.backend_pid.clone(), self.pool.clone())
    }

    /// 测试数据库连通性：执行 SELECT 1
    pub async fn ping(&self) -> Result<(), ConnectorError> {
        self.pool.execute("SELECT 1")
            .await
            .map(|_| ())
            .map_err(|e| ConnectorError::ConnectionFailed {
                message: format!("ping 失败: {e}"),
                suggestion: Some("请检查数据库连接是否正常".to_string()),
                retryable: true,
                source_str: Some(format!("{e:?}")),
            })
    }

    /// 显式关闭连接池
    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// 查询当前连接的后端 PID
    async fn fetch_backend_pid(&self, conn: &mut sqlx::AnyConnection) -> Result<u32, ConnectorError> {
        let pid_sql = match self.config.kind {
            DatabaseKind::MySQL => "SELECT CONNECTION_ID()",
            DatabaseKind::PostgreSQL => "SELECT pg_backend_pid()",
        };
        let row = sqlx::query(pid_sql)
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| ConnectorError::Internal {
                message: format!("获取后端 PID 失败: {e}"),
                source_str: Some(format!("{e:?}")),
            })?;
        Ok(row.try_get::<i64, _>(0).unwrap_or(0) as u32)
    }
}
