use serde::{Deserialize, Serialize};
use insightdb_rules::{RuleFinding, Severity};
use insightdb_explain::PlanNode;
use insightdb_catalog::SchemaInfo;

/// 诊断报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosisReport {
    /// 被诊断的原始 SQL
    pub sql: String,

    /// 数据库类型
    pub db_type: String,

    /// 数据库版本
    pub db_version: String,

    /// 数据库名
    pub database_name: String,

    /// 诊断发现（按严重级别排序）
    pub findings: Vec<RuleFinding>,

    /// 执行计划快照
    pub plan: PlanNode,

    /// Schema 快照
    pub schema: SchemaInfo,

    /// 报告生成时间
    pub generated_at: String,

    /// 摘要
    pub summary: String,

    /// 整体严重级别
    pub overall_severity: Severity,

    /// 总找到问题数
    pub total_findings: usize,
}

impl DiagnosisReport {
    pub fn new(
        sql: impl Into<String>,
        db_type: impl Into<String>,
        db_version: impl Into<String>,
        database_name: impl Into<String>,
        mut findings: Vec<RuleFinding>,
        plan: PlanNode,
        schema: SchemaInfo,
    ) -> Self {
        // 去重和排序
        findings.sort_by(|a, b| {
            a.severity.cmp(&b.severity)
                .then_with(|| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal))
        });
        findings.dedup_by(|a, b| a.id == b.id && a.evidence == b.evidence);

        let overall = findings.first().map(|f| f.severity.clone()).unwrap_or(Severity::Info);
        let total = findings.len();
        let summary = build_summary(&findings, &overall);

        Self {
            sql: sql.into(),
            db_type: db_type.into(),
            db_version: db_version.into(),
            database_name: database_name.into(),
            findings,
            plan,
            schema,
            generated_at: chrono::Utc::now().to_rfc3339(),
            summary,
            overall_severity: overall,
            total_findings: total,
        }
    }
}

fn build_summary(findings: &[RuleFinding], overall: &Severity) -> String {
    if findings.is_empty() {
        return "未发现性能问题，执行计划正常。".into();
    }

    let mut parts = Vec::new();
    let high = findings.iter().filter(|f| f.severity <= Severity::High).count();
    let med = findings.iter().filter(|f| f.severity == Severity::Medium).count();
    let low = findings.iter().filter(|f| f.severity >= Severity::Low).count();

    if high > 0 {
        parts.push(format!("发现 {high} 个高危问题"));
    }
    if med > 0 {
        parts.push(format!("{med} 个中危问题"));
    }
    if low > 0 && high + med > 0 {
        parts.push(format!("{low} 个低危提示"));
    } else if low > 0 {
        parts.push(format!("发现 {low} 个低危提示"));
    }

    match overall {
        Severity::Critical | Severity::High => {
            parts.push("建议优先处理高危问题后再进行后续优化。".into());
        }
        Severity::Medium => {
            parts.push("建议根据业务影响排期处理。".into());
        }
        _ => {}
    }

    parts.join("，")
}

#[cfg(test)]
mod tests {
    use super::*;
    use insightdb_rules::Severity;

    fn make_finding(id: &str, severity: Severity) -> RuleFinding {
        RuleFinding {
            id: id.into(),
            severity,
            title: format!("Test {id}"),
            evidence: format!("Evidence for {id}"),
            recommendation: "Do something".into(),
            risk: "Risk".into(),
            verification: "Verify".into(),
            confidence: 0.8,
        }
    }

    fn empty_plan() -> PlanNode {
        PlanNode::leaf("Seq Scan")
    }

    fn empty_schema() -> SchemaInfo {
        SchemaInfo {
            db_type: "mysql".into(),
            version: "8.0.0".into(),
            database_name: "test".into(),
            tables: vec![],
            collected_at: "".into(),
        }
    }

    #[test]
    fn test_report_with_no_findings() {
        let report = DiagnosisReport::new(
            "SELECT 1", "mysql", "8.0.0", "test",
            vec![], empty_plan(), empty_schema(),
        );
        assert_eq!(report.total_findings, 0);
        assert_eq!(report.overall_severity, Severity::Info);
        assert!(report.summary.contains("未发现"));
    }

    #[test]
    fn test_report_sorts_by_severity() {
        let findings = vec![
            make_finding("LOW", Severity::Low),
            make_finding("HIGH", Severity::High),
            make_finding("MED", Severity::Medium),
        ];
        let report = DiagnosisReport::new(
            "SELECT 1", "mysql", "8.0", "test",
            findings, empty_plan(), empty_schema(),
        );
        assert_eq!(report.findings[0].severity, Severity::High);
        assert_eq!(report.findings[1].severity, Severity::Medium);
        assert_eq!(report.findings[2].severity, Severity::Low);
        assert_eq!(report.overall_severity, Severity::High);
    }

    #[test]
    fn test_report_deduplicates_same_findings() {
        let f = make_finding("DUP", Severity::High);
        let findings = vec![
            f.clone(),
            f.clone(),
            make_finding("OTHER", Severity::Low),
        ];
        let report = DiagnosisReport::new(
            "SELECT 1", "mysql", "8.0", "test",
            findings, empty_plan(), empty_schema(),
        );
        assert_eq!(report.total_findings, 2, "重复的 finding 应被去重");
    }

    #[test]
    fn test_report_serialization() {
        let report = DiagnosisReport::new(
            "SELECT * FROM users WHERE age > 18",
            "mysql",
            "8.0.30",
            "mydb",
            vec![
                make_finding("FULL_TABLE_SCAN", Severity::High),
                make_finding("MISSING_INDEX", Severity::Medium),
            ],
            empty_plan(),
            empty_schema(),
        );
        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("FULL_TABLE_SCAN"));
        assert!(json.contains("mydb"));
        let parsed: DiagnosisReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.total_findings, 2);
    }

    #[test]
    fn test_summary_with_high_medium_low() {
        let findings = vec![
            make_finding("a", Severity::High),
            make_finding("b", Severity::High),
            make_finding("c", Severity::Medium),
            make_finding("d", Severity::Low),
        ];
        let report = DiagnosisReport::new(
            "SELECT 1", "mysql", "8.0", "test",
            findings, empty_plan(), empty_schema(),
        );
        assert!(report.summary.contains("2 个高危"));
        assert!(report.summary.contains("1 个中危"));
    }

    #[test]
    fn test_overall_severity_reflects_worst() {
        let findings = vec![
            make_finding("a", Severity::Low),
            make_finding("b", Severity::Low),
        ];
        let report = DiagnosisReport::new(
            "SELECT 1", "mysql", "8.0", "test",
            findings, empty_plan(), empty_schema(),
        );
        assert_eq!(report.overall_severity, Severity::Low);
    }
}
