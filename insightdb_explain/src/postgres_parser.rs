use crate::plan::PlanNode;
use crate::ParseError;

/// 解析 PostgreSQL `EXPLAIN (FORMAT JSON)` 输出的顶层 JSON 数组
pub fn parse_postgres_json(json: &str) -> Result<PlanNode, ParseError> {
    let root: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| ParseError::JsonParse(format!("PG EXPLAIN JSON 解析失败: {e}")))?;

    let plans = root
        .as_array()
        .ok_or_else(|| ParseError::InvalidFormat("PG EXPLAIN JSON 应为数组".into()))?;

    if plans.is_empty() {
        return Err(ParseError::InvalidFormat("PG EXPLAIN JSON 数组为空".into()));
    }

    let first = &plans[0];
    let plan_node = first
        .get("Plan")
        .ok_or_else(|| ParseError::InvalidFormat("缺少 Plan 字段".into()))?;

    parse_plan_node(plan_node)
}

fn parse_plan_node(node: &serde_json::Value) -> Result<PlanNode, ParseError> {
    let node_type = node
        .get("Node Type")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    let table_name = node
        .get("Relation Name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let alias = node
        .get("Alias")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let index_name = node
        .get("Index Name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let index_cond = node
        .get("Index Cond")
        .and_then(|v| v.as_str())
        .map(|s| reflow_pg_expr(s));

    let estimated_rows = node
        .get("Plan Rows")
        .and_then(|v| v.as_u64());

    let actual_rows = node
        .get("Actual Rows")
        .and_then(|v| v.as_f64())
        .map(|r| r as u64);

    let total_cost = node
        .get("Total Cost")
        .and_then(|v| v.as_f64());

    let filter = node
        .get("Filter")
        .and_then(|v| v.as_str())
        .map(|s| reflow_pg_expr(s));

    let join_type = node
        .get("Join Type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let hash_cond = node
        .get("Hash Cond")
        .and_then(|v| v.as_str())
        .map(|s| reflow_pg_expr(s));

    let parallel_aware = node
        .get("Parallel Aware")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let inner_unique = node
        .get("Inner Unique")
        .and_then(|v| v.as_bool());

    let sort_keys: Vec<String> = node
        .get("Sort Key")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    let access_method = classify_pg_node_type(&node_type, index_name.as_deref());

    // 递归解析子计划
    let mut children = Vec::new();
    if let Some(sub_plans) = node.get("Plans").and_then(|v| v.as_array()) {
        for sub in sub_plans {
            children.push(parse_plan_node(sub)?);
        }
    }

    // Plan Width 作为额外信息
    let plan_width = node.get("Plan Width").and_then(|v| v.as_u64());

    let mut extra_map = serde_json::Map::new();
    if let Some(w) = plan_width {
        extra_map.insert("plan_width".into(), w.into());
    }
    if let Some(workers) = node.get("Workers Planned") {
        extra_map.insert("workers_planned".into(), workers.clone());
    }
    if let Some(strategy) = node.get("Strategy") {
        extra_map.insert("strategy".into(), strategy.clone());
    }
    if let Some(schema) = node.get("Schema") {
        extra_map.insert("schema".into(), schema.clone());
    }

    Ok(PlanNode {
        node_type,
        table_name,
        alias,
        access_method,
        index_name,
        index_cond,
        estimated_rows,
        actual_rows,
        total_cost,
        filter,
        join_type,
        hash_cond,
        sort_keys,
        uses_temporary: false,
        uses_filesort: false,
        parallel_aware,
        inner_unique,
        children,
        extra: if extra_map.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(extra_map))
        },
    })
}

/// 将 PG 的 Node Type 归类为统一的访问方法标签
fn classify_pg_node_type(node_type: &str, index_name: Option<&str>) -> Option<String> {
    match node_type {
        "Seq Scan" => Some("seq_scan".into()),
        "Index Scan" => Some("index_scan".into()),
        "Index Only Scan" => Some("index_only_scan".into()),
        "Bitmap Heap Scan" => Some("bitmap_scan".into()),
        "Bitmap Index Scan" => Some("bitmap_index".into()),
        "Tid Scan" => Some("tid_scan".into()),
        "Subquery Scan" => Some("subquery_scan".into()),
        "Function Scan" => Some("function_scan".into()),
        "Values Scan" => Some("values_scan".into()),
        "CTE Scan" => Some("cte_scan".into()),
        "WorkTable Scan" => Some("worktable_scan".into()),
        "Foreign Scan" => Some("foreign_scan".into()),
        "Custom Scan" => Some("custom_scan".into()),
        // Join nodes don't have access methods directly
        "Nested Loop" | "Hash Join" | "Merge Join" => match index_name {
            Some(_) => Some("index_scan".into()),
            None => None,
        },
        // Non-scan nodes
        "Sort" | "Aggregate" | "Hash Aggregate" | "Group Aggregate"
        | "Limit" | "Hash" | "Materialize" | "Unique" | "SetOp"
        | "Append" | "Merge Append" | "WindowAgg" | "Result"
        | "Gather" | "Gather Merge" | "Memoize" => None,
        _ => Some(node_type.to_lowercase().replace(' ', "_")),
    }
}

/// 去掉 PG explain 表达式中可能存在的前后括号冗余
fn reflow_pg_expr(expr: &str) -> String {
    let s = expr.trim();
    if s.starts_with('(') && s.ends_with(')') {
        let inner = &s[1..s.len() - 1];
        if inner.matches('(').count() == inner.matches(')').count() {
            return inner.to_string();
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_seq_scan() {
        let json = r#"[
            {
                "Plan": {
                    "Node Type": "Seq Scan",
                    "Parallel Aware": false,
                    "Async Capable": false,
                    "Relation Name": "users",
                    "Alias": "users",
                    "Startup Cost": 0.00,
                    "Total Cost": 35.50,
                    "Plan Rows": 1000,
                    "Plan Width": 120,
                    "Filter": "((age > 18) AND (status = 'active'::text))"
                }
            }
        ]"#;

        let plan = parse_postgres_json(json).unwrap();
        assert_eq!(plan.node_type, "Seq Scan");
        assert_eq!(plan.table_name, Some("users".into()));
        assert_eq!(plan.access_method, Some("seq_scan".into()));
        assert_eq!(plan.estimated_rows, Some(1000));
        assert_eq!(plan.total_cost, Some(35.5));
        assert!(plan.filter.is_some());
        assert!(plan.filter.unwrap().contains("age > 18"));
        assert!(!plan.parallel_aware);
        assert!(plan.children.is_empty());
    }

    #[test]
    fn test_parse_index_scan() {
        let json = r#"[
            {
                "Plan": {
                    "Node Type": "Index Scan",
                    "Parallel Aware": false,
                    "Async Capable": false,
                    "Scan Direction": "Forward",
                    "Index Name": "users_pkey",
                    "Relation Name": "users",
                    "Alias": "users",
                    "Startup Cost": 0.00,
                    "Total Cost": 8.44,
                    "Plan Rows": 1,
                    "Plan Width": 120,
                    "Index Cond": "(id = 42)"
                }
            }
        ]"#;

        let plan = parse_postgres_json(json).unwrap();
        assert_eq!(plan.node_type, "Index Scan");
        assert_eq!(plan.access_method, Some("index_scan".into()));
        assert_eq!(plan.index_name, Some("users_pkey".into()));
        assert_eq!(plan.index_cond, Some("id = 42".into()));
        assert_eq!(plan.estimated_rows, Some(1));
    }

    #[test]
    fn test_parse_index_only_scan() {
        let json = r#"[
            {
                "Plan": {
                    "Node Type": "Index Only Scan",
                    "Parallel Aware": false,
                    "Async Capable": false,
                    "Scan Direction": "Forward",
                    "Index Name": "idx_users_email",
                    "Relation Name": "users",
                    "Alias": "users",
                    "Startup Cost": 0.00,
                    "Total Cost": 4.50,
                    "Plan Rows": 10,
                    "Plan Width": 60,
                    "Index Cond": "(email > 'a'::text)"
                }
            }
        ]"#;

        let plan = parse_postgres_json(json).unwrap();
        assert_eq!(plan.access_method, Some("index_only_scan".into()));
    }

    #[test]
    fn test_parse_nested_loop_join() {
        let json = r#"[
            {
                "Plan": {
                    "Node Type": "Nested Loop",
                    "Parallel Aware": false,
                    "Async Capable": false,
                    "Join Type": "Inner",
                    "Startup Cost": 0.00,
                    "Total Cost": 500.00,
                    "Plan Rows": 100,
                    "Plan Width": 160,
                    "Inner Unique": true,
                    "Plans": [
                        {
                            "Node Type": "Seq Scan",
                            "Parent Relationship": "Outer",
                            "Parallel Aware": false,
                            "Async Capable": false,
                            "Relation Name": "orders",
                            "Alias": "orders",
                            "Startup Cost": 0.00,
                            "Total Cost": 100.00,
                            "Plan Rows": 100,
                            "Plan Width": 80,
                            "Filter": "((status)::text = 'active'::text)"
                        },
                        {
                            "Node Type": "Index Scan",
                            "Parent Relationship": "Inner",
                            "Parallel Aware": false,
                            "Async Capable": false,
                            "Scan Direction": "Forward",
                            "Index Name": "users_pkey",
                            "Relation Name": "users",
                            "Alias": "users",
                            "Startup Cost": 0.00,
                            "Total Cost": 4.00,
                            "Plan Rows": 1,
                            "Plan Width": 80,
                            "Index Cond": "(id = orders.user_id)"
                        }
                    ]
                }
            }
        ]"#;

        let plan = parse_postgres_json(json).unwrap();
        assert_eq!(plan.node_type, "Nested Loop");
        assert_eq!(plan.join_type, Some("Inner".into()));
        assert_eq!(plan.inner_unique, Some(true));
        assert_eq!(plan.children.len(), 2);

        let outer = &plan.children[0];
        assert_eq!(outer.node_type, "Seq Scan");
        assert_eq!(outer.table_name, Some("orders".into()));

        let inner = &plan.children[1];
        assert_eq!(inner.node_type, "Index Scan");
        assert_eq!(inner.index_name, Some("users_pkey".into()));
        assert_eq!(inner.index_cond, Some("id = orders.user_id".into()));
    }

    #[test]
    fn test_parse_sort() {
        let json = r#"[
            {
                "Plan": {
                    "Node Type": "Sort",
                    "Parallel Aware": false,
                    "Async Capable": false,
                    "Startup Cost": 100.00,
                    "Total Cost": 105.00,
                    "Plan Rows": 100,
                    "Plan Width": 80,
                    "Sort Key": ["amount DESC", "created_at"],
                    "Plans": [
                        {
                            "Node Type": "Index Scan",
                            "Parent Relationship": "Outer",
                            "Parallel Aware": false,
                            "Async Capable": false,
                            "Scan Direction": "Forward",
                            "Index Name": "idx_orders_user_id",
                            "Relation Name": "orders",
                            "Alias": "orders",
                            "Startup Cost": 0.00,
                            "Total Cost": 50.00,
                            "Plan Rows": 100,
                            "Plan Width": 80,
                            "Index Cond": "(user_id = 42)"
                        }
                    ]
                }
            }
        ]"#;

        let plan = parse_postgres_json(json).unwrap();
        assert_eq!(plan.node_type, "Sort");
        assert_eq!(plan.sort_keys, vec!["amount DESC", "created_at"]);
        assert_eq!(plan.children.len(), 1);
        assert_eq!(plan.children[0].access_method, Some("index_scan".into()));
    }

    #[test]
    fn test_parse_hash_join() {
        let json = r#"[
            {
                "Plan": {
                    "Node Type": "Hash Join",
                    "Parallel Aware": false,
                    "Async Capable": false,
                    "Join Type": "Inner",
                    "Startup Cost": 50.00,
                    "Total Cost": 200.00,
                    "Plan Rows": 100,
                    "Plan Width": 160,
                    "Inner Unique": true,
                    "Hash Cond": "(orders.user_id = users.id)",
                    "Plans": [
                        {
                            "Node Type": "Seq Scan",
                            "Parent Relationship": "Outer",
                            "Parallel Aware": false,
                            "Async Capable": false,
                            "Relation Name": "orders",
                            "Alias": "orders",
                            "Startup Cost": 0.00,
                            "Total Cost": 100.00,
                            "Plan Rows": 100,
                            "Plan Width": 80
                        },
                        {
                            "Node Type": "Hash",
                            "Parent Relationship": "Inner",
                            "Parallel Aware": false,
                            "Async Capable": false,
                            "Startup Cost": 30.00,
                            "Total Cost": 30.00,
                            "Plan Rows": 50,
                            "Plan Width": 80,
                            "Plans": [
                                {
                                    "Node Type": "Seq Scan",
                                    "Parent Relationship": "Outer",
                                    "Parallel Aware": false,
                                    "Async Capable": false,
                                    "Relation Name": "users",
                                    "Alias": "users",
                                    "Startup Cost": 0.00,
                                    "Total Cost": 30.00,
                                    "Plan Rows": 50,
                                    "Plan Width": 80
                                }
                            ]
                        }
                    ]
                }
            }
        ]"#;

        let plan = parse_postgres_json(json).unwrap();
        assert_eq!(plan.node_type, "Hash Join");
        assert_eq!(plan.join_type, Some("Inner".into()));
        assert_eq!(plan.hash_cond, Some("orders.user_id = users.id".into()));
        assert_eq!(plan.children.len(), 2);
        assert_eq!(plan.children[0].node_type, "Seq Scan");
        assert_eq!(plan.children[1].node_type, "Hash");
        assert_eq!(plan.children[1].children.len(), 1);
    }

    #[test]
    fn test_parse_explain_analyze_with_actual_rows() {
        let json = r#"[
            {
                "Plan": {
                    "Node Type": "Seq Scan",
                    "Parallel Aware": false,
                    "Relation Name": "users",
                    "Alias": "users",
                    "Startup Cost": 0.00,
                    "Total Cost": 35.50,
                    "Plan Rows": 1000,
                    "Plan Width": 120,
                    "Actual Startup Time": 0.025,
                    "Actual Total Time": 0.150,
                    "Actual Rows": 950,
                    "Actual Loops": 1
                }
            }
        ]"#;

        let plan = parse_postgres_json(json).unwrap();
        assert_eq!(plan.actual_rows, Some(950));
    }

    #[test]
    fn test_parse_empty_array() {
        let result = parse_postgres_json("[]");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_plan_field() {
        let result = parse_postgres_json("[{}]");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = parse_postgres_json("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_reflow_pg_expr() {
        assert_eq!(reflow_pg_expr("(id = 42)"), "id = 42");
        assert_eq!(reflow_pg_expr("((a > 1) AND (b < 2))"), "(a > 1) AND (b < 2)");
        assert_eq!(reflow_pg_expr("no parens"), "no parens");
        assert_eq!(reflow_pg_expr("(single)"), "single");
    }

    #[test]
    fn test_classify_pg_node_type() {
        assert_eq!(classify_pg_node_type("Seq Scan", None), Some("seq_scan".into()));
        assert_eq!(classify_pg_node_type("Index Scan", Some("idx")), Some("index_scan".into()));
        assert_eq!(classify_pg_node_type("Index Only Scan", Some("idx")), Some("index_only_scan".into()));
        assert_eq!(classify_pg_node_type("Bitmap Heap Scan", Some("idx")), Some("bitmap_scan".into()));
        assert_eq!(classify_pg_node_type("Sort", None), None);
        assert_eq!(classify_pg_node_type("Aggregate", None), None);
        assert_eq!(classify_pg_node_type("Nested Loop", None), None);
    }

    #[test]
    fn test_parse_parallel_seq_scan() {
        let json = r#"[
            {
                "Plan": {
                    "Node Type": "Gather",
                    "Parallel Aware": false,
                    "Startup Cost": 1000.00,
                    "Total Cost": 5000.00,
                    "Plan Rows": 10000,
                    "Plan Width": 80,
                    "Workers Planned": 2,
                    "Single Copy": false,
                    "Plans": [
                        {
                            "Node Type": "Seq Scan",
                            "Parent Relationship": "Outer",
                            "Parallel Aware": true,
                            "Relation Name": "large_table",
                            "Alias": "large_table",
                            "Startup Cost": 0.00,
                            "Total Cost": 4000.00,
                            "Plan Rows": 5000,
                            "Plan Width": 80
                        }
                    ]
                }
            }
        ]"#;

        let plan = parse_postgres_json(json).unwrap();
        assert_eq!(plan.node_type, "Gather");
        assert!(!plan.parallel_aware);
        assert_eq!(plan.children.len(), 1);
        assert!(plan.children[0].parallel_aware);
        assert_eq!(plan.children[0].node_type, "Seq Scan");
    }

    #[test]
    fn test_parse_group_aggregate() {
        let json = r#"[
            {
                "Plan": {
                    "Node Type": "Aggregate",
                    "Strategy": "Sorted",
                    "Partial Mode": "Simple",
                    "Parallel Aware": false,
                    "Startup Cost": 50.00,
                    "Total Cost": 55.00,
                    "Plan Rows": 10,
                    "Plan Width": 80,
                    "Group Key": ["status"],
                    "Plans": [
                        {
                            "Node Type": "Sort",
                            "Parent Relationship": "Outer",
                            "Parallel Aware": false,
                            "Startup Cost": 50.00,
                            "Total Cost": 52.00,
                            "Plan Rows": 100,
                            "Plan Width": 80,
                            "Sort Key": ["status"],
                            "Plans": [
                                {
                                    "Node Type": "Seq Scan",
                                    "Parent Relationship": "Outer",
                                    "Parallel Aware": false,
                                    "Relation Name": "orders",
                                    "Alias": "orders",
                                    "Startup Cost": 0.00,
                                    "Total Cost": 40.00,
                                    "Plan Rows": 100,
                                    "Plan Width": 80
                                }
                            ]
                        }
                    ]
                }
            }
        ]"#;

        let plan = parse_postgres_json(json).unwrap();
        assert_eq!(plan.node_type, "Aggregate");
        assert_eq!(plan.children.len(), 1);
        assert_eq!(plan.children[0].node_type, "Sort");
        assert_eq!(plan.children[0].children[0].node_type, "Seq Scan");
    }
}
