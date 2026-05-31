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

// ── 真实 AI 客户端（OpenRouter / OpenAI 兼容 API）──

use reqwest::Client as HttpClient;
use std::env;

pub struct RealAiClient {
    model: String,
    api_url: String,
    api_key: String,
    http: HttpClient,
}

impl RealAiClient {
    pub fn new(model: impl Into<String>) -> Self {
        let key = env::var("INSIGHTDB_AI_KEY").unwrap_or_default();
        let url = env::var("INSIGHTDB_AI_URL")
            .unwrap_or_else(|_| "https://openrouter.ai/api/v1/chat/completions".into());
        Self {
            model: model.into(),
            api_url: url,
            api_key: key,
            http: HttpClient::new(),
        }
    }

    pub fn with_key(model: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            api_url: "https://openrouter.ai/api/v1/chat/completions".into(),
            api_key: api_key.into(),
            http: HttpClient::new(),
        }
    }

    async fn call_api(&self, prompt: &str) -> Result<String, AiError> {
        if self.api_key.is_empty() || self.api_key == "MOCK" {
            return Err(AiError::CallFailed(
                "API key 未配置。请设置环境变量 INSIGHTDB_AI_KEY 或使用 MOCK 客户端。".into(),
            ));
        }

        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "user", "content": prompt }
            ],
            "temperature": 0.3,
            "max_tokens": 2048,
        });

        let resp = self
            .http
            .post(&self.api_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::CallFailed(format!("网络请求失败: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(if status.as_u16() == 429 {
                AiError::RateLimited(text)
            } else {
                AiError::CallFailed(format!("API 返回 {status}: {text}"))
            });
        }

        let result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AiError::ParseError(format!("响应解析失败: {e}")))?;

        let content = result["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| AiError::ParseError("API 响应缺少 choices[0].message.content".into()))?;

        Ok(content.to_string())
    }

    fn parse_explanation(&self, response: &str) -> Result<AiExplanation, AiError> {
        let content = extract_json_block(response);
        serde_json::from_str(&content)
            .map_err(|e| AiError::ParseError(format!("JSON 解析失败: {e}")))
    }
}

fn extract_json_block(text: &str) -> String {
    if let Some(start) = text.find('{') {
        let mut depth = 0;
        let mut in_string = false;
        let mut escape = false;
        for (i, c) in text[start..].char_indices() {
            if escape {
                escape = false;
                continue;
            }
            match c {
                '\\' => escape = true,
                '"' => in_string = !in_string,
                '{' if !in_string => depth += 1,
                '}' if !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        return text[start..start + i + 1].to_string();
                    }
                }
                _ => {}
            }
        }
    }
    text.to_string()
}

#[async_trait]
impl AiClient for RealAiClient {
    async fn explain(&self, prompt: &str) -> Result<AiExplanation, AiError> {
        let response = self.call_api(prompt).await?;
        self.parse_explanation(&response)
    }

    async fn explain_stream(&self, _prompt: &str) -> Result<AiStream, AiError> {
        let (tx, rx) = mpsc::channel::<AiStreamEvent>(1);
        let _ = tx
            .send(AiStreamEvent::Error(
                "RealAiClient 暂不支持流式输出，请使用非流式接口".into(),
            ))
            .await;
        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    fn model_name(&self) -> &str {
        &self.model
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

    #[test]
    fn test_extract_json_block_pure_json() {
        let json = r#"{"problem_summary":"test","evidence":[],"recommendations":[],"confidence":0.5,"model":"test"}"#;
        let result = extract_json_block(json);
        assert_eq!(result, json);
    }

    #[test]
    fn test_extract_json_block_with_prefix_text() {
        let text = r#"Here is the analysis:
```json
{"problem_summary":"test","evidence":[],"recommendations":[],"confidence":0.5,"model":"test"}
```
Hope it helps!"#;
        let result = extract_json_block(text);
        assert!(result.starts_with("{"));
        assert!(result.ends_with("}"));
        assert!(result.contains("\"problem_summary\""));
    }

    #[test]
    fn test_extract_json_block_nested_braces() {
        let text = r#"{"problem_summary":"ok","evidence":[{"statement":"x","source":"s","is_fact":true}],"recommendations":[],"confidence":0.5,"model":"m"}"#;
        let result = extract_json_block(text);
        assert!(result.starts_with("{"));
        assert!(result.ends_with("}"));
        assert!(result.contains("\"statement\":\"x\""));
    }

    #[test]
    fn test_extract_json_block_string_with_braces() {
        let text = r#"{"a":"{b}","c":"}"}"#;
        let result = extract_json_block(text);
        assert_eq!(result, text);
    }

    #[test]
    fn test_extract_json_block_no_json() {
        let text = "no json here at all";
        let result = extract_json_block(text);
        assert_eq!(result, text);
    }

    #[test]
    fn test_real_client_parse_explanation_valid() {
        let client = RealAiClient::with_key("test-model", "MOCK");
        let json = r#"{"problem_summary":"test","evidence":[],"recommendations":[],"confidence":0.5,"model":"test"}"#;
        let result = client.parse_explanation(json).unwrap();
        assert_eq!(result.problem_summary, "test");
        assert_eq!(result.confidence, 0.5);
    }

    #[test]
    fn test_real_client_parse_explanation_with_wrapper() {
        let client = RealAiClient::with_key("test-model", "MOCK");
        let text = "asdf\n```json\n{\"problem_summary\":\"ok\",\"evidence\":[],\"recommendations\":[{\"action\":\"create index\",\"benefit\":\"fast\",\"risk\":\"write cost\",\"verification_sql\":null,\"is_inference\":true}],\"confidence\":0.9,\"model\":\"gpt4\"}\n```";
        let result = client.parse_explanation(text).unwrap();
        assert_eq!(result.problem_summary, "ok");
        assert_eq!(result.recommendations.len(), 1);
        assert_eq!(result.confidence, 0.9);
    }

    #[test]
    fn test_real_client_parse_explanation_invalid() {
        let client = RealAiClient::with_key("test-model", "MOCK");
        let result = client.parse_explanation("not json {");
        assert!(result.is_err());
    }
}
