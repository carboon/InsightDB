import { useState } from "react";
import { api } from "../api/adapter";
import { useAppStore } from "../stores/appStore";
import type { DiagnosisReport, AiExplanation } from "../api/types";

interface Props {
  diagnosis: DiagnosisReport;
}

export function AiPanel({ diagnosis }: Props) {
  const { aiExplanation, aiRunning, aiError, setAiExplanation, setAiRunning, setAiError } =
    useAppStore();
  const [expanded, setExpanded] = useState(false);

  const handleAskAi = async () => {
    if (!diagnosis) return;
    setAiRunning(true);
    setAiError(null);
    setExpanded(true);
    try {
      const json = JSON.stringify(diagnosis);
      const result = await api.aiExplain(json);
      setAiExplanation(result);
    } catch (e) {
      setAiError(String(e));
    }
  };

  if (!expanded) {
    return (
      <div className="ai-trigger">
        <button onClick={handleAskAi} disabled={aiRunning} className="btn btn-accent btn-sm">
          {aiRunning ? "AI 分析中..." : "AI 解释诊断结果"}
        </button>
      </div>
    );
  }

  if (aiRunning) {
    return <div className="panel ai-panel loading">AI 分析中...</div>;
  }

  if (aiError) {
    return (
      <div className="panel ai-panel">
        <div className="error-text">{aiError}</div>
        <button onClick={handleAskAi} className="btn btn-sm">重试</button>
      </div>
    );
  }

  if (!aiExplanation) return null;

  return (
    <div className="panel ai-panel">
      <div className="ai-header">
        <h3>AI 诊断解释</h3>
        <span className="text-xs text-muted">模型: {aiExplanation.model} · 置信度 {(aiExplanation.confidence * 100).toFixed(0)}%</span>
      </div>

      <div className="ai-summary">
        <strong>摘要：</strong>{aiExplanation.problem_summary}
      </div>

      {aiExplanation.evidence.length > 0 && (
        <div className="ai-section">
          <div className="label-fact">证据事实</div>
          {aiExplanation.evidence.map((e, i) => (
            <div key={i} className="evidence-item">
              <span className="tag fact-tag">事实</span>
              <span>{e.statement}</span>
              <span className="text-xs text-muted ml-sm">来源: {e.source}</span>
            </div>
          ))}
        </div>
      )}

      {aiExplanation.recommendations.length > 0 && (
        <div className="ai-section">
          <div className="label-inference">AI 推断建议</div>
          {aiExplanation.recommendations.map((r, i) => (
            <div key={i} className="recommendation-item">
              <span className="tag inference-tag">推断</span>
              <div>
                <p><strong>操作：</strong>{r.action}</p>
                <p className="text-sm text-muted">收益：{r.benefit}</p>
                <p className="text-sm text-muted">风险：{r.risk}</p>
                {r.verification_sql && (
                  <p className="text-sm">
                    <code>{r.verification_sql}</code>
                  </p>
                )}
              </div>
            </div>
          ))}
        </div>
      )}

      <button onClick={() => setExpanded(false)} className="btn btn-secondary btn-sm">收起</button>
    </div>
  );
}
