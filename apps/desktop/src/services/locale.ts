import { invoke } from "@tauri-apps/api/core";
import type { LocaleOverview, LocaleRestoreResult, RepairOverview } from "../types/locale";

export function getLocaleOverview(): Promise<LocaleOverview> {
  return invoke<LocaleOverview>("get_locale_overview");
}

export function restoreLocaleConfiguration(
  selectedExecutablePath?: string,
): Promise<LocaleRestoreResult> {
  return invoke<LocaleRestoreResult>("restore_locale_configuration", {
    selectedExecutablePath: selectedExecutablePath ?? null,
  });
}

export function getRepairOverview(): Promise<RepairOverview> {
  return invoke<RepairOverview>("get_repair_overview");
}
