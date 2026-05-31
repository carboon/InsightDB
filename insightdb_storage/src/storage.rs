use std::path::Path;
use serde::{Deserialize, Serialize};
use insightdb_advisor::DiagnosisReport;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("数据库错误: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("序列化错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("报告未找到: {0}")]
    NotFound(String),
}

/// 报告摘要（用于列表展示，避免加载完整 plan/schema）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub id: String,
    pub sql: String,
    pub db_type: String,
    pub database_name: String,
    pub overall_severity: String,
    pub total_findings: usize,
    pub summary: String,
    pub generated_at: String,
}

/// 本地 SQLite 报告存储
pub struct ReportStorage {
    conn: rusqlite::Connection,
}

impl ReportStorage {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        let conn = rusqlite::Connection::open(path)?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    /// 在内存中创建（用于测试）
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = rusqlite::Connection::open_in_memory()?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    fn init_schema(&self) -> Result<(), StorageError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS reports (
                id              TEXT PRIMARY KEY,
                sql             TEXT NOT NULL,
                db_type         TEXT NOT NULL,
                db_version      TEXT NOT NULL,
                database_name   TEXT NOT NULL,
                overall_severity TEXT NOT NULL,
                total_findings  INTEGER NOT NULL,
                summary         TEXT NOT NULL,
                generated_at    TEXT NOT NULL,
                report_json     TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_reports_generated_at ON reports(generated_at);
            CREATE INDEX IF NOT EXISTS idx_reports_severity ON reports(overall_severity);",
        )?;
        Ok(())
    }

    /// 保存报告，返回生成的 id
    pub fn save(&self, report: &DiagnosisReport) -> Result<String, StorageError> {
        let id = format!("rpt_{}", uuid::Uuid::new_v4());
        let json = serde_json::to_string(report)?;

        self.conn.execute(
            "INSERT INTO reports (id, sql, db_type, db_version, database_name,
                                  overall_severity, total_findings, summary,
                                  generated_at, report_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            (
                &id,
                &report.sql,
                &report.db_type,
                &report.db_version,
                &report.database_name,
                format!("{:?}", report.overall_severity),
                report.total_findings,
                &report.summary,
                &report.generated_at,
                &json,
            ),
        )?;

        Ok(id)
    }

    /// 获取报告列表（按时间倒序）
    pub fn list(&self) -> Result<Vec<ReportSummary>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sql, db_type, database_name, overall_severity,
                    total_findings, summary, generated_at
             FROM reports ORDER BY generated_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(ReportSummary {
                id: row.get(0)?,
                sql: row.get(1)?,
                db_type: row.get(2)?,
                database_name: row.get(3)?,
                overall_severity: row.get(4)?,
                total_findings: row.get(5)?,
                summary: row.get(6)?,
                generated_at: row.get(7)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// 获取完整报告
    pub fn get(&self, id: &str) -> Result<DiagnosisReport, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT report_json FROM reports WHERE id = ?1",
        )?;

        let json: String = stmt
            .query_row([id], |row| row.get(0))
            .map_err(|_| StorageError::NotFound(id.into()))?;

        let report: DiagnosisReport = serde_json::from_str(&json)?;
        Ok(report)
    }

    /// 删除报告
    pub fn delete(&self, id: &str) -> Result<(), StorageError> {
        let affected = self.conn.execute("DELETE FROM reports WHERE id = ?1", [id])?;
        if affected == 0 {
            return Err(StorageError::NotFound(id.into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insightdb_rules::{RuleFinding, Severity};
    use insightdb_explain::PlanNode;
    use insightdb_catalog::SchemaInfo;

    fn make_finding(id: &str, severity: Severity) -> RuleFinding {
        RuleFinding {
            id: id.into(),
            severity,
            title: format!("Test {id}"),
            evidence: format!("Evidence for {id}"),
            recommendation: "Fix it".into(),
            risk: "Bad".into(),
            verification: "Check".into(),
            confidence: 0.9,
        }
    }

    fn make_report(sql: &str, findings_count: usize) -> DiagnosisReport {
        let findings: Vec<RuleFinding> = (0..findings_count)
            .map(|i| make_finding(&format!("RULE_{i}"), Severity::High))
            .collect();
        DiagnosisReport::new(
            sql,
            "mysql",
            "8.0.30",
            "testdb",
            findings,
            PlanNode::leaf("Seq Scan"),
            SchemaInfo {
                db_type: "mysql".into(),
                version: "8.0.30".into(),
                database_name: "testdb".into(),
                tables: vec![],
                collected_at: "".into(),
            },
        )
    }

    #[test]
    fn test_save_and_list() {
        let storage = ReportStorage::open_in_memory().unwrap();
        let report = make_report("SELECT * FROM users", 2);
        let id = storage.save(&report).unwrap();

        let list = storage.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].sql, "SELECT * FROM users");
        assert_eq!(list[0].total_findings, 2);
    }

    #[test]
    fn test_get_full_report() {
        let storage = ReportStorage::open_in_memory().unwrap();
        let report = make_report("SELECT 1", 1);
        let id = storage.save(&report).unwrap();

        let loaded = storage.get(&id).unwrap();
        assert_eq!(loaded.sql, "SELECT 1");
        assert_eq!(loaded.findings.len(), 1);
    }

    #[test]
    fn test_delete_report() {
        let storage = ReportStorage::open_in_memory().unwrap();
        let report = make_report("SELECT 1", 0);
        let id = storage.save(&report).unwrap();

        storage.delete(&id).unwrap();
        assert!(storage.list().unwrap().is_empty());
    }

    #[test]
    fn test_delete_nonexistent() {
        let storage = ReportStorage::open_in_memory().unwrap();
        let result = storage.delete("no_such_id");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_nonexistent() {
        let storage = ReportStorage::open_in_memory().unwrap();
        let result = storage.get("no_such_id");
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_reports_ordered_by_time() {
        let storage = ReportStorage::open_in_memory().unwrap();
        let r1 = make_report("SELECT 1", 1);
        let r2 = make_report("SELECT 2", 2);

        let id1 = storage.save(&r1).unwrap();
        // Small sleep to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(10));
        let id2 = storage.save(&r2).unwrap();

        let list = storage.list().unwrap();
        assert_eq!(list.len(), 2);
        // Most recent first
        assert_eq!(list[0].id, id2);
        assert_eq!(list[1].id, id1);
    }

    #[test]
    fn test_severity_stored_correctly() {
        let storage = ReportStorage::open_in_memory().unwrap();
        let finding = make_finding("HIGH_RULE", Severity::High);
        let report = DiagnosisReport::new(
            "SELECT 1",
            "mysql",
            "8.0",
            "testdb",
            vec![finding],
            PlanNode::leaf("Seq Scan"),
            SchemaInfo {
                db_type: "mysql".into(),
                version: "8.0".into(),
                database_name: "testdb".into(),
                tables: vec![],
                collected_at: "".into(),
            },
        );

        storage.save(&report).unwrap();
        let list = storage.list().unwrap();
        assert_eq!(list[0].overall_severity, "High");
    }
}
