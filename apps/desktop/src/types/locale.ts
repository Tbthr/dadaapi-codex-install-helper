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

export interface LocaleActivationResult {
  app: DesktopApp;
  locale: LocaleStatus;
  configChanged: boolean;
  globalStateChanged: boolean;
  restarted: boolean;
}

export interface LocaleRestoreResult {
  app: DesktopApp | null;
  locale: LocaleStatus;
  restoredFiles: string[];
  restarted: boolean;
}

export interface CommandError {
  code: string;
  message: string;
}
