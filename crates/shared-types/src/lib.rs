use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperatingSystem {
    Windows,
    MacOs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CpuArchitecture {
    X64,
    Arm64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DesktopProduct {
    ChatGpt,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SoftwareProductId {
    ChatGptDesktop,
    ClaudeDesktop,
    CcSwitch,
    NodeJsLts,
    VisualStudioCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CliToolId {
    CodexCli,
    ClaudeCodeCli,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliToolStatus {
    pub id: CliToolId,
    pub display_name: String,
    pub installed: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliToolsOverview {
    pub node_version: Option<String>,
    pub npm_version: Option<String>,
    pub tools: Vec<CliToolStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstalledSoftwareId {
    ChatGpt,
    ClaudeDesktop,
    CcSwitch,
    NodeJsLts,
    VisualStudioCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareInstallationStatus {
    pub id: InstalledSoftwareId,
    pub installed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadPackageKind {
    Dmg,
    ExeBootstrapper,
    Msi,
    Msix,
    Pkg,
    Zip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadCompatibility {
    Native,
    VendorBootstrapper,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareArtifactSummary {
    pub id: String,
    pub operating_system: OperatingSystem,
    pub native_architecture: Option<CpuArchitecture>,
    pub compatibility: DownloadCompatibility,
    pub package_kind: DownloadPackageKind,
    pub file_name: String,
    pub minimum_os: Option<String>,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareProductSummary {
    pub id: SoftwareProductId,
    pub display_name: String,
    pub publisher: String,
    pub official_page_url: Url,
    pub artifacts: Vec<SoftwareArtifactSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadCatalog {
    pub operating_system: OperatingSystem,
    pub cpu_architecture: CpuArchitecture,
    pub products: Vec<SoftwareProductSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadTaskState {
    Queued,
    Resolving,
    Downloading,
    Ready,
    Cancelled,
    Failed,
    Launching,
    Launched,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTaskSnapshot {
    pub id: Uuid,
    pub product_id: SoftwareProductId,
    pub artifact_id: String,
    pub state: DownloadTaskState,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub resumed_from: u64,
    pub file_name: String,
    pub error: Option<CommandError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopApp {
    pub product: DesktopProduct,
    pub display_name: String,
    pub install_path: String,
    pub executable_path: String,
    pub bundle_identifier: Option<String>,
    pub version: Option<String>,
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocaleStatus {
    pub chinese_enabled: bool,
    pub config_locale: Option<String>,
    pub global_state_locale: Option<String>,
    pub config_path: String,
    pub global_state_path: String,
    pub restore_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocaleOverview {
    pub apps: Vec<DesktopApp>,
    pub locale: LocaleStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairOverview {
    pub app: Option<DesktopApp>,
    pub locale: LocaleStatus,
    pub activation_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocaleActivationResult {
    pub app: DesktopApp,
    pub locale: LocaleStatus,
    pub config_changed: bool,
    pub global_state_changed: bool,
    pub restarted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkRecoveryStatus {
    pub pending: bool,
    pub local_proxy_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocaleRestoreResult {
    pub app: Option<DesktopApp>,
    pub locale: LocaleStatus,
    pub restored_files: Vec<String>,
    pub configuration_restored: bool,
    pub restarted: bool,
    pub restart_warning: Option<CommandError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivationPhase {
    Idle,
    DetectingApp,
    FetchingProxyConfig,
    FilteringProxyNodes,
    TestingProxyNodes,
    SelectingProxyNode,
    StartingLocalProxy,
    SavingNetworkState,
    WritingLocale,
    StoppingDesktopApp,
    LaunchingDesktopApp,
    Verifying,
    RestoringNetwork,
    StoppingLocalProxy,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationEvent {
    pub phase: ActivationPhase,
    pub message: String,
    pub occurred_at: DateTime<Utc>,
}

impl ActivationEvent {
    #[must_use]
    pub fn new(phase: ActivationPhase, message: impl Into<String>) -> Self {
        Self {
            phase,
            message: message.into(),
            occurred_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionEndpoint {
    pub id: Uuid,
    pub url: Url,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_certificate_der_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfig {
    pub version: String,
    pub generated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub subscription_endpoints: Vec<SubscriptionEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedClientConfig {
    pub payload: ClientConfig,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub ok: bool,
    pub service: String,
    pub version: String,
}
