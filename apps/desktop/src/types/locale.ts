export type DesktopProduct = "chatGpt" | "codex";

export interface DesktopApp {
  product: DesktopProduct;
  displayName: string;
  installPath: string;
  executablePath: string;
  bundleIdentifier: string | null;
  version: string | null;
  running: boolean;
}

export interface LocaleStatus {
  chineseEnabled: boolean;
  configLocale: string | null;
  globalStateLocale: string | null;
  configPath: string;
  globalStatePath: string;
  restoreAvailable: boolean;
}

export interface LocaleOverview {
  apps: DesktopApp[];
  locale: LocaleStatus;
}

export interface RepairOverview {
  app: DesktopApp | null;
  locale: LocaleStatus;
  activationAvailable: boolean;
}

export interface LocaleActivationResult {
  app: DesktopApp;
  locale: LocaleStatus;
  configChanged: boolean;
  globalStateChanged: boolean;
  restarted: boolean;
}

export interface NetworkRecoveryStatus {
  pending: boolean;
  localProxyActive: boolean;
}

export interface LocaleRestoreResult {
  app: DesktopApp | null;
  locale: LocaleStatus;
  restoredFiles: string[];
  configurationRestored: boolean;
  restarted: boolean;
  restartWarning: CommandError | null;
}

export interface CommandError {
  code: string;
  message: string;
}

export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    typeof value.code === "string" &&
    "message" in value &&
    typeof value.message === "string"
  );
}
