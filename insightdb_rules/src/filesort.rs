use crate::models::*;
use insightdb_explain::PlanNode;
use insightdb_catalog::SchemaInfo;

pub fn check_filesort(plan: &PlanNode, _schema: &SchemaInfo) -> Vec<RuleFinding> {
    let mut findings = Vec::new();

    for node in plan.walk() {
        if !node.uses_filesort {
            continue;
        }

        let table = node.table_name.as_deref().unwrap_or("(unknown)");
        let rows = node.estimated_rows.unwrap_or(0);

        let severity = if rows >= 100_000 {
            Severity::High
        } else if rows >= 10_000 {
            Severity::Medium
        } else {
            Severity::Low
        };

        findings.push(RuleFinding {
            id: "FILESORT".into(),
            severity,
            title: "外部排序 (filesort)".into(),
            evidence: format!(
                "表 `{table}` 的查询触发了 filesort，估算 {rows} 行需要外部排序。排序无法利用现有索引的天然顺序"
            ),
            recommendation: format!(
                "检查 ORDER BY 列是否与 WHERE 条件中的列能组成联合索引。索引应覆盖过滤条件 + 排序列"
            ),
            risk: "filesort 在排序数据量大时需使用磁盘临时文件，显著增加查询延迟".into(),
            verification: "创建覆盖 WHERE + ORDER BY 的联合索引后，重新 EXPLAIN 确认 filesort 消失".into(),
            confidence: 0.9,
        });
    }

    findings
}
