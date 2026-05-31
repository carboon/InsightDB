import { create } from "zustand";
import type {
  ConnectionInfo,
  QueryResultData,
  DiagnosisReport,
  AiExplanation,
  ReportSummary,
} from "../api/types";

type ViewMode = "query" | "diagnosis" | "history";

interface AppState {
  // 连接状态
  connection: ConnectionInfo | null;
  connecting: boolean;
  connectionError: string | null;

  // 查询状态
  sql: string;
  queryResult: QueryResultData | null;
  queryRunning: boolean;
  queryError: string | null;

  // 诊断状态
  diagnosis: DiagnosisReport | null;
  diagnosisRunning: boolean;
  diagnosisError: string | null;

  // AI 状态
  aiExplanation: AiExplanation | null;
  aiRunning: boolean;
  aiError: string | null;

  // 历史报告状态
  historyList: ReportSummary[];
  historyLoading: boolean;
  historyError: string | null;

  // UI 状态
  viewMode: ViewMode;
  selectedHistoryId: string | null;

  // Actions
  setConnection: (info: ConnectionInfo | null) => void;
  setConnecting: (v: boolean) => void;
  setConnectionError: (e: string | null) => void;

  setSql: (sql: string) => void;
  setQueryResult: (r: QueryResultData | null) => void;
  setQueryRunning: (v: boolean) => void;
  setQueryError: (e: string | null) => void;

  setDiagnosis: (d: DiagnosisReport | null) => void;
  setDiagnosisRunning: (v: boolean) => void;
  setDiagnosisError: (e: string | null) => void;

  setAiExplanation: (a: AiExplanation | null) => void;
  setAiRunning: (v: boolean) => void;
  setAiError: (e: string | null) => void;

  setHistoryList: (list: ReportSummary[]) => void;
  setHistoryLoading: (v: boolean) => void;
  setHistoryError: (e: string | null) => void;

  setViewMode: (m: ViewMode) => void;
  setSelectedHistoryId: (id: string | null) => void;

  reset: () => void;
}

const initialState = {
  connection: null,
  connecting: false,
  connectionError: null,
  sql: "SELECT 1",
  queryResult: null,
  queryRunning: false,
  queryError: null,
  diagnosis: null,
  diagnosisRunning: false,
  diagnosisError: null,
  aiExplanation: null,
  aiRunning: false,
  aiError: null,
  historyList: [],
  historyLoading: false,
  historyError: null,
  viewMode: "query" as ViewMode,
  selectedHistoryId: null,
};

export const useAppStore = create<AppState>((set) => ({
  ...initialState,

  setConnection: (connection) => set({ connection, connecting: false }),
  setConnecting: (connecting) => set({ connecting }),
  setConnectionError: (connectionError) => set({ connectionError, connecting: false }),

  setSql: (sql) => set({ sql }),
  setQueryResult: (queryResult) => set({ queryResult, queryRunning: false }),
  setQueryRunning: (queryRunning) => set({ queryRunning }),
  setQueryError: (queryError) => set({ queryError, queryRunning: false }),

  setDiagnosis: (diagnosis) => set({ diagnosis, diagnosisRunning: false }),
  setDiagnosisRunning: (diagnosisRunning) => set({ diagnosisRunning }),
  setDiagnosisError: (diagnosisError) => set({ diagnosisError, diagnosisRunning: false }),

  setAiExplanation: (aiExplanation) => set({ aiExplanation, aiRunning: false }),
  setAiRunning: (aiRunning) => set({ aiRunning }),
  setAiError: (aiError) => set({ aiError, aiRunning: false }),

  setHistoryList: (historyList) => set({ historyList, historyLoading: false }),
  setHistoryLoading: (historyLoading) => set({ historyLoading }),
  setHistoryError: (historyError) => set({ historyError, historyLoading: false }),

  setViewMode: (viewMode) => set({ viewMode }),
  setSelectedHistoryId: (selectedHistoryId) => set({ selectedHistoryId }),

  reset: () => set(initialState),
}));
