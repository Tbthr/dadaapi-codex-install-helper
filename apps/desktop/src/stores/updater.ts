import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { reactive } from "vue";
import {
  checkForAppUpdate,
  downloadAndInstallAppUpdate,
  type AppUpdateInfo,
} from "../services/updater";

type UpdaterPhase = "idle" | "checking" | "available" | "downloading" | "ready" | "current" | "error";

const state = reactive({
  initialized: false,
  phase: "idle" as UpdaterPhase,
  currentVersion: "0.1.0",
  update: null as AppUpdateInfo | null,
  downloadedBytes: 0,
  totalBytes: null as number | null,
  message: "",
});

async function checkNow(): Promise<void> {
  if (state.phase === "checking" || state.phase === "downloading") {
    return;
  }
  state.phase = "checking";
  state.message = "";
  try {
    state.update = await checkForAppUpdate();
    state.phase = state.update ? "available" : "current";
  } catch {
    state.phase = "error";
    state.message = "检查更新失败，请确认能够访问 GitHub";
  }
}

async function initialize(): Promise<void> {
  if (state.initialized) {
    return;
  }
  state.initialized = true;
  state.currentVersion = await getVersion().catch(() => "0.1.0");
  await checkNow();
}

async function install(): Promise<void> {
  if (state.phase !== "available") {
    return;
  }
  state.phase = "downloading";
  state.downloadedBytes = 0;
  state.totalBytes = null;
  state.message = "";
  try {
    await downloadAndInstallAppUpdate((event) => {
      if (event.event === "Started") {
        state.totalBytes = event.data.contentLength ?? null;
      } else if (event.event === "Progress") {
        state.downloadedBytes += event.data.chunkLength;
      }
    });
    state.phase = "ready";
  } catch {
    state.phase = "error";
    state.message = "更新下载或安装失败，请稍后重试";
  }
}

async function restart(): Promise<void> {
  if (state.phase === "ready") {
    await relaunch();
  }
}

export function useUpdaterStore() {
  return { state, initialize, checkNow, install, restart };
}
