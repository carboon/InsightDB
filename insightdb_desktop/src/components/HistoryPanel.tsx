import React, { useEffect } from "react";
import { api } from "../api/adapter";
import { useAppStore } from "../stores/appStore";

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

export function HistoryPanel() {
  const {
    historyList,
    historyLoading,
    historyError,
    selectedHistoryId,
    setHistoryList,
    setHistoryLoading,
    setHistoryError,
    setViewMode,
    setSelectedHistoryId,
    setDiagnosis,
  } = useAppStore();

  const loadHistory = async () => {
    setHistoryLoading(true);
    setHistoryError(null);
    try {
      const list = await api.listReports();
      setHistoryList(list);
    } catch (e) {
      setHistoryError(String(e));
    }
  };

  const handleView = async (id: string) => {
    try {
      const report = await api.getReport(id);
      setSelectedHistoryId(id);
      setDiagnosis(report);
      setViewMode("history");
    } catch (e) {
      setHistoryError(String(e));
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await api.deleteReport(id);
      await loadHistory();
      if (selectedHistoryId === id) {
        setSelectedHistoryId(null);
        setDiagnosis(null);
        setViewMode("query");
      }
    } catch (e) {
      setHistoryError(String(e));
    }
  };

  useEffect(() => {
    loadHistory();
  }, []);

  return (
    <div className="panel history-panel">
      <div className="flex-row" style={{ justifyContent: "space-between", marginBottom: 8 }}>
        <h3 style={{ fontSize: 14 }}>历史报告</h3>
        <button className="btn btn-sm btn-secondary" onClick={loadHistory} disabled={historyLoading}>
          刷新
        </button>
      </div>

      {historyError && <div className="error-text">{historyError}</div>}

      {historyLoading && <div className="loading">加载中...</div>}

      {!historyLoading && historyList.length === 0 && (
        <div className="text-sm text-muted" style={{ padding: 8 }}>
          暂无历史报告，执行诊断后可保存。
        </div>
      )}

      {historyList.map((item) => (
        <div
          key={item.id}
          className={`history-item ${selectedHistoryId === item.id ? "history-item-active" : ""}`}
          onClick={() => handleView(item.id)}
        >
          <div className="flex-row" style={{ justifyContent: "space-between" }}>
            <span className="badge" style={{ backgroundColor: severityColor(item.overall_severity) }}>
              {item.overall_severity}
            </span>
            <button
              className="btn btn-sm btn-danger"
              onClick={(e) => {
                e.stopPropagation();
                handleDelete(item.id);
              }}
            >
              删除
            </button>
          </div>
          <div className="history-item-sql text-sm text-muted" title={item.sql}>
            {item.sql.length > 60 ? item.sql.slice(0, 60) + "..." : item.sql}
          </div>
          <div className="flex-row gap-sm text-xs text-muted">
            <span>{item.database_name}</span>
            <span>{item.total_findings} 个问题</span>
            <span>{new Date(item.generated_at).toLocaleString()}</span>
          </div>
        </div>
      ))}

      {historyList.length > 0 && (
        <div className="text-xs text-muted" style={{ padding: 4 }}>
          点击报告查看详情
        </div>
      )}
    </div>
  );
}
