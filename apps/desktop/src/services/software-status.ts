import { invoke } from "@tauri-apps/api/core";
import type { SoftwareInstallationStatus } from "../types/software-status";

export function getSoftwareInstallationStatuses(): Promise<SoftwareInstallationStatus[]> {
  return invoke<SoftwareInstallationStatus[]>("get_software_installation_statuses");
}
