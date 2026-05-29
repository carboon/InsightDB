use crate::models::*;
use insightdb_explain::PlanNode;
use insightdb_catalog::SchemaInfo;

const FULL_TABLE_SCAN_THRESHOLD: u64 = 1000;

pub fn check_full_table_scan(plan: &PlanNode, schema: &SchemaInfo) -> Vec<RuleFinding> {
    let mut findings = Vec::new();

    for node in plan.walk() {
        let is_seq_scan = node.access_method.as_deref() == Some("seq_scan")
            || node.access_method.as_deref() == Some("ALL");
        let rows = node.estimated_rows.unwrap_or(0);

        if is_seq_scan && rows >= FULL_TABLE_SCAN_THRESHOLD {
            let table = node.table_name.as_deref().unwrap_or("(unknown)");
            let table_rows = schema.tables.iter()
                .find(|t| t.name == table)
                .and_then(|t| t.row_count_estimate)
                .unwrap_or(rows);

            let severity = if rows >= 100_000 {
                Severity::High
            } else if rows >= 10_000 {
                Severity::Medium
            } else {
                Severity::Low
            };

            findings.push(RuleFinding {
                id: "FULL_TABLE_SCAN".into(),
                severity,
                title: "全表扫描".into(),
                evidence: format!(
                    "表 `{table}` 执行全表扫描，估算行数 {rows}，表总行数约 {table_rows}"
                ),
                recommendation: format!(
                    "检查查询是否包含针对 `{table}` 的有效过滤条件（WHERE），为过滤列创建索引"
                ),
                risk: "全表扫描在数据量增大时导致线性性能下降，消耗大量 I/O 和 CPU".into(),
                verification: format!(
                    "为 `{table}` 的过滤列创建索引后重新 EXPLAIN，确认 access_type 变为 RANGE 或 REF"
                ),
                confidence: 0.95,
            });
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threshold_exact_boundary_below_1000() {
        let plan = PlanNode {
            node_type: "Table Scan".into(),
            table_name: Some("t".into()),
            access_method: Some("seq_scan".into()),
            estimated_rows: Some(999),
            ..PlanNode::leaf("Table Scan")
        };
        let schema = SchemaInfo {
            db_type: "mysql".into(),
            version: "8.0".into(),
            database_name: "test".into(),
            tables: vec![],
            collected_at: "".into(),
        };
        let findings = check_full_table_scan(&plan, &schema);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_threshold_at_boundary() {
        let plan = PlanNode {
            node_type: "Table Scan".into(),
            table_name: Some("t".into()),
            access_method: Some("seq_scan".into()),
            estimated_rows: Some(1000),
            ..PlanNode::leaf("Table Scan")
        };
        let schema = SchemaInfo {
            db_type: "mysql".into(),
            version: "8.0".into(),
            database_name: "test".into(),
            tables: vec![],
            collected_at: "".into(),
        };
        let findings = check_full_table_scan(&plan, &schema);
        assert_eq!(findings.len(), 1, "1000 行应触发告警");
    }
}
