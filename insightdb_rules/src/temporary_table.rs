use crate::models::*;
use insightdb_explain::PlanNode;
use insightdb_catalog::SchemaInfo;

pub fn check_temporary_table(plan: &PlanNode, _schema: &SchemaInfo) -> Vec<RuleFinding> {
    let mut findings = Vec::new();

    for node in plan.walk() {
        if !node.uses_temporary {
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
            id: "TEMPORARY_TABLE".into(),
            severity,
            title: "临时表使用".into(),
            evidence: format!(
                "表 `{table}` 的查询使用了临时表，估算 {rows} 行。常见于 GROUP BY、DISTINCT、UNION 等操作"
            ),
            recommendation: format!(
                "优化查询：①确保 GROUP BY 字段有索引利用其天然顺序；②检查是否可以拆分子查询减少中间结果；③对 DISTINCT 考虑使用 EXISTS 替代"
            ),
            risk: "临时表在数据量大时占用大量内存或磁盘空间，可能导致 OOM 或磁盘 I/O 瓶颈".into(),
            verification: "创建覆盖 GROUP BY / DISTINCT 列的索引后重新 EXPLAIN，确认 using_temporary_table 消失".into(),
            confidence: 0.85,
        });
    }

    findings
}
