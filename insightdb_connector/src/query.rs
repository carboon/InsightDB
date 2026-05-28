use sqlx::Row;

/// 查询结果，列为 Vec<String>，每行为 Vec<Option<String>>
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub affected_rows: Option<u64>,
}

/// 单行数据（每列可选值）
pub type QueryRow = Vec<Option<String>>;

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
        return Some(format!("0x{}", hex::encode(&v)));
    }
    None
}
