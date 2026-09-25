import { invoke } from "@tauri-apps/api/core";

/** Matches the Rust AppError shape (spec §23). */
export interface AppError {
  status: "error";
  code: string;
  message: string;
  details?: string | null;
  recovery?: string | null;
}

export function isAppError(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

export function describe(e: unknown): AppError {
  if (isAppError(e)) return e;
  return {
    status: "error",
    code: "UNEXPECTED",
    message: "Something went wrong that DevWorkstation didn't anticipate.",
    details: String(e),
    recovery: "Check Logs for the full detail.",
  };
}

export const call = <T,>(cmd: string, args?: Record<string, unknown>) =>
  invoke<T>(cmd, args);

// ---------------------------------------------------------------- types

export interface SessionUser {
  id: number;
  username: string;
  display_name: string;
  password_changed: boolean;
}

export interface LicenseInfo {
  activation_key: string;
  status: string;
  last_verified_at: string | null;
}

/** Where a user with no valid activation key is sent to buy or fix one. */
export const ACTIVATION_PURCHASE_URL =
  "https://devworkstation-website.vercel.app/activationkey";

export interface LogLine {
  id: number; at: string; level: string; module: string;
  message: string; detail: string | null;
}

export interface Resources {
  cpu_percent: number; memory_used_mb: number; memory_total_mb: number;
  host: string; os: string;
}

export interface DashboardData {
  resources: Resources;
  activity: { at: string; kind: string; label: string; target: string }[];
  recent_errors: LogLine[];
  project_count: number; server_count: number; cv_count: number;
  online: boolean; data_folder: string;
}

export interface Project {
  id?: number | null; name: string; path: string; technology: string;
  repository: string; server_id?: number | null; database_id?: number | null;
  notes: string; status: string; last_scan?: string | null;
}

export interface Server {
  id?: number | null; name: string; host: string; port: number; username: string;
  auth_kind: string; key_path: string; vault_ref: string; kind: string;
}

export interface DatabaseProfile {
  id?: number | null; name: string; engine: string; host: string; port: number;
  dbname: string; username: string; file_path: string; vault_ref: string;
}

export interface FsEntry {
  name: string; path: string; is_dir: boolean; size: number;
  modified: string; kind: string;
}

export interface VaultItem {
  reference: string; label: string; kind: string; username: string;
}

export interface WebsiteReport {
  url: string; final_url: string; addresses: string[]; status: number;
  http_version: string; scheme: string;
  timings: { dns_ms: number; connect_ms: number; first_response_ms: number; total_ms: number };
  headers: [string, string][];
  security_headers: [string, string | null][];
  cookies: string[]; cors: string | null;
  redirects: { from: string; status: number; to: string }[];
  page: {
    html_bytes: number; stylesheets: number; scripts: number; images: number;
    links_internal: number; links_external: number; inline_scripts: number;
    title: string; external_hosts: string[];
  };
  findings: { severity: string; area: string; title: string; detail: string }[];
  generated_at: string;
}

export interface Analysis {
  root: string; technologies: string[]; entry_points: string[]; routes: string[];
  api_calls: string[]; database_hints: string[]; env_vars: string[];
  dependencies: Record<string, string>;
  language_totals: Record<string, number>;
  files: { path: string; language: string; lines: number; bytes: number }[];
  edges: { from: string; to: string; kind: string }[];
  insights: { severity: string; category: string; title: string; detail: string; path: string | null; line: number | null }[];
  file_count: number; skipped_count: number; scanned_at: string;
}

export interface PdfInfo {
  path: string;
  pages: { number: number; classification: string; characters: number; images: number; width: number; height: number; rotation: number }[];
  page_count: number; encrypted: boolean; title: string; author: string;
  producer: string; version: string; bytes: number; needs_ocr: boolean; note: string;
}

export interface Diagnosis {
  severity: string; code: string; title: string; detail: string;
  table: string | null; suggested_sql: string | null;
}

export interface DbAnalysis {
  path: string; engine: string; integrity: string;
  tables: {
    name: string; rows: number;
    columns: { name: string; data_type: string; not_null: boolean; default_value: string | null; primary_key: boolean }[];
    indexes: { name: string; unique: boolean; columns: string[] }[];
    foreign_keys: { column: string; references_table: string; references_column: string; on_delete: string }[];
  }[];
  views: string[]; triggers: string[]; diagnoses: Diagnosis[];
  plan_token: string; plan_sql: string[]; analysed_at: string;
}

export interface Check {
  id: string; label: string; program: string; args: string[]; description: string;
}

export interface Detection {
  project_type: string; package_manager: string; test_framework: string;
  build_system: string; available_checks: Check[]; warnings: string[];
}

export interface CheckResult {
  id: string; label: string; outcome: string; exit_code: number | null;
  duration_ms: number; stdout: string; stderr: string;
  problems: { file: string; line: number | null; column: number | null; message: string; cause: string; suggestion: string }[];
}

export interface ShellResult {
  command: string; exit_code: number | null; stdout: string;
  stderr: string; duration_ms: number; cwd: string;
}

export interface CvDocument {
  full_name: string; headline: string; email: string; phone: string;
  location: string; website: string; summary: string;
  sections: CvSection[];
  style: CvStyle;
}

export interface CvSection {
  id: string; heading: string; layout: "items" | "list" | "text";
  items: CvItem[]; entries: string[]; text: string;
}

export interface CvItem {
  title: string; subtitle: string; period: string; location: string; bullets: string[];
}

export interface CvStyle {
  font: string; body_size: number; heading_size: number; name_size: number;
  line_spacing: number; margin_mm: number; page: string; accent: string;
  header_align: string;
}

export const blankCv = (): CvDocument => ({
  full_name: "",
  headline: "",
  email: "",
  phone: "",
  location: "",
  website: "",
  summary: "",
  sections: [
    { id: "exp", heading: "Experience", layout: "items", items: [], entries: [], text: "" },
    { id: "edu", heading: "Education", layout: "items", items: [], entries: [], text: "" },
    { id: "skills", heading: "Skills", layout: "list", items: [], entries: [], text: "" },
  ],
  style: {
    font: "Helvetica", body_size: 10, heading_size: 12, name_size: 22,
    line_spacing: 1.35, margin_mm: 18, page: "A4",
    accent: "#1F3A5F", header_align: "left",
  },
});
