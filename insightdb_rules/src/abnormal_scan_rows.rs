use crate::models::*;
use insightdb_explain::PlanNode;
use insightdb_catalog::SchemaInfo;

const ABNORMAL_SCAN_THRESHOLD: u64 = 100_000;

pub fn check_abnormal_scan_rows(plan: &PlanNode, _schema: &SchemaInfo) -> Vec<RuleFinding> {
    let mut findings = Vec::new();

    for node in plan.walk() {
        let rows = node.estimated_rows.unwrap_or(0);

        if rows < ABNORMAL_SCAN_THRESHOLD {
            continue;
        }

        let table = node.table_name.as_deref().unwrap_or("(unknown)");
        let access = node.access_method.as_deref().unwrap_or("?");

        findings.push(RuleFinding {
            id: "ABNORMAL_SCAN_ROWS".into(),
            severity: if rows >= 500_000 { Severity::High } else { Severity::Medium },
            title: "异常扫描行数".into(),
            evidence: format!(
                "表 `{table}` 估算扫描 {rows} 行 (access_type={access})，\
                 可能因统计信息过期导致优化器误判"
            ),
            recommendation: format!(
                "对表 `{table}` 执行 ANALYZE TABLE / ANALYZE 更新统计信息。\
                 如果查询确实需要扫描全表，检查是否有索引可以缩小扫描范围"
            ),
            risk: "过期的统计信息可能导致优化器选择不合理的执行计划（如不必要的全表扫描、错误的连接顺序）".into(),
            verification: "执行 ANALYZE 后重新 EXPLAIN 并对比估算行数的变化".into(),
            confidence: 0.7,
        });
    }

    findings
}
