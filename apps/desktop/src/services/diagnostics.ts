import { invoke } from "@tauri-apps/api/core";
import type { DiagnosticExportResult, DiagnosticReport } from "../types/diagnostics";

export function getDiagnosticSummary(): Promise<DiagnosticReport> {
  return invoke<DiagnosticReport>("get_diagnostic_summary");
}

export function exportDiagnostics(): Promise<DiagnosticExportResult> {
  return invoke<DiagnosticExportResult>("export_diagnostics");
}

export function revealDiagnosticsExport(fileName: string): Promise<void> {
  return invoke<void>("reveal_diagnostics_export", { fileName });
}
