import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { defineStore } from "pinia";
import { computed, ref } from "vue";
import {
  ACTIVATION_PROGRESS_EVENT,
  activateChinese,
  getNetworkRecoveryStatus,
  isActivationAvailable,
  restoreNetwork,
} from "../services/activation";
import type { ActivationEvent, ActivationPhase } from "../types/activation";
import type { CommandError, LocaleActivationResult, NetworkRecoveryStatus } from "../types/locale";

export type ActivationAvailabilityState =
  "unknown" | "loading" | "available" | "unavailable" | "error";

export type NetworkStatusState = "unknown" | "loading" | "ready" | "error";
export type ActivationProgressStep = 0 | 1 | 2 | 3 | 4;

const EMPTY_NETWORK_STATUS: NetworkRecoveryStatus = {
  pending: false,
  localProxyActive: false,
};

export const useActivationStore = defineStore("activation", () => {
  const availabilityState = ref<ActivationAvailabilityState>("unknown");
  const availabilityError = ref("");
  const available = computed(() => availabilityState.value === "available");
  const running = ref(false);
  const selectedExecutablePath = ref("");
  const phase = ref<ActivationPhase>("idle");
  const progressStep = ref<ActivationProgressStep>(0);
  const message = ref("");
  const error = ref("");
  const result = ref<LocaleActivationResult | null>(null);
  const networkStatusState = ref<NetworkStatusState>("unknown");
  const networkStatus = ref<NetworkRecoveryStatus>({ ...EMPTY_NETWORK_STATUS });
  const networkStatusError = ref("");
  const networkPending = computed(() =>
    networkStatusState.value === "ready" ? networkStatus.value.pending : null,
  );
  const localProxyActive = computed(() =>
    networkStatusState.value === "ready" ? networkStatus.value.localProxyActive : null,
  );
  const recoveryRunning = ref(false);
  const recoveryError = ref("");
  const canActivate = computed(
    () =>
      availabilityState.value === "available" &&
      networkStatusState.value === "ready" &&
      !networkStatus.value.pending &&
      !running.value &&
      !recoveryRunning.value,
  );
  const resultMessage = computed(() => {
    if (!result.value) {
      return "";
    }
    switch (networkStatusState.value) {
      case "unknown":
      case "loading":
        return "中文已生效，正在确认网络状态";
      case "error":
        return "中文已生效，网络状态待确认";
      case "ready":
        return networkStatus.value.pending ? "中文已生效，代理使用中" : "中文已生效，网络已恢复";
      default:
        return assertNever(networkStatusState.value);
    }
  });

  let stopListening: UnlistenFn | null = null;
  let initializePromise: Promise<void> | null = null;
  let initialized = false;
  let lifecycleRevision = 0;
  let availabilityRequestRevision = 0;
  let networkRequestRevision = 0;

  async function initialize(): Promise<void> {
    if (initialized) {
      return;
    }
    if (initializePromise) {
      return initializePromise;
    }

    const revision = lifecycleRevision;
    const task = (async () => {
      await installProgressListener(revision);
      if (revision !== lifecycleRevision) {
        return;
      }
      await Promise.all([refreshAvailability(), refreshNetworkStatus()]);
      if (revision === lifecycleRevision) {
        initialized = true;
      }
    })();
    initializePromise = task;

    try {
      await task;
    } finally {
      if (initializePromise === task) {
        initializePromise = null;
      }
    }
  }

  async function installProgressListener(revision: number): Promise<void> {
    if (stopListening) {
      return;
    }
    try {
      const unlisten = await listen<ActivationEvent>(ACTIVATION_PROGRESS_EVENT, (event) => {
        applyProgressEvent(event.payload);
      });
      if (revision !== lifecycleRevision) {
        unlisten();
        return;
      }
      stopListening = unlisten;
    } catch {
      stopListening = null;
    }
  }

  function dispose(): void {
    lifecycleRevision += 1;
    initialized = false;
    initializePromise = null;
    const unlisten = stopListening;
    stopListening = null;
    unlisten?.();
  }

  async function refreshAvailability(): Promise<void> {
    const requestRevision = ++availabilityRequestRevision;
    availabilityState.value = "loading";
    availabilityError.value = "";
    try {
      const nextAvailable = await isActivationAvailable();
      if (requestRevision !== availabilityRequestRevision) {
        return;
      }
      availabilityState.value = nextAvailable ? "available" : "unavailable";
    } catch (reason) {
      if (requestRevision !== availabilityRequestRevision) {
        return;
      }
      availabilityState.value = "error";
      availabilityError.value = errorMessage(reason, "无法确认当前平台是否支持中文设置。");
    }
  }

  async function activate(executablePath: string): Promise<boolean> {
    if (!canActivate.value || !executablePath.trim()) {
      return false;
    }
    running.value = true;
    selectedExecutablePath.value = executablePath;
    phase.value = "detectingApp";
    progressStep.value = activationProgressStep("detectingApp");
    message.value = "准备开始中文激活";
    error.value = "";
    result.value = null;
    try {
      result.value = await activateChinese(executablePath);
      phase.value = "succeeded";
      progressStep.value = 4;
      message.value = "中文已生效，正在确认网络状态";
      return true;
    } catch (reason) {
      phase.value = "failed";
      error.value = errorMessage(reason, "中文激活失败，请稍后重试。");
      return false;
    } finally {
      await refreshNetworkStatus();
      if (result.value) {
        message.value = resultMessage.value;
      }
      running.value = false;
    }
  }

  async function refreshNetworkStatus(): Promise<void> {
    const requestRevision = ++networkRequestRevision;
    networkStatusState.value = "loading";
    networkStatusError.value = "";
    try {
      const nextStatus = await getNetworkRecoveryStatus();
      if (requestRevision !== networkRequestRevision) {
        return;
      }
      networkStatus.value = nextStatus;
      networkStatusState.value = "ready";
      if (!nextStatus.pending) {
        recoveryError.value = "";
      }
    } catch (reason) {
      if (requestRevision !== networkRequestRevision) {
        return;
      }
      networkStatusState.value = "error";
      networkStatusError.value = errorMessage(reason, "无法读取当前网络恢复状态。");
    }
  }

  async function restoreOriginalNetwork(): Promise<boolean> {
    if (
      recoveryRunning.value ||
      running.value ||
      networkStatusState.value !== "ready" ||
      !networkStatus.value.pending
    ) {
      return false;
    }
    recoveryRunning.value = true;
    recoveryError.value = "";
    networkStatusError.value = "";
    networkStatusState.value = "loading";
    networkRequestRevision += 1;
    try {
      networkStatus.value = await restoreNetwork();
      networkStatusState.value = "ready";
      await refreshNetworkStatus();
      const restored = networkStatusState.value === "ready" && !networkStatus.value.pending;
      if (!restored && networkStatusState.value === "ready") {
        recoveryError.value = "网络仍待恢复，请重试。";
      }
      if (result.value) {
        message.value = resultMessage.value;
      }
      return restored;
    } catch (reason) {
      recoveryError.value = errorMessage(reason, "恢复原网络失败，请稍后重试。");
      await refreshNetworkStatus();
      if (result.value) {
        message.value = resultMessage.value;
      }
      return false;
    } finally {
      recoveryRunning.value = false;
    }
  }

  function applyProgressEvent(event: ActivationEvent): void {
    if (!running.value || isTerminalPhase(phase.value)) {
      return;
    }
    if (activationPhaseOrder(event.phase) < activationPhaseOrder(phase.value)) {
      return;
    }
    phase.value = event.phase;
    progressStep.value = Math.max(
      progressStep.value,
      activationProgressStep(event.phase),
    ) as ActivationProgressStep;
    message.value = event.message;
  }

  function errorMessage(reason: unknown, fallback: string): string {
    if (typeof reason === "object" && reason !== null && "message" in reason) {
      return String((reason as CommandError).message);
    }
    return typeof reason === "string" ? reason : fallback;
  }

  return {
    availabilityState,
    availabilityError,
    available,
    canActivate,
    running,
    selectedExecutablePath,
    phase,
    progressStep,
    message,
    error,
    result,
    resultMessage,
    networkStatusState,
    networkStatus,
    networkStatusError,
    networkPending,
    localProxyActive,
    recoveryRunning,
    recoveryError,
    initialize,
    dispose,
    refreshAvailability,
    activate,
    refreshNetworkStatus,
    restoreOriginalNetwork,
  };
});

function activationProgressStep(phase: ActivationPhase): ActivationProgressStep {
  switch (phase) {
    case "idle":
    case "failed":
      return 0;
    case "detectingApp":
    case "fetchingProxyConfig":
    case "filteringProxyNodes":
    case "testingProxyNodes":
    case "selectingProxyNode":
      return 1;
    case "startingLocalProxy":
    case "savingNetworkState":
      return 2;
    case "writingLocale":
    case "stoppingDesktopApp":
    case "launchingDesktopApp":
      return 3;
    case "verifying":
    case "restoringNetwork":
    case "stoppingLocalProxy":
    case "succeeded":
      return 4;
    default:
      return assertNever(phase);
  }
}

function activationPhaseOrder(phase: ActivationPhase): number {
  switch (phase) {
    case "idle":
      return 0;
    case "detectingApp":
      return 1;
    case "fetchingProxyConfig":
      return 2;
    case "filteringProxyNodes":
      return 3;
    case "testingProxyNodes":
      return 4;
    case "selectingProxyNode":
      return 5;
    case "startingLocalProxy":
      return 6;
    case "savingNetworkState":
      return 7;
    case "writingLocale":
      return 8;
    case "stoppingDesktopApp":
      return 9;
    case "launchingDesktopApp":
      return 10;
    case "verifying":
      return 11;
    case "restoringNetwork":
      return 12;
    case "stoppingLocalProxy":
      return 13;
    case "succeeded":
    case "failed":
      return 14;
    default:
      return assertNever(phase);
  }
}

function isTerminalPhase(phase: ActivationPhase): boolean {
  return phase === "succeeded" || phase === "failed";
}

function assertNever(value: never): never {
  throw new Error(`Unhandled activation phase: ${String(value)}`);
}
