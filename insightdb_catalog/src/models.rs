use serde::{Deserialize, Serialize};

/// 数据库 Schema 完整快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaInfo {
    pub db_type: String,
    pub version: String,
    pub database_name: String,
    pub tables: Vec<TableInfo>,
    pub collected_at: String,
}

/// 表信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    pub name: String,
    pub table_type: String,
    pub engine: Option<String>,
    pub row_count_estimate: Option<u64>,
    pub columns: Vec<ColumnInfo>,
    pub indexes: Vec<IndexInfo>,
}

/// 列信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub ordinal: u32,
    pub data_type: String,
    pub nullable: bool,
    pub is_primary_key: bool,
    pub default_value: Option<String>,
    pub character_max_length: Option<u64>,
    pub column_comment: Option<String>,
}

/// 索引信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub index_type: String,
    pub is_primary: bool,
}

/// 数据库版本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub full_version: String,
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}
