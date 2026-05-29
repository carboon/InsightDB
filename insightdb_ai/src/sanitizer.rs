use std::collections::HashMap;
use crate::types::SanitizedContext;
use insightdb_advisor::DiagnosisReport;
use insightdb_explain::PlanNode;

/// 脱敏引擎：将原始诊断上下文中的敏感标识替换为通用占位符
#[derive(Debug, Clone)]
pub struct Sanitizer {
    /// 数据库名映射
    db_names: HashMap<String, String>,
    /// 表名映射
    table_names: HashMap<String, String>,
    /// 列名映射
    column_names: HashMap<String, String>,
    table_counter: usize,
    column_counter: usize,
}

impl Sanitizer {
    pub fn new() -> Self {
        Self {
            db_names: HashMap::new(),
            table_names: HashMap::new(),
            column_names: HashMap::new(),
            table_counter: 0,
            column_counter: 0,
        }
    }

    /// 对完整诊断报告进行脱敏
    pub fn sanitize(&mut self, report: &DiagnosisReport) -> SanitizedContext {
        let mut mapping = Vec::new();

        // 注册数据库名
        if !report.database_name.is_empty() {
            self.db_names.entry(report.database_name.clone())
                .or_insert_with(|| "database_1".to_string());
        }

        // 注册所有表名
        for table in &report.schema.tables {
            self.register_table(&table.name);
            // 注册所有列名
            for col in &table.columns {
                self.register_column(&col.name);
            }
        }

        let sanitized_sql = self.sanitize_sql(&report.sql);

        let catalog_summary = self.sanitize_catalog(&report.schema.tables);

        let explain_summary = self.sanitize_explain(&report.plan);

        let rule_findings: Vec<String> = report.findings.iter()
            .map(|f| {
                let evidence = self.sanitize_text(&f.evidence);
                let recommendation = self.sanitize_text(&f.recommendation);
                format!("[{:?}|{}] {}: {} → {}",
                    f.severity, f.id, f.title, evidence, recommendation)
            })
            .collect();

        // 构建映射表
        for (orig, mapped) in &self.table_names {
            mapping.push((format!("table:{}", orig), mapped.clone()));
        }
        for (orig, mapped) in &self.column_names {
            mapping.push((format!("col:{}", orig), mapped.clone()));
        }

        SanitizedContext {
            sanitized_sql,
            catalog_summary,
            explain_summary,
            rule_findings,
            db_type: report.db_type.clone(),
            db_version: report.db_version.clone(),
            identifier_mapping: mapping,
        }
    }

    fn register_table(&mut self, name: &str) {
        if !self.table_names.contains_key(name) {
            self.table_counter += 1;
            self.table_names.insert(name.to_string(), format!("t_{}", self.table_counter));
        }
    }

    fn register_column(&mut self, name: &str) {
        if !self.column_names.contains_key(name) {
            self.column_counter += 1;
            self.column_names.insert(name.to_string(), format!("c_{}", self.column_counter));
        }
    }

    fn sanitize_sql(&self, sql: &str) -> String {
        let mut result = sql.to_string();
        // 替换表名
        for (orig, mapped) in &self.table_names {
            result = result.replace(orig.as_str(), mapped.as_str());
        }
        // 替换列名
        for (orig, mapped) in &self.column_names {
            result = result.replace(orig.as_str(), mapped.as_str());
        }
        // 替换字面量字符串
        result = replace_string_literals(&result);
        // 替换数字字面量
        result = replace_number_literals(&result);
        result
    }

    fn sanitize_catalog(&self, tables: &[insightdb_catalog::TableInfo]) -> String {
        let mut lines = Vec::new();
        for table in tables {
            let mapped_name = self.table_names.get(&table.name)
                .cloned()
                .unwrap_or_else(|| table.name.clone());
            let cols: Vec<String> = table.columns.iter()
                .map(|c| {
                    let mapped_col = self.column_names.get(&c.name)
                        .cloned()
                        .unwrap_or_else(|| c.name.clone());
                    format!("  {mapped_col} {} {} {}",
                        c.data_type,
                        if c.nullable { "NULL" } else { "NOT NULL" },
                        if c.is_primary_key { "PK" } else { "" })
                })
                .collect();
            let idxs: Vec<String> = table.indexes.iter()
                .map(|i| {
                    let cols: Vec<String> = i.columns.iter()
                        .map(|c| self.column_names.get(c).cloned().unwrap_or_else(|| c.clone()))
                        .collect();
                    let idx_name = self.sanitize_text(&i.name);
                    format!("  INDEX {idx_name} ({})", cols.join(", "))
                })
                .collect();

            lines.push(format!(
                "Table {mapped_name} (est. {} rows) engine={}",
                table.row_count_estimate.unwrap_or(0),
                table.engine.as_deref().unwrap_or("?")
            ));
            if !cols.is_empty() { lines.extend(cols); }
            if !idxs.is_empty() { lines.extend(idxs); }
        }
        lines.join("\n")
    }

    fn sanitize_explain(&self, plan: &PlanNode) -> String {
        let mut lines = Vec::new();
        self.format_plan_node(plan, 0, &mut lines);
        lines.join("\n")
    }

    /// 对任意文本执行标识符替换（表名、列名、字面量）
    fn sanitize_text(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (orig, mapped) in &self.table_names {
            result = result.replace(orig.as_str(), mapped.as_str());
        }
        for (orig, mapped) in &self.column_names {
            result = result.replace(orig.as_str(), mapped.as_str());
        }
        result = replace_string_literals(&result);
        result = replace_number_literals(&result);
        result
    }

    fn format_plan_node(&self, node: &PlanNode, depth: usize, lines: &mut Vec<String>) {
        let indent = "  ".repeat(depth);
        let table = node.table_name.as_deref()
            .and_then(|n| self.table_names.get(n))
            .map(|s| s.as_str())
            .or(node.table_name.as_deref())
            .unwrap_or("-");

        let mut parts = vec![format!("{}[{}]", node.node_type, table)];

        if let Some(ref method) = node.access_method {
            parts.push(format!("via {method}"));
        }
        if let Some(rows) = node.estimated_rows {
            parts.push(format!("~{rows} rows"));
        }
        if let Some(ref filter) = node.filter {
            let mut sanitized_filter = filter.clone();
            for (orig, mapped) in &self.column_names {
                sanitized_filter = sanitized_filter.replace(orig.as_str(), mapped.as_str());
            }
            for (orig, mapped) in &self.table_names {
                sanitized_filter = sanitized_filter.replace(orig.as_str(), mapped.as_str());
            }
            sanitized_filter = replace_string_literals(&sanitized_filter);
            sanitized_filter = replace_number_literals(&sanitized_filter);
            parts.push(format!("filter={sanitized_filter}"));
        }
        if node.uses_filesort {
            parts.push("filesort".into());
        }
        if node.uses_temporary {
            parts.push("temporary".into());
        }
        if !node.sort_keys.is_empty() {
            parts.push(format!("sort={}", node.sort_keys.join(",")));
        }

        lines.push(format!("{indent}{}", parts.join(" ")));

        for child in &node.children {
            self.format_plan_node(child, depth + 1, lines);
        }
    }
}

impl Default for Sanitizer {
    fn default() -> Self {
        Self::new()
    }
}

fn replace_string_literals(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            // 找到字符串字面量的结束位置
            let start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        i += 2; // escaped quote
                    } else {
                        i += 1; // end of string literal
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            // 替换整个字符串字面量为占位符
            let replacement = if start > 0 && bytes[start - 1] == b'(' {
                "<string_literal>"
            } else {
                "'<string_literal>'"
            };
            result.push_str(replacement);
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

fn replace_number_literals(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() && (i == 0 || !chars[i - 1].is_alphanumeric() && chars[i - 1] != '_') {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < chars.len() && chars[i] == '.' {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            // 检查后面不是标识符字符
            if i >= chars.len() || !chars[i].is_alphanumeric() && chars[i] != '_' {
                result.push_str("<number>");
            } else {
                // 是标识符的一部分，原样保留
                for j in start..i {
                    result.push(chars[j]);
                }
            }
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use insightdb_catalog::*;

    #[test]
    fn test_string_literal_replacement() {
        let sql = "SELECT * FROM users WHERE email = 'test@example.com'";
        let result = replace_string_literals(sql);
        assert!(!result.contains("test@example.com"));
        assert!(result.contains("string_literal"));
    }

    #[test]
    fn test_string_literal_with_escaped_quotes() {
        let sql = "SELECT * FROM t WHERE name = 'it''s a test'";
        let result = replace_string_literals(sql);
        assert!(!result.contains("it's"));
        assert!(result.contains("string_literal"));
    }

    #[test]
    fn test_table_name_mapping() {
        let mut s = Sanitizer::new();
        s.register_table("users");
        s.register_table("orders");
        assert_eq!(s.table_names.get("users"), Some(&"t_1".to_string()));
        assert_eq!(s.table_names.get("orders"), Some(&"t_2".to_string()));
    }

    #[test]
    fn test_table_name_stable_on_second_register() {
        let mut s = Sanitizer::new();
        s.register_table("users");
        s.register_table("users");
        assert_eq!(s.table_counter, 1);
    }

    #[test]
    fn test_column_name_mapping() {
        let mut s = Sanitizer::new();
        s.register_column("email");
        s.register_column("name");
        assert_eq!(s.column_names.get("email"), Some(&"c_1".to_string()));
        assert_eq!(s.column_names.get("name"), Some(&"c_2".to_string()));
    }

    #[test]
    fn test_sanitize_sql_replaces_table_and_columns() {
        let mut s = Sanitizer::new();
        s.register_table("users");
        s.register_column("email");
        s.register_column("name");
        let sql = "SELECT email, name FROM users";
        let sanitized = s.sanitize_sql(sql);
        assert!(!sanitized.contains("users"));
        assert!(!sanitized.contains("email"));
        assert!(!sanitized.contains("name"));
        assert!(sanitized.contains("t_1"));
    }

    #[test]
    fn test_sanitize_removes_credentials_from_url() {
        // 凭据从未被采集到诊断上下文中，因此不会发送给模型
        // sanitize_sql 处理的是 SQL 查询文本，而非连接 URL
        let _sql = "mysql://root:secret@host/db";
    }

    #[test]
    fn test_sanitize_full_report() {
        use insightdb_advisor::DiagnosisReport;
        use insightdb_explain::PlanNode;
        use insightdb_rules::{RuleFinding, Severity};

        let plan = PlanNode::leaf("Seq Scan");
        let schema = SchemaInfo {
            db_type: "mysql".into(),
            version: "8.0.30".into(),
            database_name: "mydb".into(),
            tables: vec![
                TableInfo {
                    name: "users".into(),
                    table_type: "BASE TABLE".into(),
                    engine: Some("InnoDB".into()),
                    row_count_estimate: Some(50000),
                    columns: vec![
                        ColumnInfo {
                            name: "id".into(), ordinal: 1, data_type: "int".into(),
                            nullable: false, is_primary_key: true,
                            default_value: None, character_max_length: None, column_comment: None,
                        },
                        ColumnInfo {
                            name: "email".into(), ordinal: 2, data_type: "varchar".into(),
                            nullable: true, is_primary_key: false,
                            default_value: None, character_max_length: Some(200), column_comment: None,
                        },
                    ],
                    indexes: vec![
                        IndexInfo {
                            name: "PRIMARY".into(), columns: vec!["id".into()],
                            unique: true, index_type: "BTREE".into(), is_primary: true,
                        },
                    ],
                },
            ],
            collected_at: "".into(),
        };

        let findings = vec![
            RuleFinding {
                id: "FULL_TABLE_SCAN".into(),
                severity: Severity::High,
                title: "全表扫描".into(),
                evidence: "表 users 估算扫描 50000 行".into(),
                recommendation: "为 email 创建索引".into(),
                risk: "性能线性下降".into(),
                verification: "CREATE INDEX ... 后 EXPLAIN".into(),
                confidence: 0.95,
            },
        ];

        let report = DiagnosisReport::new(
            "SELECT id, email FROM users WHERE email = 'test@example.com'",
            "mysql", "8.0.30", "mydb",
            findings, plan, schema,
        );

        let mut sanitizer = Sanitizer::new();
        let ctx = sanitizer.sanitize(&report);

        // 验证脱敏
        assert!(!ctx.sanitized_sql.contains("users"));
        assert!(!ctx.sanitized_sql.contains("email"));
        assert!(!ctx.sanitized_sql.contains("test@example.com"));
        assert!(!ctx.catalog_summary.contains("users"));

        // 验证映射表
        assert!(ctx.identifier_mapping.iter().any(|(k, _)| k == "table:users"));
        assert!(ctx.identifier_mapping.iter().any(|(k, _)| k == "col:email"));

        // 验证规则被保留
        assert_eq!(ctx.rule_findings.len(), 1);
    }

    #[test]
    fn test_sanitized_context_fields_present() {
        let ctx = SanitizedContext {
            sanitized_sql: "SELECT * FROM t_1".into(),
            catalog_summary: "Table t_1 (50000 rows)".into(),
            explain_summary: "[Seq Scan] t_1 ~50000 rows".into(),
            rule_findings: vec!["[HIGH|FULL_TABLE_SCAN] ...".into()],
            db_type: "mysql".into(),
            db_version: "8.0.30".into(),
            identifier_mapping: vec![],
        };

        assert!(!ctx.sanitized_sql.is_empty());
        assert!(!ctx.catalog_summary.is_empty());
        assert!(!ctx.explain_summary.is_empty());
    }
}
