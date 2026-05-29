use crate::models::*;
use insightdb_explain::PlanNode;
use insightdb_catalog::SchemaInfo;

pub fn check_missing_index(plan: &PlanNode, schema: &SchemaInfo) -> Vec<RuleFinding> {
    let mut findings = Vec::new();

    for node in plan.walk() {
        let is_seq_scan = matches!(
            node.access_method.as_deref(),
            Some("seq_scan") | Some("ALL")
        );

        if !is_seq_scan {
            continue;
        }

        let has_filter = node.filter.is_some();
        let table = node.table_name.as_deref().unwrap_or("(unknown)");
        let table_info = schema.tables.iter().find(|t| t.name == table);
        let has_indexes = table_info.map(|t| !t.indexes.is_empty()).unwrap_or(false);

        if has_filter && !has_indexes {
            let rows = node.estimated_rows.unwrap_or(0);
            let severity = if rows >= 100_000 { Severity::High } else { Severity::Medium };

            findings.push(RuleFinding {
                id: "MISSING_INDEX".into(),
                severity,
                title: "索引缺失".into(),
                evidence: format!(
                    "表 `{table}` 存在过滤条件 `{}` 但执行了全表扫描，且表上无可用索引",
                    node.filter.as_deref().unwrap_or("")
                ),
                recommendation: format!(
                    "为表 `{table}` 中过滤条件涉及的列创建索引。优先考虑选择性和查询频率最高的列"
                ),
                risk: "无索引的过滤查询在数据增长后每次都需要全表扫描，导致性能持续下降".into(),
                verification: format!(
                    "对 `{table}` 创建索引后，使用 `EXPLAIN` 验证 access_type 变为 RANGE 或 REF"
                ),
                confidence: 0.85,
            });
        }
    }

    findings
}
