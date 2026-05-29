export interface ConnectionInfo {
  db_type: string;
  version: string;
  database: string;
  connected: boolean;
}

export interface QueryResultData {
  columns: string[];
  rows: (string | null)[][];
  affected_rows: number | null;
}

export interface EvidenceItem {
  statement: string;
  source: string;
  is_fact: boolean;
}

export interface Recommendation {
  action: string;
  benefit: string;
  risk: string;
  verification_sql: string | null;
  is_inference: boolean;
}

export interface AiExplanation {
  problem_summary: string;
  evidence: EvidenceItem[];
  recommendations: Recommendation[];
  confidence: number;
  model: string;
}

export interface RuleFinding {
  id: string;
  severity: string;
  title: string;
  evidence: string;
  recommendation: string;
  risk: string;
  verification: string;
  confidence: number;
}

export interface DiagnosisReport {
  sql: string;
  db_type: string;
  db_version: string;
  database_name: string;
  findings: RuleFinding[];
  plan: Record<string, unknown>;
  schema: Record<string, unknown>;
  generated_at: string;
  summary: string;
  overall_severity: string;
  total_findings: number;
}

export interface SanitizedContext {
  sanitized_sql: string;
  catalog_summary: string;
  explain_summary: string;
  rule_findings: string[];
  db_type: string;
  db_version: string;
  identifier_mapping: [string, string][];
}
