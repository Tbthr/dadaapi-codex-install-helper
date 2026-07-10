import { invoke } from "@tauri-apps/api/core";
import type { LocaleOverview } from "../types/locale";

export function getLocaleOverview(): Promise<LocaleOverview> {
  return invoke<LocaleOverview>("get_locale_overview");
}
