import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionInfo,
  QueryResultData,
  DiagnosisReport,
  AiExplanation,
  SanitizedContext,
  ReportSummary,
} from "./types";

export const api = {
  connect: (url: string, readOnly: boolean = true): Promise<ConnectionInfo> =>
    invoke("connect", { req: { url, read_only: readOnly } }),

  disconnect: (): Promise<void> => invoke("disconnect"),

  ping: (): Promise<string> => invoke("ping"),

  executeQuery: (sql: string, fetchSize: number = 100): Promise<QueryResultData> =>
    invoke("execute_query", { req: { sql, fetch_size: fetchSize } }),

  cancelQuery: (): Promise<void> => invoke("cancel_query"),

  diagnose: (sql: string): Promise<DiagnosisReport> =>
    invoke("diagnose", { req: { sql } }),

  aiExplain: (reportJson: string): Promise<AiExplanation> =>
    invoke("ai_explain", { req: { report_json: reportJson } }),

  sanitizeContext: (reportJson: string): Promise<SanitizedContext> =>
    invoke("sanitize_context", { reportJson }),

  saveReport: (reportJson: string): Promise<string> =>
    invoke("save_report", { reportJson }),

  listReports: (): Promise<ReportSummary[]> =>
    invoke("list_reports"),

  getReport: (id: string): Promise<DiagnosisReport> =>
    invoke("get_report", { id }),

  deleteReport: (id: string): Promise<void> =>
    invoke("delete_report", { id }),
};
