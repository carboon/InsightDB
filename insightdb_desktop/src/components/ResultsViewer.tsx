import { useAppStore } from "../stores/appStore";

export function ResultsViewer() {
  const { queryResult, queryRunning, queryError } = useAppStore();

  if (queryRunning) {
    return <div className="panel results-panel loading">执行中...</div>;
  }

  if (queryError) {
    return <div className="panel results-panel error-text">{queryError}</div>;
  }

  if (!queryResult) {
    return <div className="panel results-panel text-muted">输入 SQL 并点击"执行"查看结果</div>;
  }

  const { columns, rows } = queryResult;

  return (
    <div className="panel results-panel">
      <div className="text-sm text-muted">
        {rows.length} 行 × {columns.length} 列
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
            {rows.map((row, i) => (
              <tr key={i}>
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
