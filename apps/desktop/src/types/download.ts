import type { CommandError } from "./locale";

export type OperatingSystem = "windows" | "macOs";
export type CpuArchitecture = "x64" | "arm64";
export type SoftwareProductId = "chatGptDesktop" | "claudeDesktop" | "ccSwitch" | "nodeJsLts";
export type DownloadPackageKind = "dmg" | "exeBootstrapper" | "msi" | "msix" | "pkg";
export type DownloadCompatibility = "native" | "vendorBootstrapper" | "unsupported";

export interface SoftwareArtifactSummary {
  id: string;
  operatingSystem: OperatingSystem;
  nativeArchitecture: CpuArchitecture | null;
  compatibility: DownloadCompatibility;
  packageKind: DownloadPackageKind;
  fileName: string;
  minimumOs: string | null;
  available: boolean;
}

export interface SoftwareProductSummary {
  id: SoftwareProductId;
  displayName: string;
  publisher: string;
  officialPageUrl: string;
  artifacts: SoftwareArtifactSummary[];
}

export interface DownloadCatalog {
  operatingSystem: OperatingSystem;
  cpuArchitecture: CpuArchitecture;
  products: SoftwareProductSummary[];
}

export type DownloadTaskState =
  | "queued"
  | "resolving"
  | "downloading"
  | "ready"
  | "cancelled"
  | "failed"
  | "launching"
  | "launched";

export interface DownloadTaskSnapshot {
  id: string;
  productId: SoftwareProductId;
  artifactId: string;
  state: DownloadTaskState;
  downloadedBytes: number;
  totalBytes: number | null;
  resumedFrom: number;
  fileName: string;
  error: CommandError | null;
}
