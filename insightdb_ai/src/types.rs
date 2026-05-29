use serde::{Deserialize, Serialize};

/// AI 解释输出：明确区分事实证据和模型推断
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiExplanation {
    /// 问题摘要
    pub problem_summary: String,

    /// 基于规则引擎和元数据的证据（事实）
    pub evidence: Vec<EvidenceItem>,

    /// AI 推断的建议
    pub recommendations: Vec<Recommendation>,

    /// 总体置信度 0.0~1.0
    pub confidence: f64,

    /// 模型名称/版本
    pub model: String,
}

/// 证据项：明确标注为事实，来源可追溯
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceItem {
    /// 证据描述
    pub statement: String,

    /// 来源（如 "explain_plan", "rule_FULL_TABLE_SCAN", "schema_metadata"）
    pub source: String,

    /// 是否为确定性事实（规则引擎输出 = true，AI 推断 = false）
    pub is_fact: bool,
}

/// 优化建议
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Recommendation {
    /// 建议描述
    pub action: String,

    /// 预期收益
    pub benefit: String,

    /// 执行风险
    pub risk: String,

    /// 验证 SQL（可为空）
    pub verification_sql: Option<String>,

    /// 该建议是从规则引擎来的事实还是 AI 推断
    pub is_inference: bool,
}

/// 脱敏后的诊断上下文（发送给模型的内容）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedContext {
    /// 脱敏后的 SQL
    pub sanitized_sql: String,

    /// 脱敏后的元数据摘要
    pub catalog_summary: String,

    /// 脱敏后的执行计划摘要
    pub explain_summary: String,

    /// 规则命中列表
    pub rule_findings: Vec<String>,

    /// 数据库类型
    pub db_type: String,

    /// 数据库版本
    pub db_version: String,

    /// 原始→脱敏 的映射表（用于 UI 反向展示）
    pub identifier_mapping: Vec<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evidence_is_fact_flag() {
        let fact = EvidenceItem {
            statement: "全表扫描 users 表".into(),
            source: "rule_FULL_TABLE_SCAN".into(),
            is_fact: true,
        };
        assert!(fact.is_fact);

        let inference = EvidenceItem {
            statement: "可能存在索引缺失".into(),
            source: "ai_inference".into(),
            is_fact: false,
        };
        assert!(!inference.is_fact);
    }

    #[test]
    fn test_recommendation_inference_flag() {
        let rec = Recommendation {
            action: "创建 idx_email 索引".into(),
            benefit: "加速邮箱查询".into(),
            risk: "写入性能下降 5%".into(),
            verification_sql: Some("EXPLAIN SELECT * FROM users WHERE email = 'test'".into()),
            is_inference: true,
        };
        assert!(rec.is_inference);
    }

    #[test]
    fn test_ai_explanation_serialization() {
        let expl = AiExplanation {
            problem_summary: "查询存在全表扫描".into(),
            evidence: vec![
                EvidenceItem {
                    statement: "users 表全表扫描 50000 行".into(),
                    source: "explain_plan".into(),
                    is_fact: true,
                },
            ],
            recommendations: vec![
                Recommendation {
                    action: "为 email 列创建索引".into(),
                    benefit: "扫描行数降至 1".into(),
                    risk: "索引维护开销".into(),
                    verification_sql: Some("EXPLAIN SELECT ...".into()),
                    is_inference: true,
                },
            ],
            confidence: 0.85,
            model: "mock-gpt-4".into(),
        };

        let json = serde_json::to_string(&expl).unwrap();
        let parsed: AiExplanation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.problem_summary, "查询存在全表扫描");
        assert_eq!(parsed.evidence.len(), 1);
        assert_eq!(parsed.confidence, 0.85);
    }
}
