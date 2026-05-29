use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};
use insightdb_connector::{ConnectorConfig, DatabaseConnection, DatabaseKind};
use insightdb_catalog::{collect_schema, collect_version};
use insightdb_explain::explain;
use insightdb_rules::run_rules;
use insightdb_advisor::DiagnosisReport;
use insightdb_ai::{Sanitizer, PromptBuilder, SanitizedContext, AiExplanation, AiClient};

/// 全局连接状态（Tauri managed state）
pub struct AppState {
    pub connection: Mutex<Option<Arc<DatabaseConnection>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            connection: Mutex::new(None),
        }
    }
}

// ── 请求/响应类型 ──

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionRequest {
    pub url: String,
    pub read_only: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub db_type: String,
    pub version: String,
    pub database: String,
    pub connected: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueryRequest {
    pub sql: String,
    pub fetch_size: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QueryResultDto {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    pub affected_rows: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiagnosisRequest {
    pub sql: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiExplainRequest {
    pub report_json: String,
}

// ── Tauri Commands ──

#[tauri::command]
pub async fn connect(
    state: tauri::State<'_, AppState>,
    req: ConnectionRequest,
) -> Result<ConnectionInfo, String> {
    let config = ConnectorConfig::from_url(&req.url).map_err(|e| format!("URL 解析失败: {e}"))?;
    let read_only = req.read_only.unwrap_or(true);

    let mut config = config;
    if !read_only {
        config.read_only = false;
    }

    let conn = DatabaseConnection::connect(config)
        .await
        .map_err(|e| format!("连接失败: {e}"))?;

    let version = collect_version(&conn).await.unwrap_or_else(|_| "unknown".into());
    let db_type = match conn.database_kind() {
        DatabaseKind::MySQL => "mysql".to_string(),
        DatabaseKind::PostgreSQL => "postgresql".to_string(),
    };

    let info = ConnectionInfo {
        db_type,
        version,
        database: conn.config().database.clone(),
        connected: true,
    };

    let mut guard = state.connection.lock().await;
    *guard = Some(Arc::new(conn));

    Ok(info)
}

#[tauri::command]
pub async fn disconnect(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut guard = state.connection.lock().await;
    if let Some(conn) = guard.take() {
        conn.close().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn execute_query(
    state: tauri::State<'_, AppState>,
    req: QueryRequest,
) -> Result<QueryResultDto, String> {
    let guard = state.connection.lock().await;
    let conn = guard.as_ref().ok_or("未连接数据库")?;

    let fetch_size = req.fetch_size.unwrap_or(100);
    let result = conn.query(&req.sql, fetch_size)
        .await
        .map_err(|e| format!("查询失败: {e}"))?;

    Ok(QueryResultDto {
        columns: result.columns,
        rows: result.rows,
        affected_rows: result.affected_rows,
    })
}

#[tauri::command]
pub async fn cancel_query(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let guard = state.connection.lock().await;
    let conn = guard.as_ref().ok_or("未连接数据库")?;

    let canceller = conn.canceller();
    canceller.cancel().await.map_err(|e| format!("取消失败: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn diagnose(
    state: tauri::State<'_, AppState>,
    req: DiagnosisRequest,
) -> Result<DiagnosisReport, String> {
    let guard = state.connection.lock().await;
    let conn = guard.as_ref().ok_or("未连接数据库")?;

    let schema = collect_schema(conn).await.map_err(|e| format!("元数据采集失败: {e}"))?;
    let plan = explain(conn, &req.sql).await.map_err(|e| format!("EXPLAIN 失败: {e}"))?;
    let findings = run_rules(&plan, &schema);
    let version = schema.version.clone();

    let report = DiagnosisReport::new(
        req.sql,
        schema.db_type.clone(),
        version,
        schema.database_name.clone(),
        findings,
        plan,
        schema,
    );

    Ok(report)
}

#[tauri::command]
pub async fn ai_explain(
    req: AiExplainRequest,
) -> Result<AiExplanation, String> {
    let report: DiagnosisReport = serde_json::from_str(&req.report_json)
        .map_err(|e| format!("报告解析失败: {e}"))?;

    let mut sanitizer = Sanitizer::new();
    let ctx = sanitizer.sanitize(&report);

    let builder = PromptBuilder::default();
    let prompt = builder.build(&ctx);

    let client = insightdb_ai::MockAiClient::new("insightdb-mock");
    client.explain(&prompt).await.map_err(|e| format!("AI 调用失败: {e}"))
}

#[tauri::command]
pub async fn sanitize_context(
    report_json: String,
) -> Result<SanitizedContext, String> {
    let report: DiagnosisReport = serde_json::from_str(&report_json)
        .map_err(|e| format!("报告解析失败: {e}"))?;

    let mut sanitizer = Sanitizer::new();
    Ok(sanitizer.sanitize(&report))
}

#[tauri::command]
pub async fn ping(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let guard = state.connection.lock().await;
    let conn = guard.as_ref().ok_or("未连接数据库")?;
    conn.ping().await.map_err(|e| format!("ping 失败: {e}"))?;
    Ok("pong".into())
}
