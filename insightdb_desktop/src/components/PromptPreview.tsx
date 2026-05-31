import { useState } from "react";
import { api } from "../api/adapter";
import type { DiagnosisReport, SanitizedContext } from "../api/types";

interface Props {
  diagnosis: DiagnosisReport;
  onClose: () => void;
}

export function PromptPreview({ diagnosis, onClose }: Props) {
  const [ctx, setCtx] = useState<SanitizedContext | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useState(() => {
    (async () => {
      try {
        const json = JSON.stringify(diagnosis);
        const result = await api.sanitizeContext(json);
        setCtx(result);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    })();
  });

  if (loading) {
    return (
      <div className="panel" style={{ marginTop: 12 }}>
        <div className="loading">正在脱敏上下文...</div>
      </div>
    );
  }

  if (error || !ctx) {
    return (
      <div className="panel" style={{ marginTop: 12 }}>
        <div className="error-text">{error || "脱敏失败"}</div>
        <button onClick={onClose} className="btn btn-sm">返回</button>
      </div>
    );
  }

  return (
    <div className="panel" style={{ marginTop: 12, maxHeight: "70vh", overflowY: "auto" }}>
      <div className="flex-row" style={{ justifyContent: "space-between", marginBottom: 8 }}>
        <h3>Prompt 预览</h3>
        <button onClick={onClose} className="btn btn-sm btn-secondary">关闭</button>
      </div>

      <div className="text-sm text-muted" style={{ marginBottom: 12 }}>
        以下是将发送给 AI 模型的脱敏上下文（含规则引擎发现的结果）
      </div>

      <Section title="数据库环境">
        <div>类型: {ctx.db_type}</div>
        <div>版本: {ctx.db_version}</div>
      </Section>

      <Section title="脱敏 SQL">
        <pre className="code-block">{ctx.sanitized_sql}</pre>
      </Section>

      <Section title="Catalog 摘要">
        <pre className="code-block">{ctx.catalog_summary || "(无)"}</pre>
      </Section>

      <Section title="Explain 摘要">
        <pre className="code-block">{ctx.explain_summary || "(无)"}</pre>
      </Section>

      <Section title="规则引擎发现 ({ctx.rule_findings.length} 条)">
        {ctx.rule_findings.length === 0 ? (
          <div className="text-muted">(无规则发现)</div>
        ) : (
          ctx.rule_findings.map((f, i) => (
            <div key={i} className="text-sm" style={{ padding: "2px 0" }}>
              {f}
            </div>
          ))
        )}
      </Section>

      <Section title="标识符映射">
        {ctx.identifier_mapping.length === 0 ? (
          <div className="text-muted">(无映射)</div>
        ) : (
          <div className="text-sm" style={{ maxHeight: 120, overflowY: "auto" }}>
            {ctx.identifier_mapping.map(([orig, mapped], i) => (
              <div key={i} style={{ fontFamily: "monospace" }}>
                {orig} → {mapped}
              </div>
            ))}
          </div>
        )}
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: 12 }}>
      <div className="text-sm" style={{ fontWeight: 600, marginBottom: 4, color: "var(--accent)" }}>
        {title}
      </div>
      {children}
    </div>
  );
}
