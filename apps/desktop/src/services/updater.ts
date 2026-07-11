import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";

export interface AppUpdateInfo {
  currentVersion: string;
  version: string;
  notes: string | null;
}

let pendingUpdate: Update | null = null;

export async function checkForAppUpdate(): Promise<AppUpdateInfo | null> {
  await pendingUpdate?.close();
  pendingUpdate = await check({ timeout: 20_000 });
  if (!pendingUpdate) {
    return null;
  }
  return {
    currentVersion: pendingUpdate.currentVersion,
    version: pendingUpdate.version,
    notes: pendingUpdate.body ?? null,
  };
}

export async function downloadAndInstallAppUpdate(
  onEvent: (event: DownloadEvent) => void,
): Promise<void> {
  if (!pendingUpdate) {
    throw new Error("没有可安装的更新");
  }
  await pendingUpdate.downloadAndInstall(onEvent, { timeout: 15 * 60 * 1000 });
}
