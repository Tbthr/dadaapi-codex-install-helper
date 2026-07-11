import { invoke } from "@tauri-apps/api/core";
import type { DownloadCatalog, DownloadTaskSnapshot, SoftwareProductId } from "../types/download";

export const DOWNLOAD_TASK_UPDATED_EVENT = "download-task-updated";

export function getDownloadCatalog(): Promise<DownloadCatalog> {
  return invoke<DownloadCatalog>("get_download_catalog");
}

export function getOfficialDownloadLink(
  productId: SoftwareProductId,
  artifactId?: string,
): Promise<string> {
  return invoke<string>("get_official_download_link", {
    productId,
    artifactId: artifactId ?? null,
  });
}

export function listDownloadTasks(): Promise<DownloadTaskSnapshot[]> {
  return invoke<DownloadTaskSnapshot[]>("list_download_tasks");
}

export function startDownload(
  productId: SoftwareProductId,
  artifactId?: string,
): Promise<DownloadTaskSnapshot> {
  return invoke<DownloadTaskSnapshot>("start_download", {
    productId,
    artifactId: artifactId ?? null,
  });
}

export function cancelDownload(taskId: string): Promise<DownloadTaskSnapshot> {
  return invoke<DownloadTaskSnapshot>("cancel_download", { taskId });
}

export function retryDownload(taskId: string): Promise<DownloadTaskSnapshot> {
  return invoke<DownloadTaskSnapshot>("retry_download", { taskId });
}

export function revealDownload(taskId: string): Promise<void> {
  return invoke<void>("reveal_download", { taskId });
}

export function launchInstaller(taskId: string): Promise<DownloadTaskSnapshot> {
  return invoke<DownloadTaskSnapshot>("launch_installer", { taskId });
}

export function openOfficialProductPage(productId: SoftwareProductId): Promise<void> {
  return invoke<void>("open_official_product_page", { productId });
}
