import { ConnectionPanel } from "./components/ConnectionPanel";
import { SqlWorkspace } from "./components/SqlWorkspace";
import { ResultsViewer } from "./components/ResultsViewer";
import { DiagnosisReport } from "./components/DiagnosisReport";
import { useAppStore } from "./stores/appStore";

function App() {
  const viewMode = useAppStore((s) => s.viewMode);

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <h1>InsightDB</h1>
          <span className="text-xs text-muted">慢 SQL 诊断工作台</span>
        </div>
        <ConnectionPanel />
      </aside>

      <main className="main">
        <SqlWorkspace />

        {viewMode === "query" && <ResultsViewer />}
        {viewMode === "diagnosis" && <DiagnosisReport />}
      </main>
    </div>
  );
}

export default App;
