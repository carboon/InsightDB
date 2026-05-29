use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use tokio::sync::mpsc;
use crate::types::{AiExplanation, EvidenceItem, Recommendation};

/// AI 客户端错误
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("模型调用失败: {0}")]
    CallFailed(String),

    #[error("流式读取中断: {0}")]
    StreamError(String),

    #[error("响应解析失败: {0}")]
    ParseError(String),

    #[error("超时: {0}")]
    Timeout(String),

    #[error("限流: {0}")]
    RateLimited(String),
}

/// AI 流式事件
#[derive(Debug, Clone)]
pub enum AiStreamEvent {
    /// 文本增量
    TextDelta(String),
    /// 流结束
    Done,
    /// 错误
    Error(String),
}

type AiStream = Pin<Box<dyn Stream<Item = AiStreamEvent> + Send>>;

/// 可插拔的 AI 客户端 trait
#[async_trait]
pub trait AiClient: Send + Sync {
    /// 同步调用 AI，返回解析后的解释
    async fn explain(&self, prompt: &str) -> Result<AiExplanation, AiError>;

    /// 流式调用 AI，通过 channel 逐 token 返回
    async fn explain_stream(&self, prompt: &str) -> Result<AiStream, AiError>;

    /// 模型名称
    fn model_name(&self) -> &str;
}

// ── Mock 客户端（用于测试和 AI 不可用时的 fallback）──

/// 基于规则直接将 findings 翻译为解释的 Mock 客户端
pub struct MockAiClient {
    model: String,
}

impl MockAiClient {
    pub fn new(model: impl Into<String>) -> Self {
        Self { model: model.into() }
    }

    /// 从 prompt 文本中提取规则 findings 并构建解释
    fn build_from_prompt(&self, prompt: &str) -> AiExplanation {
        let mut evidence = Vec::new();
        let mut recommendations = Vec::new();

        // 提取规则 findings
        for line in prompt.lines() {
            let line = line.trim();
            if line.contains("FULL_TABLE_SCAN") {
                evidence.push(EvidenceItem {
                    statement: extract_between(line, "全表扫描: ", " →").unwrap_or_else(|| "全表扫描".to_string()),
                    source: "rule_FULL_TABLE_SCAN".into(),
                    is_fact: true,
                });
                recommendations.push(Recommendation {
                    action: "为过滤条件中涉及的列创建索引，优先选择选择性高的列".into(),
                    benefit: "将全表扫描变为索引扫描，大幅降低扫描行数".into(),
                    risk: "索引会增加写入成本和存储空间".into(),
                    verification_sql: Some("EXPLAIN <original_query> 确认不再是 seq_scan".into()),
                    is_inference: true,
                });
            }
            if line.contains("MISSING_INDEX") {
                evidence.push(EvidenceItem {
                    statement: extract_between(line, "索引缺失: ", " →").unwrap_or_else(|| "索引缺失".to_string()),
                    source: "rule_MISSING_INDEX".into(),
                    is_fact: true,
                });
                recommendations.push(Recommendation {
                    action: "为过滤条件涉及的列创建索引".into(),
                    benefit: "消除全表扫描".into(),
                    risk: "索引维护开销".into(),
                    verification_sql: Some("EXPLAIN 验证 access_type 变为 RANGE/REF".into()),
                    is_inference: true,
                });
            }
            if line.contains("FILESORT") {
                evidence.push(EvidenceItem {
                    statement: extract_between(line, "外部排序: ", " →").unwrap_or_else(|| "filesort".to_string()),
                    source: "rule_FILESORT".into(),
                    is_fact: true,
                });
                recommendations.push(Recommendation {
                    action: "创建覆盖 WHERE 和 ORDER BY 的联合索引".into(),
                    benefit: "消除 filesort".into(),
                    risk: "联合索引会增加索引大小".into(),
                    verification_sql: Some("EXPLAIN 确认 filesort 消失".into()),
                    is_inference: true,
                });
            }
            if line.contains("NESTED_LOOP_RISK") {
                evidence.push(EvidenceItem {
                    statement: extract_between(line, "Nested Loop 连接: ", " →").unwrap_or_else(|| "Nested Loop".to_string()),
                    source: "rule_NESTED_LOOP_RISK".into(),
                    is_fact: true,
                });
                recommendations.push(Recommendation {
                    action: "为内表连接列创建索引或增大 work_mem 使优化器选择 Hash Join".into(),
                    benefit: "避免 Nested Loop 导致的指数级扫描".into(),
                    risk: "增加内存使用或索引维护成本".into(),
                    verification_sql: Some("EXPLAIN ANALYZE 对比优化前后耗时".into()),
                    is_inference: true,
                });
            }
        }

        // 如果没有任何规则命中，返回空
        if evidence.is_empty() {
            return AiExplanation {
                problem_summary: "基于提供的执行计划和规则分析，未发现明确的性能问题。".into(),
                evidence,
                recommendations,
                confidence: 0.5,
                model: self.model.clone(),
            };
        }

        let problem_summary = format!(
            "诊断发现 {} 个问题：{}",
            evidence.len(),
            evidence.iter().map(|e| e.statement.as_str()).collect::<Vec<_>>().join("；")
        );

        AiExplanation {
            problem_summary,
            evidence,
            recommendations,
            confidence: 0.85,
            model: self.model.clone(),
        }
    }
}

fn extract_between<'a>(s: &'a str, start: &str, end: &str) -> Option<String> {
    let s = s.split(start).nth(1)?;
    let s = s.split(end).next()?;
    Some(s.trim().to_string())
}

#[async_trait]
impl AiClient for MockAiClient {
    async fn explain(&self, prompt: &str) -> Result<AiExplanation, AiError> {
        Ok(self.build_from_prompt(prompt))
    }

    async fn explain_stream(&self, prompt: &str) -> Result<AiStream, AiError> {
        let result = self.build_from_prompt(prompt);
        let json = serde_json::to_string(&result)
            .unwrap_or_else(|_| r#"{"problem_summary":"解析失败"}"#.into());

        let (tx, rx) = mpsc::channel::<AiStreamEvent>(32);
        let _model = self.model.clone();

        tokio::spawn(async move {
            // 模拟流式输出：逐字符发送
            for chunk in json.chars().collect::<Vec<_>>().chunks(4) {
                let text: String = chunk.iter().collect();
                if tx.send(AiStreamEvent::TextDelta(text)).await.is_err() {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            let _ = tx.send(AiStreamEvent::Done).await;
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// Noop 客户端：当 AI 不可用时返回空解释
pub struct NoopAiClient;

#[async_trait]
impl AiClient for NoopAiClient {
    async fn explain(&self, _prompt: &str) -> Result<AiExplanation, AiError> {
        Ok(AiExplanation {
            problem_summary: "AI 服务不可用，以下仅展示规则引擎的诊断结果。".into(),
            evidence: vec![],
            recommendations: vec![],
            confidence: 0.0,
            model: "noop".into(),
        })
    }

    async fn explain_stream(&self, _prompt: &str) -> Result<AiStream, AiError> {
        let (tx, rx) = mpsc::channel::<AiStreamEvent>(1);
        tokio::spawn(async move {
            let _ = tx.send(AiStreamEvent::TextDelta(
                r#"{"problem_summary":"AI 服务不可用","evidence":[],"recommendations":[],"confidence":0.0,"model":"noop"}"#.into()
            )).await;
            let _ = tx.send(AiStreamEvent::Done).await;
        });
        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    fn model_name(&self) -> &str {
        "noop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_mock_explain_with_full_table_scan() {
        let prompt = "[High|FULL_TABLE_SCAN] 全表扫描: 表 t_1 估算扫描 50000 行 → 为 t_1 创建索引";
        let client = MockAiClient::new("mock-v1");
        let result = client.explain(prompt).await.unwrap();

        assert_eq!(result.model, "mock-v1");
        assert!(result.confidence > 0.0);
        assert!(!result.evidence.is_empty());
        assert!(result.evidence[0].is_fact);
        assert!(!result.recommendations.is_empty());
        assert!(result.recommendations[0].is_inference);
    }

    #[tokio::test]
    async fn test_mock_explain_empty_context() {
        let prompt = "empty prompt with no findings";
        let client = MockAiClient::new("mock-v1");
        let result = client.explain(prompt).await.unwrap();

        assert!(result.evidence.is_empty());
        assert!(result.problem_summary.contains("未发现"));
    }

    #[tokio::test]
    async fn test_mock_stream_output() {
        let prompt = "[Medium|FILESORT] 外部排序: 表 t_1 filesort → 创建联合索引";
        let client = MockAiClient::new("mock-v1");
        let stream = client.explain_stream(prompt).await.unwrap();

        tokio::pin!(stream);
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event);
        }

        assert!(!events.is_empty());
        let has_done = events.iter().any(|e| matches!(e, AiStreamEvent::Done));
        assert!(has_done);
    }

    #[tokio::test]
    async fn test_noop_client_returns_empty() {
        let client = NoopAiClient;
        let result = client.explain("anything").await.unwrap();
        assert!(result.evidence.is_empty());
        assert!(result.confidence == 0.0);
        assert_eq!(client.model_name(), "noop");
    }

    #[tokio::test]
    async fn test_mock_client_model_name() {
        let client = MockAiClient::new("gpt-4-test");
        assert_eq!(client.model_name(), "gpt-4-test");
    }
}
