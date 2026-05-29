use std::sync::Arc;
use std::sync::OnceLock;
use sqlx::Row;
use crate::error::ConnectorError;

/// 查询结果，列为 Vec<String>，每行为 Vec<Option<String>>
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub affected_rows: Option<u64>,
}

/// 单行数据（每列可选值）
pub type QueryRow = Vec<Option<String>>;

/// 流式查询结果，通过异步通道逐行返回行数据
pub struct QueryStream {
    columns: Arc<OnceLock<Vec<String>>>,
    rx: tokio::sync::mpsc::Receiver<Result<QueryRow, ConnectorError>>,
}

impl QueryStream {
    pub(crate) fn new(
        columns: Arc<OnceLock<Vec<String>>>,
        rx: tokio::sync::mpsc::Receiver<Result<QueryRow, ConnectorError>>,
    ) -> Self {
        Self { columns, rx }
    }

    /// 获取列名（首行到达后可用）
    pub fn columns(&self) -> Option<&[String]> {
        self.columns.get().map(|v| v.as_slice())
    }

    /// 获取下一行，返回 None 表示流结束
    pub async fn next(&mut self) -> Option<Result<QueryRow, ConnectorError>> {
        self.rx.recv().await
    }
}

/// 从 sqlx Row 中按列索引提取值，按类型优先级回退
///
/// 回退顺序：String -> i64 -> f64 -> bool -> Vec<u8>(hex)
pub fn format_any_value(row: &sqlx::any::AnyRow, idx: usize) -> Option<String> {
    if let Ok(v) = row.try_get::<String, _>(idx) {
        return Some(v);
    }
    if let Ok(v) = row.try_get::<i64, _>(idx) {
        return Some(v.to_string());
    }
    if let Ok(v) = row.try_get::<i32, _>(idx) {
        return Some(v.to_string());
    }
    if let Ok(v) = row.try_get::<f64, _>(idx) {
        return Some(v.to_string());
    }
    if let Ok(v) = row.try_get::<bool, _>(idx) {
        return Some(v.to_string());
    }
    if let Ok(v) = row.try_get::<Vec<u8>, _>(idx) {
        return String::from_utf8(v.clone())
            .ok()
            .or_else(|| Some(format!("0x{}", hex::encode(&v))));
    }
    None
}
