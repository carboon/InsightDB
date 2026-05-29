import { useState } from "react";
import { api } from "../api/adapter";
import { useAppStore } from "../stores/appStore";

export function ConnectionPanel() {
  const [url, setUrl] = useState("mysql://root:root@localhost:3306/mysql");
  const [readOnly, setReadOnly] = useState(true);

  const { connection, connecting, connectionError, setConnection, setConnecting, setConnectionError } =
    useAppStore();

  const handleConnect = async () => {
    setConnecting(true);
    setConnectionError(null);
    try {
      const info = await api.connect(url, readOnly);
      setConnection(info);
    } catch (e) {
      setConnectionError(String(e));
    }
  };

  const handleDisconnect = async () => {
    try {
      await api.disconnect();
      setConnection(null);
    } catch (e) {
      setConnectionError(String(e));
    }
  };

  if (connection?.connected) {
    return (
      <div className="panel connection-panel connected">
        <div className="flex-row">
          <span className="dot green" />
          <span>{connection.database}</span>
          <span className="tag">{connection.db_type}</span>
          <span className="text-muted text-xs">{connection.version}</span>
        </div>
        <button onClick={handleDisconnect} className="btn btn-secondary btn-sm">断开</button>
      </div>
    );
  }

  return (
    <div className="panel connection-panel">
      <div className="flex-col gap-sm">
        <label className="text-sm text-muted">数据库连接 URL</label>
        <input
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          placeholder="mysql://user:pass@host:3306/db"
          className="input"
        />
        <label className="flex-row gap-sm">
          <input
            type="checkbox"
            checked={readOnly}
            onChange={(e) => setReadOnly(e.target.checked)}
          />
          <span className="text-sm">只读模式</span>
        </label>
        <button onClick={handleConnect} disabled={connecting} className="btn btn-primary">
          {connecting ? "连接中..." : "连接"}
        </button>
        {connectionError && (
          <div className="error-text">{connectionError}</div>
        )}
      </div>
    </div>
  );
}
