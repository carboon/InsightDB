import { useState, useEffect } from "react";
import { api } from "../api/adapter";
import { useAppStore } from "../stores/appStore";

export function SqlWorkspace() {
  const { sql, setSql, connection, queryRunning, diagnosisRunning, setViewMode, viewMode } =
    useAppStore();

  const [fetchSize, setFetchSize] = useState(100);
  const [elapsed, setElapsed] = useState(0);

  const isRunning = queryRunning || diagnosisRunning;

  useEffect(() => {
    if (isRunning) {
      setElapsed(0);
      const interval = setInterval(() => setElapsed((t) => t + 1), 1000);
      return () => clearInterval(interval);
    }
  }, [isRunning]);

  const setQueryResult = useAppStore((s) => s.setQueryResult);
  const setQueryRunning = useAppStore((s) => s.setQueryRunning);
  const setQueryError = useAppStore((s) => s.setQueryError);

  const setDiagnosis = useAppStore((s) => s.setDiagnosis);
  const setDiagnosisRunning = useAppStore((s) => s.setDiagnosisRunning);
  const setDiagnosisError = useAppStore((s) => s.setDiagnosisError);

  const handleExecute = async () => {
    if (!connection) return;
    setQueryRunning(true);
    setQueryError(null);
    setViewMode("query");
    try {
      const result = await api.executeQuery(sql, fetchSize);
      setQueryResult(result);
    } catch (e) {
      setQueryError(String(e));
    }
  };

  const handleDiagnose = async () => {
    if (!connection) return;
    setDiagnosisRunning(true);
    setDiagnosisError(null);
    setViewMode("diagnosis");
    try {
      const report = await api.diagnose(sql);
      setDiagnosis(report);
    } catch (e) {
      setDiagnosisError(String(e));
    }
  };

  const handleCancel = async () => {
    try {
      await api.cancelQuery();
    } catch (e) {
      console.error("取消失败:", e);
    }
  };

  return (
    <div className="panel sql-workspace">
      <textarea
        value={sql}
        onChange={(e) => setSql(e.target.value)}
        rows={6}
        className="textarea"
        placeholder="输入 SQL 语句..."
        disabled={!connection}
      />
      <div className="flex-row gap-sm toolbar">
        <button onClick={handleExecute} disabled={!connection || isRunning} className="btn btn-primary">
          执行
        </button>
        <button onClick={handleDiagnose} disabled={!connection || isRunning} className="btn btn-accent">
          诊断
        </button>
        <button onClick={handleCancel} disabled={!isRunning} className="btn btn-danger">
          取消
        </button>
        <span className="text-sm text-muted">
          每页
          <input
            type="number"
            value={fetchSize}
            onChange={(e) => setFetchSize(Number(e.target.value))}
            className="input-inline"
            min={1}
            max={10000}
          />
          行
        </span>
        {isRunning && (
          <span className="text-sm" style={{ color: "var(--accent)" }}>
            {queryRunning ? "查询中" : "诊断中"}... {elapsed}s
          </span>
        )}
        {!isRunning && (
          <span className="text-sm">
            {viewMode === "query" ? "查询模式" : "诊断模式"}
          </span>
        )}
      </div>
    </div>
  );
}
