use insightdb_connector::{DatabaseConnection, DatabaseKind};
use crate::{mysql_parser, postgres_parser, plan::PlanNode, ParseError};

/// 对目标 SQL 执行 EXPLAIN，返回解析后的统一执行计划
pub async fn explain(conn: &DatabaseConnection, sql: &str) -> Result<PlanNode, ParseError> {
    match conn.database_kind() {
        DatabaseKind::MySQL => {
            let explain_sql = format!("EXPLAIN FORMAT=JSON {sql}");
            let result = conn.query(&explain_sql, 5).await
                .map_err(|e| ParseError::ExecutionFailed(format!("执行 EXPLAIN 失败: {e}")))?;
            let json_str = result.rows.first()
                .and_then(|row| row.first())
                .and_then(|v| v.as_deref().map(|s| s.to_string()))
                .ok_or_else(|| ParseError::InvalidFormat("EXPLAIN 未返回任何数据".into()))?;
            mysql_parser::parse_mysql_json(&json_str)
        }
        DatabaseKind::PostgreSQL => {
            explain_postgres(conn, sql).await
        }
    }
}

/// 对目标 SQL 执行 EXPLAIN ANALYZE（仅 PostgreSQL 有效返回 actual_rows）
pub async fn explain_analyze(conn: &DatabaseConnection, sql: &str) -> Result<PlanNode, ParseError> {
    match conn.database_kind() {
        DatabaseKind::PostgreSQL => {
            explain_postgres_analyze(conn, sql).await
        }
        DatabaseKind::MySQL => {
            explain(conn, sql).await
        }
    }
}

/// 使用原生 PostgreSQL 驱动执行 EXPLAIN (FORMAT JSON)
async fn explain_postgres(conn: &DatabaseConnection, sql: &str) -> Result<PlanNode, ParseError> {
    use sqlx::Connection;
    use sqlx::Row;

    let cs = conn.connection_string();
    let mut pg_conn = sqlx::PgConnection::connect(&cs).await
        .map_err(|e| ParseError::ExecutionFailed(format!("PG 原生连接失败: {e}")))?;

    let explain_sql = format!("EXPLAIN (FORMAT JSON) {sql}");
    let row = sqlx::query(&explain_sql)
        .fetch_one(&mut pg_conn)
        .await
        .map_err(|e| ParseError::ExecutionFailed(format!("PG EXPLAIN 执行失败: {e}")))?;

    let json_val: serde_json::Value = row.try_get(0)
        .map_err(|e| ParseError::ExecutionFailed(format!("读取 EXPLAIN 结果失败: {e}")))?;
    let json_str = json_val.to_string();

    pg_conn.close().await.ok();
    postgres_parser::parse_postgres_json(&json_str)
}

/// 使用原生 PostgreSQL 驱动执行 EXPLAIN (FORMAT JSON, ANALYZE)
async fn explain_postgres_analyze(conn: &DatabaseConnection, sql: &str) -> Result<PlanNode, ParseError> {
    use sqlx::Connection;
    use sqlx::Row;

    let cs = conn.connection_string();
    let mut pg_conn = sqlx::PgConnection::connect(&cs).await
        .map_err(|e| ParseError::ExecutionFailed(format!("PG 原生连接失败: {e}")))?;

    let explain_sql = format!("EXPLAIN (FORMAT JSON, ANALYZE) {sql}");
    let row = sqlx::query(&explain_sql)
        .fetch_one(&mut pg_conn)
        .await
        .map_err(|e| ParseError::ExecutionFailed(format!("PG EXPLAIN ANALYZE 执行失败: {e}")))?;

    let json_val: serde_json::Value = row.try_get(0)
        .map_err(|e| ParseError::ExecutionFailed(format!("读取 EXPLAIN 结果失败: {e}")))?;
    let json_str = json_val.to_string();

    pg_conn.close().await.ok();
    postgres_parser::parse_postgres_json(&json_str)
}
