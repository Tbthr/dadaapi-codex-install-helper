import { invoke } from "@tauri-apps/api/core";
import type { LocaleActivationResult, NetworkRecoveryStatus } from "../types/locale";

export const ACTIVATION_PROGRESS_EVENT = "activation-progress";

export function isActivationAvailable(): Promise<boolean> {
  return invoke<boolean>("is_activation_available");
}

export function activateChinese(selectedExecutablePath: string): Promise<LocaleActivationResult> {
  return invoke<LocaleActivationResult>("activate_chinese", { selectedExecutablePath });
}

export function getNetworkRecoveryStatus(): Promise<NetworkRecoveryStatus> {
  return invoke<NetworkRecoveryStatus>("get_network_recovery_status");
}

export function restoreNetwork(): Promise<NetworkRecoveryStatus> {
  return invoke<NetworkRecoveryStatus>("restore_network");
}

export function prepareActivationNetwork(): Promise<NetworkRecoveryStatus> {
  return invoke<NetworkRecoveryStatus>("prepare_activation_network");
}
