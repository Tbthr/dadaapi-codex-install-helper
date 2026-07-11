export type DiagnosticCheckState = "healthy" | "degraded" | "failed" | "unavailable" | "notChecked";

export interface DiagnosticChecks {
  desktopApp: DiagnosticCheckState;
  localeConfiguration: DiagnosticCheckState;
  routeBundle: DiagnosticCheckState;
  localProxy: DiagnosticCheckState;
  networkRecovery: DiagnosticCheckState;
  officialDownloads: DiagnosticCheckState;
}

export interface DiagnosticReport {
  schemaVersion: number;
  generatedAt: string;
  serviceName: string;
  applicationVersion: string;
  operatingSystem: "macos" | "windows" | "other";
  architecture: "aarch64" | "x86_64" | "other";
  buildProfile: "debug" | "release";
  checks: DiagnosticChecks;
  retainedLogFiles: number;
  retainedLogBytes: number;
}

export interface DiagnosticExportResult {
  fileName: string;
  bytes: number;
  entryCount: number;
}
