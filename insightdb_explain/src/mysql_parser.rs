use crate::plan::PlanNode;
use crate::ParseError;

/// 解析 MySQL `EXPLAIN FORMAT=JSON` 输出的顶层 JSON 文本
pub fn parse_mysql_json(json: &str) -> Result<PlanNode, ParseError> {
    let root: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| ParseError::JsonParse(format!("MySQL EXPLAIN JSON 解析失败: {e}")))?;

    let query_block = root
        .get("query_block")
        .ok_or_else(|| ParseError::InvalidFormat("缺少 query_block".into()))?;

    parse_query_block(query_block)
}

fn parse_query_block(block: &serde_json::Value) -> Result<PlanNode, ParseError> {
    let mut root = PlanNode::leaf("Query");

    if let Some(cost) = block.get("cost_info") {
        if let Some(qc) = cost.get("query_cost") {
            root.total_cost = qc.as_str().and_then(|s| s.parse().ok());
        }
    }

    // ordering_operation: 表示使用了 filesort
    if let Some(ordering) = block.get("ordering_operation") {
        root.uses_filesort = ordering
            .get("using_filesort")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if let Some(table) = ordering.get("table") {
            let mut child = parse_table(table)?;
            child.uses_filesort = root.uses_filesort;
            root.children.push(child);
        }
    }
    // grouping_operation: 表示使用了临时表（GROUP BY / DISTINCT）
    else if let Some(grouping) = block.get("grouping_operation") {
        root.uses_temporary = grouping
            .get("using_temporary_table")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if let Some(table) = grouping.get("table") {
            let mut child = parse_table(table)?;
            child.uses_temporary = root.uses_temporary;
            root.children.push(child);
        }
    }
    // nested_loop: 多表连接
    else if let Some(nested_loops) = block.get("nested_loop").and_then(|v| v.as_array()) {
        for entry in nested_loops {
            if let Some(table) = entry.get("table") {
                root.children.push(parse_table(table)?);
            }
        }
    }
    // 单表
    else if let Some(table) = block.get("table") {
        root = parse_table(table)?;
        // 把 query_block 级别的 cost 合入
        if root.total_cost.is_none() {
            if let Some(cost) = block.get("cost_info") {
                if let Some(qc) = cost.get("query_cost") {
                    root.total_cost = qc.as_str().and_then(|s| s.parse().ok());
                }
            }
        }
    }

    // 保留原始 JSON 用于审计
    root.extra = Some(block.clone());

    Ok(root)
}

/// 解析单个 table 节点
fn parse_table(table: &serde_json::Value) -> Result<PlanNode, ParseError> {
    let table_name = table
        .get("table_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let access_type = table
        .get("access_type")
        .and_then(|v| v.as_str())
        .and_then(|s| normalize_mysql_access_type(s));

    let index_name = table
        .get("key")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let index_cond = table
        .get("index_condition")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let estimated_rows = table
        .get("rows_examined_per_scan")
        .or_else(|| table.get("rows_produced_per_join"))
        .and_then(|v| v.as_u64());

    let filter = table
        .get("attached_condition")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let total_cost = table
        .get("cost_info")
        .and_then(|c| c.get("prefix_cost"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok());

    let uses_temporary = table
        .get("using_temporary_table")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let node_type = if index_name.is_some() {
        match access_type.as_deref() {
            Some("index_lookup") | Some("const") => "Index Lookup".to_string(),
            _ => "Index Scan".to_string(),
        }
    } else {
        "Table Scan".to_string()
    };

    Ok(PlanNode {
        node_type,
        table_name,
        alias: None,
        access_method: access_type,
        index_name,
        index_cond,
        estimated_rows,
        actual_rows: None,
        total_cost,
        filter,
        join_type: None,
        hash_cond: None,
        sort_keys: vec![],
        uses_temporary,
        uses_filesort: false,
        parallel_aware: false,
        inner_unique: None,
        children: vec![],
        extra: Some(table.clone()),
    })
}

/// 将 MySQL access_type 归类为统一的访问方法标签
fn normalize_mysql_access_type(access_type: &str) -> Option<String> {
    match access_type.to_uppercase().as_str() {
        "ALL" => Some("seq_scan".into()),
        "INDEX" => Some("index_scan".into()),
        "RANGE" => Some("index_range".into()),
        "REF" | "EQ_REF" => Some("index_lookup".into()),
        "CONST" | "SYSTEM" => Some("const".into()),
        "REF_OR_NULL" => Some("index_lookup".into()),
        "INDEX_MERGE" => Some("index_merge".into()),
        "FULLTEXT" => Some("fulltext".into()),
        other => Some(other.to_lowercase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_table_scan() {
        let json = r#"{
            "query_block": {
                "select_id": 1,
                "cost_info": {
                    "query_cost": "2.00"
                },
                "table": {
                    "table_name": "users",
                    "access_type": "ALL",
                    "rows_examined_per_scan": 100,
                    "rows_produced_per_join": 100,
                    "filtered": "100.00",
                    "cost_info": {
                        "read_cost": "1.00",
                        "eval_cost": "1.00",
                        "prefix_cost": "2.00",
                        "data_read_per_join": "4K"
                    },
                    "used_columns": ["id", "name", "email"]
                }
            }
        }"#;

        let plan = parse_mysql_json(json).unwrap();
        assert_eq!(plan.node_type, "Table Scan");
        assert_eq!(plan.table_name, Some("users".into()));
        assert_eq!(plan.access_method, Some("seq_scan".into()));
        assert_eq!(plan.estimated_rows, Some(100));
        assert_eq!(plan.total_cost, Some(2.0));
        assert!(plan.children.is_empty());
        assert!(plan.extra.is_some());
    }

    #[test]
    fn test_parse_index_range_scan() {
        let json = r#"{
            "query_block": {
                "select_id": 1,
                "table": {
                    "table_name": "orders",
                    "access_type": "range",
                    "possible_keys": ["idx_user_id"],
                    "key": "idx_user_id",
                    "key_length": "5",
                    "used_key_parts": ["user_id"],
                    "rows_examined_per_scan": 50,
                    "rows_produced_per_join": 50,
                    "filtered": "100.00",
                    "cost_info": {
                        "read_cost": "4.00",
                        "eval_cost": "5.00",
                        "prefix_cost": "9.00",
                        "data_read_per_join": "2K"
                    },
                    "used_columns": ["id", "user_id", "amount"],
                    "attached_condition": "(`orders`.`status` = 'active')"
                }
            }
        }"#;

        let plan = parse_mysql_json(json).unwrap();
        assert_eq!(plan.node_type, "Index Scan");
        assert_eq!(plan.access_method, Some("index_range".into()));
        assert_eq!(plan.index_name, Some("idx_user_id".into()));
        assert_eq!(plan.estimated_rows, Some(50));
        assert!(plan.filter.is_some());
        assert!(plan.filter.unwrap().contains("status"));
    }

    #[test]
    fn test_parse_filesort() {
        let json = r#"{
            "query_block": {
                "select_id": 1,
                "ordering_operation": {
                    "using_filesort": true,
                    "cost_info": {
                        "sort_cost": "3.00"
                    },
                    "table": {
                        "table_name": "orders",
                        "access_type": "ALL",
                        "rows_examined_per_scan": 200,
                        "rows_produced_per_join": 200,
                        "filtered": "100.00",
                        "cost_info": {
                            "read_cost": "17.00",
                            "eval_cost": "20.00",
                            "prefix_cost": "37.00",
                            "data_read_per_join": "8K"
                        },
                        "used_columns": ["id", "user_id", "amount", "created_at"]
                    }
                }
            }
        }"#;

        let plan = parse_mysql_json(json).unwrap();
        assert!(plan.uses_filesort);
        assert_eq!(plan.children.len(), 1);
        assert!(plan.children[0].uses_filesort);
        assert_eq!(plan.children[0].node_type, "Table Scan");
        assert_eq!(plan.children[0].table_name, Some("orders".into()));
    }

    #[test]
    fn test_parse_nested_loop_join() {
        let json = r#"{
            "query_block": {
                "select_id": 1,
                "cost_info": {
                    "query_cost": "3.50"
                },
                "nested_loop": [
                    {
                        "table": {
                            "table_name": "orders",
                            "access_type": "ALL",
                            "rows_examined_per_scan": 100,
                            "rows_produced_per_join": 100,
                            "filtered": "100.00",
                            "cost_info": {
                                "read_cost": "1.00",
                                "eval_cost": "1.00",
                                "prefix_cost": "2.00",
                                "data_read_per_join": "4K"
                            },
                            "used_columns": ["id", "user_id", "amount"]
                        }
                    },
                    {
                        "table": {
                            "table_name": "users",
                            "access_type": "eq_ref",
                            "possible_keys": ["PRIMARY"],
                            "key": "PRIMARY",
                            "key_length": "4",
                            "used_key_parts": ["id"],
                            "ref": ["testdb.orders.user_id"],
                            "rows_examined_per_scan": 1,
                            "rows_produced_per_join": 100,
                            "filtered": "100.00",
                            "cost_info": {
                                "read_cost": "1.00",
                                "eval_cost": "0.50",
                                "prefix_cost": "3.50",
                                "data_read_per_join": "6K"
                            },
                            "used_columns": ["id", "name", "email"]
                        }
                    }
                ]
            }
        }"#;

        let plan = parse_mysql_json(json).unwrap();
        assert_eq!(plan.children.len(), 2);
        assert_eq!(plan.children[0].node_type, "Table Scan");
        assert_eq!(plan.children[1].node_type, "Index Lookup");
        assert_eq!(plan.children[1].access_method, Some("index_lookup".into()));
        assert_eq!(plan.children[1].index_name, Some("PRIMARY".into()));
    }

    #[test]
    fn test_parse_empty_or_missing_data() {
        let result = parse_mysql_json("{}");
        assert!(result.is_err());

        let result = parse_mysql_json("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_access_type() {
        assert_eq!(normalize_mysql_access_type("ALL"), Some("seq_scan".into()));
        assert_eq!(normalize_mysql_access_type("INDEX"), Some("index_scan".into()));
        assert_eq!(normalize_mysql_access_type("RANGE"), Some("index_range".into()));
        assert_eq!(normalize_mysql_access_type("REF"), Some("index_lookup".into()));
        assert_eq!(normalize_mysql_access_type("EQ_REF"), Some("index_lookup".into()));
        assert_eq!(normalize_mysql_access_type("CONST"), Some("const".into()));
        assert_eq!(normalize_mysql_access_type("INDEX_MERGE"), Some("index_merge".into()));
        assert_eq!(normalize_mysql_access_type("FULLTEXT"), Some("fulltext".into()));
        assert_eq!(normalize_mysql_access_type("UNKNOWN"), Some("unknown".into()));
    }

    #[test]
    fn test_parse_table_scan_from_cli_like_output() {
        // 模拟 EXPLAIN FORMAT=JSON 对空表的输出
        let json = r#"{
            "query_block": {
                "select_id": 1,
                "cost_info": {
                    "query_cost": "0.35"
                },
                "table": {
                    "table_name": "empty_table",
                    "access_type": "ALL",
                    "rows_examined_per_scan": 0,
                    "rows_produced_per_join": 0,
                    "filtered": "100.00",
                    "cost_info": {
                        "read_cost": "0.25",
                        "eval_cost": "0.10",
                        "prefix_cost": "0.35",
                        "data_read_per_join": "0"
                    },
                    "used_columns": ["id"]
                }
            }
        }"#;

        let plan = parse_mysql_json(json).unwrap();
        assert_eq!(plan.estimated_rows, Some(0));
        assert_eq!(plan.total_cost, Some(0.35));
    }

    #[test]
    fn test_plan_node_fields_are_correctly_set() {
        let json = r#"{
            "query_block": {
                "select_id": 1,
                "table": {
                    "table_name": "users",
                    "access_type": "range",
                    "possible_keys": ["PRIMARY", "idx_email"],
                    "key": "idx_email",
                    "key_length": "768",
                    "used_key_parts": ["email"],
                    "rows_examined_per_scan": 1,
                    "rows_produced_per_join": 1,
                    "filtered": "100.00",
                    "index_condition": "(`users`.`email` like 'test%')",
                    "cost_info": {
                        "read_cost": "0.25",
                        "eval_cost": "0.10",
                        "prefix_cost": "0.35",
                        "data_read_per_join": "120"
                    },
                    "used_columns": ["id", "email", "name"]
                }
            }
        }"#;

        let plan = parse_mysql_json(json).unwrap();
        assert_eq!(plan.node_type, "Index Scan");
        assert_eq!(plan.access_method, Some("index_range".into()));
        assert_eq!(plan.index_name, Some("idx_email".into()));
        assert_eq!(plan.index_cond, Some("(`users`.`email` like 'test%')".into()));
        assert_eq!(plan.estimated_rows, Some(1));
        assert!(!plan.uses_filesort);
        assert!(!plan.uses_temporary);
        assert!(!plan.parallel_aware);
        assert!(plan.extra.is_some());
        assert!(plan.children.is_empty());
    }
}
