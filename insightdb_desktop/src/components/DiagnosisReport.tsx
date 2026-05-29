import { useAppStore } from "../stores/appStore";
import { AiPanel } from "./AiPanel";

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

export function DiagnosisReport() {
  const { diagnosis, diagnosisRunning, diagnosisError } = useAppStore();

  if (diagnosisRunning) {
    return <div className="panel diagnosis-panel loading">诊断中...</div>;
  }

  if (diagnosisError) {
    return <div className="panel diagnosis-panel error-text">{diagnosisError}</div>;
  }

  if (!diagnosis) {
    return null;
  }

  const { findings, summary, overall_severity, total_findings, sql } = diagnosis;

  return (
    <div className="panel diagnosis-panel">
      <div className="report-header">
        <h2>诊断报告</h2>
        <span style={{ color: severityColor(overall_severity), fontWeight: 600 }}>
          {overall_severity}
        </span>
        <span className="text-sm text-muted">共 {total_findings} 个问题</span>
      </div>

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

      <AiPanel diagnosis={diagnosis} />
    </div>
  );
}
