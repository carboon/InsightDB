use serde::{Deserialize, Serialize};

/// 统一的执行计划节点模型
///
/// 将 MySQL `EXPLAIN FORMAT=JSON` 和 PostgreSQL `EXPLAIN (FORMAT JSON)` 的输出
/// 转换为统一结构，供规则引擎和诊断报告使用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanNode {
    /// 节点类型：Seq Scan、Index Scan、Index Only Scan、Nested Loop、Hash Join、Sort、Aggregate 等
    pub node_type: String,

    /// 目标表名（扫描节点）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_name: Option<String>,

    /// 表别名
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// 访问方法归类：seq_scan、index_scan、index_only_scan、index_range、eq_ref、bitmap 等
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_method: Option<String>,

    /// 使用的索引名
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_name: Option<String>,

    /// 索引条件（PostgreSQL Index Cond）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_cond: Option<String>,

    /// 估算行数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_rows: Option<u64>,

    /// 实际行数（EXPLAIN ANALYZE 可用时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_rows: Option<u64>,

    /// 总成本估算
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cost: Option<f64>,

    /// 过滤条件（WHERE 子句中针对本节点的条件）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,

    /// 连接类型（Inner、Left、Right、Semi、Anti）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join_type: Option<String>,

    /// 哈希连接条件（PostgreSQL Hash Cond）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_cond: Option<String>,

    /// 排序键
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort_keys: Vec<String>,

    /// 是否使用临时表（MySQL）
    #[serde(default)]
    pub uses_temporary: bool,

    /// 是否使用 filesort（MySQL）
    #[serde(default)]
    pub uses_filesort: bool,

    /// 是否并行执行
    #[serde(default)]
    pub parallel_aware: bool,

    /// Nested Loop 内侧是否唯一（PostgreSQL Inner Unique）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inner_unique: Option<bool>,

    /// 子节点
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PlanNode>,

    /// 保留的原始数据（审计/调试用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

impl PlanNode {
    /// 创建叶子节点（无子节点）
    pub fn leaf(node_type: impl Into<String>) -> Self {
        PlanNode {
            node_type: node_type.into(),
            table_name: None,
            alias: None,
            access_method: None,
            index_name: None,
            index_cond: None,
            estimated_rows: None,
            actual_rows: None,
            total_cost: None,
            filter: None,
            join_type: None,
            hash_cond: None,
            sort_keys: vec![],
            uses_temporary: false,
            uses_filesort: false,
            parallel_aware: false,
            inner_unique: None,
            children: vec![],
            extra: None,
        }
    }

    /// 遍历所有节点（深度优先）
    pub fn walk(&self) -> Vec<&PlanNode> {
        let mut nodes = vec![self];
        for child in &self.children {
            nodes.extend(child.walk());
        }
        nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_node_serialization() {
        let node = PlanNode {
            node_type: "Seq Scan".into(),
            table_name: Some("users".into()),
            access_method: Some("seq_scan".into()),
            estimated_rows: Some(1000),
            total_cost: Some(35.5),
            filter: Some("(age > 18)".into()),
            ..PlanNode::leaf("Seq Scan")
        };

        let json = serde_json::to_string(&node).unwrap();
        let deserialized: PlanNode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.node_type, "Seq Scan");
        assert_eq!(deserialized.table_name, Some("users".into()));
        assert_eq!(deserialized.estimated_rows, Some(1000));
    }

    #[test]
    fn test_walk_flat_tree() {
        let root = PlanNode::leaf("Sort");
        let nodes = root.walk();
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn test_walk_nested_tree() {
        let leaf = PlanNode::leaf("Seq Scan");
        let root = PlanNode {
            node_type: "Sort".into(),
            children: vec![leaf],
            ..PlanNode::leaf("Sort")
        };
        let nodes = root.walk();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].node_type, "Sort");
        assert_eq!(nodes[1].node_type, "Seq Scan");
    }

    #[test]
    fn test_json_skips_empty_fields() {
        let node = PlanNode::leaf("Seq Scan");
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("node_type"));
        assert!(!json.contains("sort_keys"));
        assert!(!json.contains("children"));
        assert!(!json.contains("extra"));
    }
}
