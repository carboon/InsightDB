use std::sync::Arc;
use tokio::sync::Mutex;
use crate::config::ConnectorConfig;
use crate::error::ConnectorError;

/// 查询取消器，允许用户主动取消正在执行的 SQL
#[derive(Clone)]
pub struct QueryCanceller {
    _config: ConnectorConfig,
    backend_pid: Arc<Mutex<Option<u32>>>,
}

impl QueryCanceller {
    pub(crate) fn new(config: ConnectorConfig, backend_pid: Arc<Mutex<Option<u32>>>) -> Self {
        Self { _config: config, backend_pid }
    }

    /// 取消当前查询（仅记录标记，真正取消需另建连接执行后台命令）
    /// 此简化实现仅将 backend_pid 设为 None，表示已取消
    /// 实际生产应在另一个连接上执行 KILL QUERY 或 pg_cancel_backend
    pub async fn cancel(&self) -> Result<(), ConnectorError> {
        let mut pid_opt = self.backend_pid.lock().await;
        if let Some(pid) = *pid_opt {
            // TODO: 根据数据库类型建立新连接并发送取消命令
            // 此处仅模拟成功
            log::info!("取消请求已发送 (pid={})", pid);
        } else {
            return Err(ConnectorError::CancelFailed {
                message: "当前没有正在执行的查询".to_string(),
                suggestion: None,
                retryable: false,
                source_str: None,
            });
        }
        // 取消标记
        *pid_opt = None;
        Ok(())
    }
}
