import { invoke } from "@tauri-apps/api/core";
import type { LocaleActivationResult } from "../types/locale";

export const ACTIVATION_PROGRESS_EVENT = "activation-progress";

export function isActivationAvailable(): Promise<boolean> {
  return invoke<boolean>("is_activation_available");
}

export function activateChinese(selectedExecutablePath: string): Promise<LocaleActivationResult> {
  return invoke<LocaleActivationResult>("activate_chinese", { selectedExecutablePath });
}
