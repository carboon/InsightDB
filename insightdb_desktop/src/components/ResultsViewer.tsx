import { useState, useEffect } from "react";
import { useAppStore } from "../stores/appStore";

const isDangerous = (msg: string | null) => msg?.startsWith("[安全拦截]");
const PAGE_SIZE = 100;

export function ResultsViewer() {
  const { queryResult, queryRunning, queryError } = useAppStore();
  const [page, setPage] = useState(0);
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    if (queryRunning) {
      setElapsed(0);
      const interval = setInterval(() => setElapsed((t) => t + 1), 1000);
      return () => clearInterval(interval);
    }
  }, [queryRunning]);

  useEffect(() => {
    setPage(0);
  }, [queryResult]);

  if (queryRunning) {
    return (
      <div className="panel results-panel loading">
        执行中... {elapsed > 0 && `(${elapsed}s)`}
      </div>
    );
  }

  if (queryError) {
    const dangerous = isDangerous(queryError);
    return (
      <div className="panel results-panel">
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
          {dangerous ? queryError.replace("[安全拦截] ", "") : queryError}
        </div>
      </div>
    );
  }

  if (!queryResult) {
    return <div className="panel results-panel text-muted">输入 SQL 并点击"执行"查看结果</div>;
  }

  const { columns, rows } = queryResult;
  const totalPages = Math.ceil(rows.length / PAGE_SIZE);
  const start = page * PAGE_SIZE;
  const pageRows = rows.slice(start, start + PAGE_SIZE);

  return (
    <div className="panel results-panel">
      <div className="flex-row" style={{ justifyContent: "space-between", marginBottom: 4 }}>
        <span className="text-sm text-muted">
          {rows.length} 行 × {columns.length} 列
        </span>
        {totalPages > 1 && (
          <div className="flex-row gap-sm">
            <button
              className="btn btn-sm btn-secondary"
              disabled={page === 0}
              onClick={() => setPage((p) => p - 1)}
            >
              上一页
            </button>
            <span className="text-sm text-muted">
              {page + 1} / {totalPages}
            </span>
            <button
              className="btn btn-sm btn-secondary"
              disabled={page >= totalPages - 1}
              onClick={() => setPage((p) => p + 1)}
            >
              下一页
            </button>
          </div>
        )}
      </div>

      <div className="table-container">
        <table>
          <thead>
            <tr>
              {columns.map((col) => (
                <th key={col}>{col}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {pageRows.map((row, i) => (
              <tr key={start + i}>
                {row.map((val, j) => (
                  <td key={j}>
                    <span className={val === null ? "null-value" : ""}>
                      {val === null ? "NULL" : String(val)}
                    </span>
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
