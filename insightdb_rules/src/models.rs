use serde::{Deserialize, Serialize};

/// 严重级别
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Critical = 0,
    High = 1,
    Medium = 2,
    Low = 3,
    Info = 4,
}

/// 规则命中结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuleFinding {
    /// 稳定规则 ID，不随文案变化
    pub id: String,

    /// 严重级别
    pub severity: Severity,

    /// 诊断标题
    pub title: String,

    /// 证据说明（引用具体事实）
    pub evidence: String,

    /// 优化建议
    pub recommendation: String,

    /// 不执行建议的风险
    pub risk: String,

    /// 验证方式
    pub verification: String,

    /// 置信度 (0.0 ~ 1.0)
    pub confidence: f64,
}

/// 诊断上下文：供给规则引擎的全部输入
#[derive(Debug, Clone)]
pub struct DiagnosisContext<'a> {
    pub plan: &'a insightdb_explain::PlanNode,
    pub schema: &'a insightdb_catalog::SchemaInfo,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical < Severity::High);
        assert!(Severity::High < Severity::Medium);
        assert!(Severity::Medium < Severity::Low);
        assert!(Severity::Low < Severity::Info);
    }

    #[test]
    fn test_rule_finding_serialization() {
        let finding = RuleFinding {
            id: "FULL_TABLE_SCAN".into(),
            severity: Severity::High,
            title: "全表扫描".into(),
            evidence: "表 users 估算扫描 10000 行".into(),
            recommendation: "为过滤列创建索引".into(),
            risk: "数据增长后性能线性下降".into(),
            verification: "CREATE INDEX ... 后重新 EXPLAIN".into(),
            confidence: 0.9,
        };
        let json = serde_json::to_string(&finding).unwrap();
        let parsed: RuleFinding = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "FULL_TABLE_SCAN");
        assert_eq!(parsed.severity, Severity::High);
        assert_eq!(parsed.confidence, 0.9);
    }
}
