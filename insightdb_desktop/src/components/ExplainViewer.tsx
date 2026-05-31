import { useState } from "react";
import type { DiagnosisReport } from "../api/types";

interface Props {
  report: DiagnosisReport;
}

interface PlanEntry {
  node_type: string;
  table_name?: string;
  access_method?: string;
  estimated_rows?: number;
  actual_rows?: number;
  estimated_cost?: number;
  actual_time?: number;
  filter?: string;
  index_name?: string;
  join_type?: string;
  uses_filesort?: boolean;
  uses_temporary?: boolean;
  sort_keys?: string[];
  children?: PlanEntry[];
}

export function ExplainViewer({ report }: Props) {
  const [expanded, setExpanded] = useState(false);
  const plan = report.plan as unknown as PlanEntry;

  if (!plan) {
    return null;
  }

  if (!expanded) {
    return (
      <div className="explain-trigger">
        <button onClick={() => setExpanded(true)} className="btn btn-secondary btn-sm">
          查看执行计划
        </button>
      </div>
    );
  }

  return (
    <div className="panel" style={{ marginTop: 12 }}>
      <div className="flex-row" style={{ justifyContent: "space-between", marginBottom: 8 }}>
        <h3>执行计划</h3>
        <button onClick={() => setExpanded(false)} className="btn btn-sm btn-secondary">
          收起
        </button>
      </div>
      <div className="plan-tree">
        <PlanNodeView node={plan} depth={0} />
      </div>
    </div>
  );
}

const costColor = (rows: number | undefined): string => {
  if (!rows) return "var(--muted)";
  if (rows > 50000) return "var(--danger)";
  if (rows > 10000) return "#f59e0b";
  return "var(--accent)";
};

function PlanNodeView({ node, depth }: { node: PlanEntry; depth: number }) {
  const indent = depth * 16;

  return (
    <div style={{ marginLeft: indent, marginBottom: depth === 0 ? 8 : 4 }}>
      <div className="plan-node" style={{ padding: "4px 8px", borderLeft: node.children ? `2px solid var(--accent)` : "none" }}>
        <span className="badge" style={{ backgroundColor: "var(--accent)", marginRight: 6 }}>
          {node.node_type}
        </span>

        {node.table_name && (
          <span className="plan-tag">{node.table_name}</span>
        )}
        {node.index_name && (
          <span className="plan-tag" style={{ background: "var(--primary-bg)", color: "var(--primary)" }}>
            索引: {node.index_name}
          </span>
        )}
        {node.join_type && (
          <span className="plan-tag" style={{ background: "#fef3c7", color: "#92400e" }}>
            {node.join_type} Join
          </span>
        )}
        {node.access_method && (
          <span className="plan-tag">{node.access_method}</span>
        )}

        <div className="plan-stats text-xs text-muted" style={{ marginTop: 2 }}>
          {node.estimated_rows != null && (
            <span style={{ color: costColor(node.estimated_rows), fontWeight: 600 }}>
              估算 {node.estimated_rows.toLocaleString()} 行
            </span>
          )}
          {node.actual_rows != null && (
            <span style={{ marginLeft: 8 }}>
              实际 {node.actual_rows.toLocaleString()} 行
            </span>
          )}
          {node.estimated_cost != null && (
            <span style={{ marginLeft: 8 }}>
              cost={node.estimated_cost.toLocaleString()}
            </span>
          )}
          {node.actual_time != null && (
            <span style={{ marginLeft: 8 }}>
              {node.actual_time.toFixed(2)}ms
            </span>
          )}
          {node.uses_filesort && (
            <span style={{ marginLeft: 8, color: "var(--danger)", fontWeight: 600 }}>
              filesort
            </span>
          )}
          {node.uses_temporary && (
            <span style={{ marginLeft: 8, color: "var(--danger)", fontWeight: 600 }}>
              temporary
            </span>
          )}
        </div>

        {node.filter && (
          <div className="text-xs" style={{ marginTop: 2, fontFamily: "monospace", color: "var(--muted)" }}>
            {node.filter}
          </div>
        )}
        {node.sort_keys && node.sort_keys.length > 0 && (
          <div className="text-xs text-muted" style={{ marginTop: 2 }}>
            排序: {node.sort_keys.join(", ")}
          </div>
        )}
      </div>

      {node.children?.map((child, i) => (
        <PlanNodeView key={i} node={child} depth={depth + 1} />
      ))}
    </div>
  );
}
