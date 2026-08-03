use shared_types::{InstalledSoftwareId, SoftwareInstallationStatus};

#[tauri::command]
pub async fn get_software_installation_statuses() -> Vec<SoftwareInstallationStatus> {
    let mut statuses = desktop_discovery::detect_supported_software();
    let cli_overview = cli_tools::inspect_cli_tools().await;
    statuses.push(SoftwareInstallationStatus {
        id: InstalledSoftwareId::NodeJsLts,
        installed: cli_overview.node_version.is_some() && cli_overview.npm_version.is_some(),
        version: cli_overview.node_version,
    });
    statuses
}
