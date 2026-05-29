use crate::types::SanitizedContext;

/// Prompt 模板构建器：将脱敏后的诊断上下文组装为结构化 Prompt
pub struct PromptBuilder {
    template: String,
}

impl PromptBuilder {
    pub fn default_template() -> Self {
        Self {
            template: DEFAULT_TEMPLATE.to_string(),
        }
    }

    pub fn new(template: impl Into<String>) -> Self {
        Self { template: template.into() }
    }

    /// 基于脱敏上下文构建最终 Prompt
    pub fn build(&self, ctx: &SanitizedContext) -> String {
        let system_context = format!(
            "- DB Type: {}\n- Version: {}\n",
            ctx.db_type, ctx.db_version
        );

        let sanitized_sql = format!("```sql\n{}\n```", ctx.sanitized_sql);

        let catalog = if ctx.catalog_summary.is_empty() {
            "(no catalog data)".to_string()
        } else {
            ctx.catalog_summary.clone()
        };

        let explain = if ctx.explain_summary.is_empty() {
            "(no explain data)".to_string()
        } else {
            ctx.explain_summary.clone()
        };

        let findings = if ctx.rule_findings.is_empty() {
            "(no rule findings)".to_string()
        } else {
            ctx.rule_findings.join("\n")
        };

        self.template
            .replace("{{system_context}}", &system_context)
            .replace("{{sanitized_sql}}", &sanitized_sql)
            .replace("{{catalog_summary}}", &catalog)
            .replace("{{explain_summary}}", &explain)
            .replace("{{rule_findings}}", &findings)
    }

    /// 估算 Prompt 的 token 数量（粗略：每 4 字符 ≈ 1 token）
    pub fn estimate_tokens(&self, ctx: &SanitizedContext) -> usize {
        self.build(ctx).len() / 4
    }
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::default_template()
    }
}

const DEFAULT_TEMPLATE: &str = r#"# Role
你是 InsightDB 的数据库性能诊断助手。你只能基于以下提供的上下文进行分析。

# Rules
- 严格区分事实证据和模型推断。
- 不生成 DROP、TRUNCATE、批量 DELETE/UPDATE、DDL 等危险操作建议。
- 不声称已经验证未被提供的数据。
- 如果证据不足，明确说明缺失信息。
- 建议必须是可验证、可操作的。
- 输出中文。

# System Context
{{system_context}}

# Sanitized SQL
{{sanitized_sql}}

# Catalog Summary
{{catalog_summary}}

# Explain Summary
{{explain_summary}}

# Rule Findings
{{rule_findings}}

# Output Format
请严格按以下 JSON 格式输出（不要包含其他文字）：

{
  "problem_summary": "一句话概述性能问题",
  "evidence": [
    {
      "statement": "证据描述",
      "source": "explain_plan 或 rule_<RULE_ID> 或 schema_metadata",
      "is_fact": true
    }
  ],
  "recommendations": [
    {
      "action": "具体操作",
      "benefit": "预期收益",
      "risk": "风险",
      "verification_sql": "用于验证的 SQL 或 null",
      "is_inference": true
    }
  ],
  "confidence": 0.85
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SanitizedContext;

    fn test_context() -> SanitizedContext {
        SanitizedContext {
            sanitized_sql: "SELECT * FROM t_1 WHERE c_1 = '<string_literal>'".into(),
            catalog_summary: "Table t_1 (50000 rows)\n  c_1 varchar NOT NULL".into(),
            explain_summary: "[Seq Scan] t_1 ~50000 rows filter=...".into(),
            rule_findings: vec![
                "[High|FULL_TABLE_SCAN] 全表扫描: t_1 估算扫描 50000 行 → 为过滤列创建索引".into(),
            ],
            db_type: "mysql".into(),
            db_version: "8.0.30".into(),
            identifier_mapping: vec![
                ("table:users".into(), "t_1".into()),
            ],
        }
    }

    #[test]
    fn test_build_contains_required_sections() {
        let ctx = test_context();
        let builder = PromptBuilder::default();
        let prompt = builder.build(&ctx);

        assert!(prompt.contains("# Role"));
        assert!(prompt.contains("# Rules"));
        assert!(prompt.contains("# System Context"));
        assert!(prompt.contains("DB Type: mysql"));
        assert!(prompt.contains("8.0.30"));
        assert!(prompt.contains("# Sanitized SQL"));
        assert!(prompt.contains("t_1"));
        assert!(prompt.contains("# Catalog Summary"));
        assert!(prompt.contains("# Explain Summary"));
        assert!(prompt.contains("# Rule Findings"));
        assert!(prompt.contains("FULL_TABLE_SCAN"));
        assert!(prompt.contains("# Output Format"));
    }

    #[test]
    fn test_build_does_not_contain_raw_identifiers() {
        let ctx = test_context();
        let builder = PromptBuilder::default();
        let prompt = builder.build(&ctx);

        // 不应包含原始标识符
        assert!(!prompt.contains("users"));
        assert!(!prompt.contains("email"));
        assert!(!prompt.contains("test@example"));
    }

    #[test]
    fn test_empty_context_handles_gracefully() {
        let ctx = SanitizedContext {
            sanitized_sql: "SELECT 1".into(),
            catalog_summary: "".into(),
            explain_summary: "".into(),
            rule_findings: vec![],
            db_type: "postgresql".into(),
            db_version: "15.0".into(),
            identifier_mapping: vec![],
        };
        let builder = PromptBuilder::default();
        let prompt = builder.build(&ctx);
        assert!(prompt.contains("no catalog data"));
        assert!(prompt.contains("no explain data"));
        assert!(prompt.contains("no rule findings"));
    }

    #[test]
    fn test_estimate_tokens_is_positive() {
        let ctx = test_context();
        let builder = PromptBuilder::default();
        let tokens = builder.estimate_tokens(&ctx);
        assert!(tokens > 0);
        assert!(tokens < 5000);
    }

    #[test]
    fn test_custom_template() {
        let ctx = test_context();
        let builder = PromptBuilder::new("Problem: {{rule_findings}}\nSQL: {{sanitized_sql}}");
        let prompt = builder.build(&ctx);
        assert!(prompt.starts_with("Problem:"));
        assert!(prompt.contains("t_1"));
        assert!(!prompt.contains("{{"));
    }
}
