use crate::models::*;
use insightdb_explain::PlanNode;
use insightdb_catalog::SchemaInfo;

const NL_HIGH_RISK_THRESHOLD: u64 = 10_000;

pub fn check_nested_loop_risk(plan: &PlanNode, _schema: &SchemaInfo) -> Vec<RuleFinding> {
    let mut findings = Vec::new();

    for node in plan.walk() {
        if node.node_type != "Nested Loop" && node.node_type != "Nested Loop Join" {
            continue;
        }

        if node.children.len() < 2 {
            continue;
        }

        let outer_rows = node.children.first().and_then(|c| c.estimated_rows).unwrap_or(0);
        let inner_rows = node.children.get(1).and_then(|c| c.estimated_rows).unwrap_or(0);
        let inner_has_index = node.children.get(1)
            .map(|c| c.index_name.is_some())
            .unwrap_or(false);
        let total_estimated = outer_rows.saturating_mul(inner_rows);

        if total_estimated >= NL_HIGH_RISK_THRESHOLD {
            let outer_table = node.children.first()
                .and_then(|c| c.table_name.as_deref())
                .unwrap_or("?");
            let inner_table = node.children.get(1)
                .and_then(|c| c.table_name.as_deref())
                .unwrap_or("?");

            let severity = if total_estimated >= 1_000_000 {
                Severity::High
            } else if total_estimated >= 100_000 {
                Severity::Medium
            } else {
                Severity::Low
            };

            let extra_note = if !inner_has_index {
                " 内侧表当前无有效索引，每行外表都需要扫描整个内侧表。"
            } else {
                " 内侧表已有索引，但数据量仍值得关注。"
            };

            findings.push(RuleFinding {
                id: "NESTED_LOOP_RISK".into(),
                severity,
                title: "Nested Loop 连接风险".into(),
                evidence: format!(
                    "Nested Loop 连接：外表 `{outer_table}` 估算 {outer_rows} 行 \
                     内表 `{inner_table}` 估算 {inner_rows} 行，\
                     总机会约 {total_estimated} 次内表查找。{extra_note}"
                ),
                recommendation: if !inner_has_index {
                    format!("为内表 `{inner_table}` 的连接列创建索引，使优化器可以选择 Index Nested Loop")
                } else {
                    "考虑增大 work_mem 或使用 Hash Join。检查是否可以缩小外表的过滤条件".into()
                },
                risk: "Nested Loop 在内表无索引时会导致极大的性能开销（每行外表 → 全表扫描内表）".into(),
                verification: "为连接列创建索引后重新 EXPLAIN，确认内表有 Index Cond".into(),
                confidence: 0.8,
            });
        }
    }

    findings
}
