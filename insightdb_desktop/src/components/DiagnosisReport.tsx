import { useState } from "react";
import { api } from "../api/adapter";
import { useAppStore } from "../stores/appStore";
import { AiPanel } from "./AiPanel";
import { ExplainViewer } from "./ExplainViewer";

const severityColor = (severity: string): string => {
  switch (severity) {
    case "Critical":
    case "High":
      return "#ef4444";
    case "Medium":
      return "#f59e0b";
    case "Low":
      return "#3b82f6";
    default:
      return "#6b7280";
  }
};

const isDangerous = (msg: string | null) => msg?.startsWith("[安全拦截]");

export function DiagnosisReport() {
  const { diagnosis, diagnosisRunning, diagnosisError, viewMode } = useAppStore();
  const { setHistoryLoading, setHistoryList, setHistoryError } = useAppStore();
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState<string | null>(null);

  if (diagnosisRunning) {
    return <div className="panel diagnosis-panel loading">诊断中...</div>;
  }

  if (diagnosisError) {
    const dangerous = isDangerous(diagnosisError);
    return (
      <div className="panel diagnosis-panel">
        <div
          className={dangerous ? "dangerous-error" : "error-text"}
          style={dangerous ? {
            padding: 16,
            background: "var(--danger-bg, #fff5f5)",
            border: "2px solid var(--danger)",
            borderRadius: 4,
            color: "var(--danger)",
            fontWeight: 600,
          } : undefined}
        >
          {dangerous && <span style={{ marginRight: 8 }}>&#x26A0;</span>}
          {dangerous ? diagnosisError.replace("[安全拦截] ", "") : diagnosisError}
        </div>
      </div>
    );
  }

  if (!diagnosis) {
    if (viewMode === "history") {
      return (
        <div className="panel diagnosis-panel">
          <div className="text-muted" style={{ padding: 24, textAlign: "center" }}>
            请从侧边栏选择一份历史报告查看详情
          </div>
        </div>
      );
    }
    return null;
  }

  const { findings, summary, overall_severity, total_findings, sql } = diagnosis;

  const handleSave = async () => {
    if (!diagnosis) return;
    setSaving(true);
    setSaveMsg(null);
    try {
      const json = JSON.stringify(diagnosis);
      await api.saveReport(json);
      setSaveMsg("已保存");
      setHistoryLoading(true);
      try {
        const list = await api.listReports();
        setHistoryList(list);
      } catch (e) {
        setHistoryError(String(e));
      }
    } catch (e) {
      setSaveMsg(`保存失败: ${e}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="panel diagnosis-panel">
      <div className="report-header">
        <h2>诊断报告</h2>
        <span style={{ color: severityColor(overall_severity), fontWeight: 600 }}>
          {overall_severity}
        </span>
        <span className="text-sm text-muted">共 {total_findings} 个问题</span>
        {viewMode === "diagnosis" && (
          <button
            className="btn btn-sm btn-secondary"
            onClick={handleSave}
            disabled={saving}
          >
            {saving ? "保存中..." : "保存报告"}
          </button>
        )}
      </div>

      {saveMsg && <div className="text-sm" style={{ marginBottom: 4, color: saveMsg.includes("失败") ? "var(--danger)" : "var(--accent)" }}>{saveMsg}</div>}

      <div className="report-summary">{summary}</div>

      <div className="report-sql text-sm text-muted">SQL: {sql}</div>

      {findings.map((f, i) => (
        <div key={i} className="finding-card">
          <div className="finding-header">
            <span className="badge" style={{ backgroundColor: severityColor(f.severity) }}>
              {f.severity}
            </span>
            <span className="finding-id" title="规则 ID">{f.id}</span>
            <strong>{f.title}</strong>
            <span className="text-sm text-muted">置信度 {(f.confidence * 100).toFixed(0)}%</span>
          </div>

          <div className="finding-section">
            <div className="finding-label label-fact">证据</div>
            <p>{f.evidence}</p>
          </div>

          <div className="finding-section">
            <div className="finding-label label-inference">建议</div>
            <p>{f.recommendation}</p>
          </div>

          <div className="finding-section">
            <div className="finding-label label-risk">风险</div>
            <p>{f.risk}</p>
          </div>

          <div className="finding-section">
            <div className="finding-label label-verify">验证</div>
            <p className="text-sm">{f.verification}</p>
          </div>
        </div>
      ))}

      <ExplainViewer report={diagnosis} />

      <AiPanel diagnosis={diagnosis} />
    </div>
  );
}
