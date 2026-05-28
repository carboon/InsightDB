/// 查询结果，列为 Vec<String>，每行为 Vec<Option<String>>
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub affected_rows: Option<u64>,
}

/// 单行数据（每列可选值）
pub type QueryRow = Vec<Option<String>>;
