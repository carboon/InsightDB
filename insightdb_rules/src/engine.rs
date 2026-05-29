use crate::models::*;
use insightdb_explain::PlanNode;
use insightdb_catalog::SchemaInfo;

use crate::full_table_scan::check_full_table_scan;
use crate::missing_index::check_missing_index;
use crate::filesort::check_filesort;
use crate::temporary_table::check_temporary_table;
use crate::nested_loop_risk::check_nested_loop_risk;
use crate::abnormal_scan_rows::check_abnormal_scan_rows;

pub type RuleFn = fn(&PlanNode, &SchemaInfo) -> Vec<RuleFinding>;

pub fn all_rules() -> Vec<RuleFn> {
    vec![
        check_full_table_scan,
        check_missing_index,
        check_filesort,
        check_temporary_table,
        check_nested_loop_risk,
        check_abnormal_scan_rows,
    ]
}

pub fn run_rules(plan: &PlanNode, schema: &SchemaInfo) -> Vec<RuleFinding> {
    let mut findings = Vec::new();
    for rule in all_rules() {
        findings.extend(rule(plan, schema));
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use insightdb_explain::PlanNode;
    use insightdb_catalog::SchemaInfo;
    use crate::models::Severity;

    fn empty_schema() -> SchemaInfo {
        SchemaInfo {
            db_type: "mysql".into(),
            version: "8.0.0".into(),
            database_name: "test".into(),
            tables: vec![],
            collected_at: "2024-01-01T00:00:00Z".into(),
        }
    }

    fn schema_with_table(name: &str, row_count: u64) -> SchemaInfo {
        SchemaInfo {
            db_type: "mysql".into(),
            version: "8.0.0".into(),
            database_name: "test".into(),
            tables: vec![
                insightdb_catalog::TableInfo {
                    name: name.into(),
                    table_type: "BASE TABLE".into(),
                    engine: Some("InnoDB".into()),
                    row_count_estimate: Some(row_count),
                    columns: vec![],
                    indexes: vec![],
                }
            ],
            collected_at: "2024-01-01T00:00:00Z".into(),
        }
    }

    // ── Full Table Scan ──

    #[test]
    fn test_full_table_scan_on_large_table() {
        let plan = PlanNode {
            node_type: "Table Scan".into(),
            table_name: Some("users".into()),
            access_method: Some("seq_scan".into()),
            estimated_rows: Some(50000),
            ..PlanNode::leaf("Table Scan")
        };
        let schema = schema_with_table("users", 50000);
        let findings = check_full_table_scan(&plan, &schema);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].id, "FULL_TABLE_SCAN");
        assert!(matches!(findings[0].severity, Severity::High | Severity::Medium));
        assert!(findings[0].evidence.contains("users"));
    }

    #[test]
    fn test_full_table_scan_on_small_table_ok() {
        let plan = PlanNode {
            node_type: "Table Scan".into(),
            table_name: Some("config".into()),
            access_method: Some("seq_scan".into()),
            estimated_rows: Some(10),
            ..PlanNode::leaf("Table Scan")
        };
        let schema = schema_with_table("config", 10);
        let findings = check_full_table_scan(&plan, &schema);
        assert!(findings.is_empty(), "小表全表扫描不应告警");
    }

    #[test]
    fn test_full_table_scan_no_table_name() {
        let plan = PlanNode {
            node_type: "Table Scan".into(),
            access_method: Some("seq_scan".into()),
            estimated_rows: Some(100000),
            ..PlanNode::leaf("Table Scan")
        };
        let schema = empty_schema();
        let findings = check_full_table_scan(&plan, &schema);
        assert!(!findings.is_empty());
    }

    // ── Missing Index ──

    #[test]
    fn test_missing_index_with_filter() {
        let plan = PlanNode {
            node_type: "Table Scan".into(),
            table_name: Some("orders".into()),
            access_method: Some("seq_scan".into()),
            estimated_rows: Some(10000),
            filter: Some("(`orders`.`status` = 'pending')".into()),
            ..PlanNode::leaf("Table Scan")
        };
        let schema = schema_with_table("orders", 10000);
        let findings = check_missing_index(&plan, &schema);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].id, "MISSING_INDEX");
        assert!(findings[0].title.contains("索引缺失"));
    }

    #[test]
    fn test_no_missing_index_when_index_used() {
        let plan = PlanNode {
            node_type: "Index Scan".into(),
            table_name: Some("orders".into()),
            access_method: Some("index_scan".into()),
            index_name: Some("idx_status".into()),
            estimated_rows: Some(10000),
            filter: Some("(`orders`.`status` = 'pending')".into()),
            ..PlanNode::leaf("Index Scan")
        };
        let schema = schema_with_table("orders", 10000);
        let findings = check_missing_index(&plan, &schema);
        assert!(findings.is_empty(), "使用索引时不应报告索引缺失");
    }

    // ── Filesort ──

    #[test]
    fn test_filesort_detected() {
        let plan = PlanNode {
            node_type: "Table Scan".into(),
            table_name: Some("logs".into()),
            access_method: Some("seq_scan".into()),
            estimated_rows: Some(50000),
            uses_filesort: true,
            ..PlanNode::leaf("Table Scan")
        };
        let schema = schema_with_table("logs", 50000);
        let findings = check_filesort(&plan, &schema);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].id, "FILESORT");
    }

    #[test]
    fn test_no_filesort_when_not_present() {
        let plan = PlanNode {
            node_type: "Index Scan".into(),
            table_name: Some("logs".into()),
            access_method: Some("index_scan".into()),
            estimated_rows: Some(50000),
            uses_filesort: false,
            ..PlanNode::leaf("Index Scan")
        };
        let schema = schema_with_table("logs", 50000);
        let findings = check_filesort(&plan, &schema);
        assert!(findings.is_empty());
    }

    // ── Temporary Table ──

    #[test]
    fn test_temporary_table_detected() {
        let plan = PlanNode {
            node_type: "Table Scan".into(),
            table_name: Some("events".into()),
            access_method: Some("seq_scan".into()),
            estimated_rows: Some(200000),
            uses_temporary: true,
            ..PlanNode::leaf("Table Scan")
        };
        let schema = schema_with_table("events", 200000);
        let findings = check_temporary_table(&plan, &schema);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].id, "TEMPORARY_TABLE");
    }

    #[test]
    fn test_no_temporary_table_when_not_present() {
        let plan = PlanNode::leaf("Index Scan");
        let schema = empty_schema();
        let findings = check_temporary_table(&plan, &schema);
        assert!(findings.is_empty());
    }

    // ── Nested Loop Risk ──

    #[test]
    fn test_nested_loop_risk_high() {
        let plan = PlanNode {
            node_type: "Nested Loop".into(),
            join_type: Some("Inner".into()),
            children: vec![
                PlanNode {
                    node_type: "Seq Scan".into(),
                    table_name: Some("orders".into()),
                    estimated_rows: Some(2_000_000),
                    ..PlanNode::leaf("Seq Scan")
                },
                PlanNode {
                    node_type: "Index Scan".into(),
                    table_name: Some("users".into()),
                    index_name: Some("PRIMARY".into()),
                    estimated_rows: Some(1),
                    ..PlanNode::leaf("Index Scan")
                },
            ],
            ..PlanNode::leaf("Nested Loop")
        };
        let schema = empty_schema();
        let findings = check_nested_loop_risk(&plan, &schema);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].id, "NESTED_LOOP_RISK");
        assert!(matches!(findings[0].severity, Severity::High));
    }

    #[test]
    fn test_nested_loop_risk_low_on_small_tables() {
        let plan = PlanNode {
            node_type: "Nested Loop".into(),
            join_type: Some("Inner".into()),
            children: vec![
                PlanNode {
                    node_type: "Seq Scan".into(),
                    table_name: Some("a".into()),
                    estimated_rows: Some(10),
                    ..PlanNode::leaf("Seq Scan")
                },
                PlanNode {
                    node_type: "Index Scan".into(),
                    table_name: Some("b".into()),
                    estimated_rows: Some(1),
                    ..PlanNode::leaf("Index Scan")
                },
            ],
            ..PlanNode::leaf("Nested Loop")
        };
        let schema = empty_schema();
        let findings = check_nested_loop_risk(&plan, &schema);
        assert!(findings.is_empty(), "小表 Nested Loop 不应高风险告警");
    }

    // ── Abnormal Scan Rows ──

    #[test]
    fn test_abnormal_scan_rows() {
        let plan = PlanNode {
            node_type: "Table Scan".into(),
            table_name: Some("big_table".into()),
            access_method: Some("seq_scan".into()),
            estimated_rows: Some(500000),
            filter: Some("(id = 1)".into()),
            ..PlanNode::leaf("Table Scan")
        };
        let schema = schema_with_table("big_table", 1000000);
        let findings = check_abnormal_scan_rows(&plan, &schema);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].id, "ABNORMAL_SCAN_ROWS");
    }

    #[test]
    fn test_normal_scan_rows_no_warning() {
        let plan = PlanNode {
            node_type: "Table Scan".into(),
            table_name: Some("small".into()),
            estimated_rows: Some(100),
            ..PlanNode::leaf("Table Scan")
        };
        let schema = schema_with_table("small", 200);
        let findings = check_abnormal_scan_rows(&plan, &schema);
        assert!(findings.is_empty());
    }

    // ── Rule finding structure ──

    #[test]
    fn test_rule_finding_has_all_required_fields() {
        let plan = PlanNode {
            node_type: "Table Scan".into(),
            table_name: Some("t".into()),
            access_method: Some("seq_scan".into()),
            estimated_rows: Some(100000),
            ..PlanNode::leaf("Table Scan")
        };
        let schema = schema_with_table("t", 100000);
        let findings = run_rules(&plan, &schema);
        for f in &findings {
            assert!(!f.id.is_empty());
            assert!(!f.title.is_empty());
            assert!(!f.evidence.is_empty());
            assert!(!f.recommendation.is_empty());
            assert!(!f.risk.is_empty());
            assert!(!f.verification.is_empty());
            assert!(f.confidence >= 0.0 && f.confidence <= 1.0);
        }
    }
}
