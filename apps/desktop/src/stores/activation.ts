import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { defineStore } from "pinia";
import { ref } from "vue";
import {
  ACTIVATION_PROGRESS_EVENT,
  activateChinese,
  getNetworkRecoveryStatus,
  isActivationAvailable,
  restoreNetwork,
} from "../services/activation";
import type { ActivationEvent, ActivationPhase } from "../types/activation";
import type {
  CommandError,
  LocaleActivationResult,
  NetworkRecoveryStatus,
} from "../types/locale";

export const useActivationStore = defineStore("activation", () => {
  const available = ref(false);
  const running = ref(false);
  const selectedExecutablePath = ref("");
  const phase = ref<ActivationPhase>("idle");
  const message = ref("");
  const error = ref("");
  const result = ref<LocaleActivationResult | null>(null);
  const networkStatus = ref<NetworkRecoveryStatus>({
    pending: false,
    localProxyActive: false,
  });
  const recoveryRunning = ref(false);
  const recoveryError = ref("");
  let stopListening: UnlistenFn | null = null;

  async function initialize(): Promise<void> {
    try {
      if (!stopListening) {
        stopListening = await listen<ActivationEvent>(ACTIVATION_PROGRESS_EVENT, (event) => {
          if (!running.value) {
            return;
          }
          phase.value = event.payload.phase;
          message.value = event.payload.message;
        });
      }
    } catch {
      stopListening = null;
    }
    try {
      available.value = await isActivationAvailable();
    } catch {
      available.value = false;
    }
    await refreshNetworkStatus();
  }

  async function activate(executablePath: string): Promise<boolean> {
    if (!available.value || running.value || networkStatus.value.pending) {
      return false;
    }
    running.value = true;
    selectedExecutablePath.value = executablePath;
    phase.value = "detectingApp";
    message.value = "准备开始中文激活";
    error.value = "";
    result.value = null;
    try {
      result.value = await activateChinese(executablePath);
      phase.value = "succeeded";
      message.value = "中文已生效，代理仍在使用，请按需手动恢复网络";
      return true;
    } catch (reason) {
      phase.value = "failed";
      error.value = errorMessage(reason);
      return false;
    } finally {
      running.value = false;
      await refreshNetworkStatus();
    }
  }

  async function refreshNetworkStatus(): Promise<void> {
    try {
      networkStatus.value = await getNetworkRecoveryStatus();
      recoveryError.value = "";
    } catch (reason) {
      recoveryError.value = errorMessage(reason);
    }
  }

  async function restoreOriginalNetwork(): Promise<boolean> {
    if (recoveryRunning.value || !networkStatus.value.pending) {
      return false;
    }
    recoveryRunning.value = true;
    recoveryError.value = "";
    try {
      networkStatus.value = await restoreNetwork();
      return !networkStatus.value.pending;
    } catch (reason) {
      recoveryError.value = errorMessage(reason);
      try {
        networkStatus.value = await getNetworkRecoveryStatus();
      } catch {
        // Keep the last known status and the original recovery error visible.
      }
      return false;
    } finally {
      recoveryRunning.value = false;
    }
  }

  function errorMessage(reason: unknown): string {
    if (typeof reason === "object" && reason !== null && "message" in reason) {
      return String((reason as CommandError).message);
    }
    return typeof reason === "string" ? reason : "中文激活失败，请稍后重试。";
  }

  return {
    available,
    running,
    selectedExecutablePath,
    phase,
    message,
    error,
    result,
    networkStatus,
    recoveryRunning,
    recoveryError,
    initialize,
    activate,
    refreshNetworkStatus,
    restoreOriginalNetwork,
  };
});
