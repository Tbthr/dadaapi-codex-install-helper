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
    pub restarted: bool,
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
