export type InstalledSoftwareId =
  "chatGpt" | "claudeDesktop" | "ccSwitch" | "nodeJsLts" | "visualStudioCode";

export interface SoftwareInstallationStatus {
  id: InstalledSoftwareId;
  installed: boolean;
}
