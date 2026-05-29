use insightdb_connector::{ConnectorError, DatabaseConnection, DatabaseKind};
use crate::models::*;

/// 采集数据库 Schema 完整快照
pub async fn collect_schema(conn: &DatabaseConnection) -> Result<SchemaInfo, ConnectorError> {
    let version = collect_version(conn).await?;
    let db_name = collect_database_name(conn).await?;
    let tables = collect_tables(conn).await?;

    Ok(SchemaInfo {
        db_type: db_type_label(conn.database_kind()),
        version,
        database_name: db_name,
        tables,
        collected_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// 采集数据库版本号
pub async fn collect_version(conn: &DatabaseConnection) -> Result<String, ConnectorError> {
    let sql = match conn.database_kind() {
        DatabaseKind::MySQL => "SELECT VERSION()",
        DatabaseKind::PostgreSQL => "SELECT version()",
    };
    let result = conn.query(sql, 1).await?;
    let version = result.rows.first()
        .and_then(|row| row.first().cloned())
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());
    Ok(version)
}

/// 采集当前数据库名
pub async fn collect_database_name(conn: &DatabaseConnection) -> Result<String, ConnectorError> {
    let sql = match conn.database_kind() {
        DatabaseKind::MySQL => "SELECT DATABASE()",
        DatabaseKind::PostgreSQL => "SELECT current_database() || ''",
    };
    let result = conn.query(sql, 1).await?;
    let name = result.rows.first()
        .and_then(|row| row.first().cloned())
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());
    Ok(name)
}

/// 采集所有用户表信息（含列和索引）
pub async fn collect_tables(conn: &DatabaseConnection) -> Result<Vec<TableInfo>, ConnectorError> {
    let table_list = list_tables(conn).await?;
    let mut tables = Vec::new();
    for table_name in table_list {
        match collect_table_detail(conn, &table_name).await {
            Ok(t) => tables.push(t),
            Err(e) => {
                log::warn!("采集表 {table_name} 失败: {e}");
                continue;
            }
        }
    }
    Ok(tables)
}

/// 列出用户表名
async fn list_tables(conn: &DatabaseConnection) -> Result<Vec<String>, ConnectorError> {
    let sql = match conn.database_kind() {
        DatabaseKind::MySQL => {
            "SELECT TABLE_NAME FROM information_schema.TABLES
             WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE = 'BASE TABLE'
             ORDER BY TABLE_NAME"
        }
        DatabaseKind::PostgreSQL => {
            "SELECT c.relname || ''
             FROM pg_catalog.pg_class c
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             WHERE c.relkind = 'r'
               AND n.nspname NOT IN ('pg_catalog', 'information_schema')
             ORDER BY c.relname"
        }
    };
    let result = conn.query(sql, 1000).await?;
    Ok(result.rows.iter()
        .filter_map(|row| row.first().cloned().flatten())
        .collect())
}

/// 采集单表的详细信息
async fn collect_table_detail(
    conn: &DatabaseConnection,
    table_name: &str,
) -> Result<TableInfo, ConnectorError> {
    match conn.database_kind() {
        DatabaseKind::MySQL => collect_mysql_table(conn, table_name).await,
        DatabaseKind::PostgreSQL => collect_postgres_table(conn, table_name).await,
    }
}

fn db_type_label(kind: &DatabaseKind) -> String {
    match kind {
        DatabaseKind::MySQL => "mysql".to_string(),
        DatabaseKind::PostgreSQL => "postgresql".to_string(),
    }
}

// ── MySQL 采集逻辑 ──

async fn collect_mysql_table(
    conn: &DatabaseConnection,
    table_name: &str,
) -> Result<TableInfo, ConnectorError> {
    let columns = collect_mysql_columns(conn, table_name).await?;
    let indexes = collect_mysql_indexes(conn, table_name).await?;
    let (engine, row_estimate) = collect_mysql_table_status(conn, table_name).await?;

    Ok(TableInfo {
        name: table_name.to_string(),
        table_type: "BASE TABLE".to_string(),
        engine: Some(engine),
        row_count_estimate: row_estimate,
        columns,
        indexes,
    })
}

async fn collect_mysql_columns(
    conn: &DatabaseConnection,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, ConnectorError> {
    // 使用 SHOW COLUMNS 避免 information_schema 的 Any 驱动类型映射问题
    let sql = format!("SHOW COLUMNS FROM `{}`", table_name.replace('`', "``"));
    let result = conn.query(&sql, 1000).await?;
    Ok(result.rows.iter().enumerate().map(|(idx, row)| {
        // SHOW COLUMNS 返回: Field, Type, Null, Key, Default, Extra
        let name = row.get(0).cloned().flatten().unwrap_or_default();
        let data_type = row.get(1).cloned().flatten().unwrap_or_default();
        let nullable = row.get(2).cloned().flatten()
            .map(|v| v == "YES").unwrap_or(false);
        let col_key = row.get(3).cloned().flatten().unwrap_or_default();
        let default_value = row.get(4).cloned().flatten()
            .filter(|v| !v.is_empty());

        ColumnInfo {
            name,
            ordinal: (idx + 1) as u32,
            data_type,
            nullable,
            is_primary_key: col_key == "PRI",
            default_value,
            character_max_length: None,   // SHOW COLUMNS 不直接提供
            column_comment: None,
        }
    }).collect())
}

async fn collect_mysql_indexes(
    conn: &DatabaseConnection,
    table_name: &str,
) -> Result<Vec<IndexInfo>, ConnectorError> {
    let sql = format!("SHOW INDEX FROM `{}`", table_name.replace('`', "``"));
    let result = conn.query(&sql, 1000).await?;

    let mut index_map: std::collections::BTreeMap<String, IndexInfo> = std::collections::BTreeMap::new();
    for row in &result.rows {
        let index_name = row.get(2).cloned().flatten().unwrap_or_default();
        let col_name = row.get(4).cloned().flatten().unwrap_or_default();
        let non_unique = row.get(1).cloned().flatten()
            .and_then(|v| v.parse::<u8>().ok()).unwrap_or(1);
        let index_type = row.get(10).cloned().flatten().unwrap_or_else(|| "BTREE".to_string());

        let entry = index_map.entry(index_name.clone()).or_insert_with(|| IndexInfo {
            name: index_name,
            columns: vec![],
            unique: non_unique == 0,
            index_type,
            is_primary: false,
        });
        entry.columns.push(col_name);
    }

    // 标识主键索引
    for idx in index_map.values_mut() {
        if idx.name == "PRIMARY" {
            idx.is_primary = true;
        }
    }

    Ok(index_map.into_values().collect())
}

async fn collect_mysql_table_status(
    conn: &DatabaseConnection,
    table_name: &str,
) -> Result<(String, Option<u64>), ConnectorError> {
    let sql = format!(
        "SELECT ENGINE, TABLE_ROWS FROM information_schema.TABLES
         WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '{}'", table_name.replace('\'', "''"));
    let result = conn.query(&sql, 1).await?;
    if let Some(row) = result.rows.first() {
        let engine = row.get(0).cloned().flatten().unwrap_or_else(|| "InnoDB".to_string());
        let row_count = row.get(1).cloned().flatten()
            .and_then(|v| v.parse().ok());
        Ok((engine, row_count))
    } else {
        Ok(("InnoDB".to_string(), None))
    }
}

// ── PostgreSQL 采集逻辑 ──

async fn collect_postgres_table(
    conn: &DatabaseConnection,
    table_name: &str,
) -> Result<TableInfo, ConnectorError> {
    let columns = collect_postgres_columns(conn, table_name).await?;
    let indexes = collect_postgres_indexes(conn, table_name).await?;
    let row_estimate = collect_postgres_row_estimate(conn, table_name).await?;

    Ok(TableInfo {
        name: table_name.to_string(),
        table_type: "BASE TABLE".to_string(),
        engine: None,
        row_count_estimate: row_estimate,
        columns,
        indexes,
    })
}

fn pg_escape(s: &str) -> String {
    s.replace('\'', "''")
}

async fn collect_postgres_columns(
    conn: &DatabaseConnection,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, ConnectorError> {
    let tn = pg_escape(table_name);
    let sql = format!(
        "SELECT c.column_name || '' as column_name, c.ordinal_position::int, c.data_type,
                c.is_nullable, c.column_default,
                CASE WHEN pk.column_name IS NOT NULL THEN 1 ELSE 0 END as is_pk,
                c.character_maximum_length::bigint,
                pg_catalog.col_description(
                    (SELECT c.oid FROM pg_catalog.pg_class c
                     JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
                     WHERE c.relname = '{tn}'
                     AND n.nspname = current_schema()
                    ),
                    c.ordinal_position
                ) as col_comment
         FROM information_schema.columns c
         LEFT JOIN (
             SELECT ku.column_name
             FROM information_schema.table_constraints tc
             JOIN information_schema.key_column_usage ku
               ON tc.constraint_name = ku.constraint_name
             WHERE tc.constraint_type = 'PRIMARY KEY'
               AND tc.table_name = '{tn}'
         ) pk ON c.column_name = pk.column_name
         WHERE c.table_name = '{tn}'
         ORDER BY c.ordinal_position"
    );
    let result = conn.query(&sql, 1000).await?;
    Ok(result.rows.iter().map(|row| {
        ColumnInfo {
            name: row.get(0).cloned().flatten().unwrap_or_default(),
            ordinal: row.get(1).cloned().flatten()
                .and_then(|v: String| v.parse().ok()).unwrap_or(0),
            data_type: row.get(2).cloned().flatten().unwrap_or_default(),
            nullable: row.get(3).cloned().flatten()
                .map(|v| v == "YES").unwrap_or(false),
            default_value: row.get(4).cloned().flatten(),
            is_primary_key: row.get(5).cloned().flatten()
                .map(|v| v == "1").unwrap_or(false),
            character_max_length: row.get(6).cloned().flatten()
                .and_then(|v| v.parse().ok()),
            column_comment: row.get(7).cloned().flatten(),
        }
    }).collect())
}

async fn collect_postgres_indexes(
    conn: &DatabaseConnection,
    table_name: &str,
) -> Result<Vec<IndexInfo>, ConnectorError> {
    let tn = pg_escape(table_name);
    let sql = format!(
        "SELECT indexname || '' as indexname, indexdef
         FROM pg_indexes
         WHERE tablename = '{tn}'
           AND schemaname NOT IN ('pg_catalog', 'information_schema')
         ORDER BY indexname"
    );
    let result = conn.query(&sql, 1000).await?;
    let mut indexes = Vec::new();
    for row in &result.rows {
        let name = row.get(0).cloned().flatten().unwrap_or_default();
        let def = row.get(1).cloned().flatten().unwrap_or_default();

        let is_unique = def.to_uppercase().contains("UNIQUE INDEX");
        let is_primary = name.contains("pkey");

        let index_type = if def.to_uppercase().contains("USING HASH") {
            "HASH".to_string()
        } else if def.to_uppercase().contains("USING GIST") {
            "GiST".to_string()
        } else if def.to_uppercase().contains("USING GIN") {
            "GIN".to_string()
        } else {
            "BTREE".to_string()
        };

        // 从 indexdef 提取列名
        let columns = extract_pg_index_columns(&def);

        indexes.push(IndexInfo {
            name,
            columns,
            unique: is_unique,
            index_type,
            is_primary,
        });
    }
    Ok(indexes)
}

fn extract_pg_index_columns(def: &str) -> Vec<String> {
    if let Some(paren_start) = def.find('(') {
        let inside = &def[paren_start + 1..];
        if let Some(paren_end) = inside.rfind(')') {
            return inside[..paren_end]
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
        }
    }
    vec![]
}

async fn collect_postgres_row_estimate(
    conn: &DatabaseConnection,
    table_name: &str,
) -> Result<Option<u64>, ConnectorError> {
    let tn = pg_escape(table_name);
    let sql = format!(
        "SELECT (reltuples::bigint) || '' FROM pg_class WHERE relname = '{tn}'"
    );
    let result = conn.query(&sql, 1).await?;
    Ok(result.rows.first()
        .and_then(|row| row.first().cloned())
        .flatten()
        .and_then(|v| v.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pg_index_columns_single() {
        let def = "CREATE INDEX idx_email ON public.users USING btree (email)";
        let cols = extract_pg_index_columns(def);
        assert_eq!(cols, vec!["email"]);
    }

    #[test]
    fn test_extract_pg_index_columns_multi() {
        let def = "CREATE INDEX idx_multi ON public.orders USING btree (user_id, status, amount DESC)";
        let cols = extract_pg_index_columns(def);
        assert_eq!(cols, vec!["user_id", "status", "amount DESC"]);
    }

    #[test]
    fn test_extract_pg_index_columns_empty() {
        assert!(extract_pg_index_columns("no parens").is_empty());
    }
}
