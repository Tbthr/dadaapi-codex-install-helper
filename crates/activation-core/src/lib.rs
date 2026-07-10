mod coordinator;
mod proxy_preparation;

pub use coordinator::{
    ActivationCoordinator, ChineseEffectError, ChineseEffectVerifier, RuntimeChineseEffectVerifier,
};
pub use proxy_preparation::{
    default_activation_selection_options, ProxyPreparationError, ProxyPreparationService,
    RemoteProxyPreparationService, StaticRouteProxyPreparationService,
};

use chrono::{DateTime, Utc};
use desktop_discovery::{DesktopDiscovery, DiscoveryError, SystemDesktopDiscovery};
use locale_config::{
    default_locale_paths, inspect_locale, restore_locale, LocaleConfigError, LocalePaths,
};
use platform::{
    LocalProxySettings, NativePlatformAdapter, NetworkState, PlatformAdapter, PlatformError,
};
use serde::{Deserialize, Serialize};
use shared_types::{
    ActivationPhase, DesktopApp, LocaleOverview, LocaleRestoreResult, OperatingSystem,
};
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

const RECOVERY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum ActivationError {
    #[error("激活流程状态错误：无法从 {from:?} 进入 {to:?}")]
    InvalidTransition {
        from: ActivationPhase,
        to: ActivationPhase,
    },
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    #[error(transparent)]
    Locale(#[from] LocaleConfigError),
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[error(transparent)]
    ProxyPreparation(#[from] ProxyPreparationError),
    #[error(transparent)]
    LocalProxy(#[from] proxy_core::LocalProxyError),
    #[error(transparent)]
    NetworkSafety(#[from] NetworkSafetyError),
    #[error(transparent)]
    ChineseEffect(#[from] ChineseEffectError),
    #[error("未检测到 ChatGPT 或 Codex，请先安装并至少打开一次")]
    DesktopAppNotFound,
    #[error("所选的 ChatGPT/Codex 已不存在，请重新检测")]
    SelectedAppNotFound,
    #[error("后台任务执行失败：{0}")]
    BackgroundTask(String),
    #[error("中文配置写入后校验未通过")]
    VerificationFailed,
    #[error("上一次网络恢复尚未完成，暂时不能开始新的激活")]
    PendingNetworkRecovery,
    #[error("本地代理会话已经丢失，无法执行安全清理")]
    LocalProxySessionMissing,
    #[error("激活操作失败：{operation}；清理同时失败：{cleanup}")]
    OperationAndCleanupFailed {
        operation: Box<ActivationError>,
        cleanup: Box<ActivationError>,
    },
}

#[derive(Debug, Error)]
pub enum RecoveryError {
    #[error("网络恢复文件路径无效")]
    InvalidPath,
    #[error("网络恢复文件操作失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("网络恢复文件格式无效：{0}")]
    Json(#[from] serde_json::Error),
    #[error("不支持的网络恢复文件版本：{0}")]
    UnsupportedVersion(u32),
    #[error("网络恢复文件属于其他操作系统")]
    OperatingSystemMismatch,
    #[error("网络恢复文件缺少原始网络状态")]
    MissingNetworkState,
}

#[derive(Debug, Error)]
pub enum NetworkSafetyError {
    #[error(transparent)]
    Recovery(#[from] RecoveryError),
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[error("应用临时代理失败：{0}")]
    ApplyFailed(PlatformError),
    #[error("应用临时代理失败，并且原网络恢复也失败：应用={apply}; 恢复={restore}")]
    ApplyAndRestoreFailed { apply: String, restore: String },
    #[error("网络恢复记录意外缺失，无法确认原网络已经恢复")]
    MissingRecoveryRecord,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkRecoveryRecord {
    pub schema_version: u32,
    pub created_at: DateTime<Utc>,
    pub operating_system: OperatingSystem,
    network_state: String,
}

impl NetworkRecoveryRecord {
    #[must_use]
    pub fn network_state(&self) -> NetworkState {
        NetworkState::from_serialized(self.network_state.clone())
    }
}

impl fmt::Debug for NetworkRecoveryRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NetworkRecoveryRecord")
            .field("schema_version", &self.schema_version)
            .field("created_at", &self.created_at)
            .field("operating_system", &self.operating_system)
            .field("network_state", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone)]
pub struct NetworkRecoveryStore {
    path: PathBuf,
}

impl NetworkRecoveryStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn save(
        &self,
        operating_system: OperatingSystem,
        state: &NetworkState,
    ) -> Result<NetworkRecoveryRecord, RecoveryError> {
        if state.expose_serialized().trim().is_empty() {
            return Err(RecoveryError::MissingNetworkState);
        }
        let record = NetworkRecoveryRecord {
            schema_version: RECOVERY_SCHEMA_VERSION,
            created_at: Utc::now(),
            operating_system,
            network_state: state.expose_serialized().to_owned(),
        };
        self.write(&record)?;
        Ok(record)
    }

    pub fn load(
        &self,
        expected_operating_system: OperatingSystem,
    ) -> Result<Option<NetworkRecoveryRecord>, RecoveryError> {
        let payload = match fs::read(&self.path) {
            Ok(payload) => payload,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let record: NetworkRecoveryRecord = serde_json::from_slice(&payload)?;
        if record.schema_version != RECOVERY_SCHEMA_VERSION {
            return Err(RecoveryError::UnsupportedVersion(record.schema_version));
        }
        if record.operating_system != expected_operating_system {
            return Err(RecoveryError::OperatingSystemMismatch);
        }
        if record.network_state.trim().is_empty() {
            return Err(RecoveryError::MissingNetworkState);
        }
        Ok(Some(record))
    }

    pub fn clear(&self) -> Result<(), RecoveryError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn write(&self, record: &NetworkRecoveryRecord) -> Result<(), RecoveryError> {
        let parent = self
            .path
            .parent()
            .filter(|value| !value.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let temporary_path = recovery_temporary_path(&self.path, parent)?;
        let payload = serde_json::to_vec_pretty(record)?;
        let write_result = write_private_recovery_file(&temporary_path, &payload)
            .and_then(|()| replace_recovery_file(&temporary_path, &self.path));
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        write_result?;
        Ok(())
    }
}

impl fmt::Debug for NetworkRecoveryStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NetworkRecoveryStore([PRIVATE PATH])")
    }
}

#[derive(Debug, Clone)]
pub struct NetworkRecoveryService<P> {
    platform: P,
    store: NetworkRecoveryStore,
    operating_system: OperatingSystem,
}

impl<P> NetworkRecoveryService<P>
where
    P: PlatformAdapter,
{
    #[must_use]
    pub fn new(
        platform: P,
        store: NetworkRecoveryStore,
        operating_system: OperatingSystem,
    ) -> Self {
        Self {
            platform,
            store,
            operating_system,
        }
    }

    pub async fn save_and_apply_proxy(
        &self,
        settings: &LocalProxySettings,
    ) -> Result<(), NetworkSafetyError> {
        let state = self.platform.save_network_state().await?;
        self.store.save(self.operating_system, &state)?;
        if let Err(apply_error) = self.platform.apply_local_proxy(settings).await {
            return match self.platform.restore_network_state(&state).await {
                Ok(()) => {
                    self.store.clear()?;
                    Err(NetworkSafetyError::ApplyFailed(apply_error))
                }
                Err(restore_error) => Err(NetworkSafetyError::ApplyAndRestoreFailed {
                    apply: apply_error.to_string(),
                    restore: restore_error.to_string(),
                }),
            };
        }
        Ok(())
    }

    pub async fn restore_pending(&self) -> Result<bool, NetworkSafetyError> {
        let Some(record) = self.store.load(self.operating_system)? else {
            return Ok(false);
        };
        self.platform
            .restore_network_state(&record.network_state())
            .await?;
        self.store.clear()?;
        Ok(true)
    }

    pub fn has_pending(&self) -> Result<bool, NetworkSafetyError> {
        Ok(self.store.load(self.operating_system)?.is_some())
    }
}

fn recovery_temporary_path(path: &Path, parent: &Path) -> Result<PathBuf, RecoveryError> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(RecoveryError::InvalidPath)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce)))
}

fn write_private_recovery_file(path: &Path, payload: &[u8]) -> Result<(), std::io::Error> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(payload)?;
    file.sync_all()?;
    Ok(())
}

fn replace_recovery_file(temporary_path: &Path, path: &Path) -> Result<(), std::io::Error> {
    #[cfg(target_os = "windows")]
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary_path, path)
}

#[derive(Debug, Clone)]
pub struct ActivationMachine {
    phase: ActivationPhase,
}

impl Default for ActivationMachine {
    fn default() -> Self {
        Self {
            phase: ActivationPhase::Idle,
        }
    }
}

impl ActivationMachine {
    #[must_use]
    pub fn phase(&self) -> ActivationPhase {
        self.phase
    }

    pub fn transition(&mut self, next: ActivationPhase) -> Result<(), ActivationError> {
        if is_allowed_transition(self.phase, next) {
            self.phase = next;
            return Ok(());
        }

        Err(ActivationError::InvalidTransition {
            from: self.phase,
            to: next,
        })
    }
}

fn is_allowed_transition(from: ActivationPhase, to: ActivationPhase) -> bool {
    use ActivationPhase::{
        DetectingApp, Failed, FetchingProxyConfig, FilteringProxyNodes, Idle, LaunchingDesktopApp,
        RestoringNetwork, SavingNetworkState, SelectingProxyNode, StartingLocalProxy,
        StoppingDesktopApp, StoppingLocalProxy, Succeeded, TestingProxyNodes, Verifying,
        WritingLocale,
    };

    matches!(
        (from, to),
        (Idle, DetectingApp)
            | (DetectingApp, FetchingProxyConfig)
            | (FetchingProxyConfig, FilteringProxyNodes)
            | (FilteringProxyNodes, TestingProxyNodes)
            | (TestingProxyNodes, SelectingProxyNode)
            | (SelectingProxyNode, StartingLocalProxy)
            | (StartingLocalProxy, SavingNetworkState | StoppingLocalProxy)
            | (SavingNetworkState, WritingLocale | StoppingLocalProxy)
            | (WritingLocale, StoppingDesktopApp)
            | (StoppingDesktopApp, LaunchingDesktopApp)
            | (LaunchingDesktopApp, Verifying)
            | (Verifying, Succeeded)
            | (
                WritingLocale | StoppingDesktopApp | LaunchingDesktopApp | Verifying,
                RestoringNetwork
            )
            | (RestoringNetwork, StoppingLocalProxy)
            | (StoppingLocalProxy, Succeeded | Failed)
    ) || (from != Succeeded && from != Failed && to == Failed)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LocaleActivationService {
    discovery: SystemDesktopDiscovery,
    platform: NativePlatformAdapter,
}

impl LocaleActivationService {
    pub async fn overview(&self) -> Result<LocaleOverview, ActivationError> {
        let apps = detect_apps(self.discovery).await?;
        let paths = default_locale_paths()?;
        let locale = inspect_paths(paths).await?;
        Ok(LocaleOverview { apps, locale })
    }

    pub async fn restore(
        &self,
        selected_executable_path: Option<String>,
    ) -> Result<LocaleRestoreResult, ActivationError> {
        let apps = detect_apps(self.discovery).await?;
        let app = match selected_executable_path.as_deref() {
            Some(selected) => Some(select_app(apps, Some(selected))?),
            None => apps.into_iter().next(),
        };
        let paths = default_locale_paths()?;
        let restore_paths = paths.clone();
        let restored = tokio::task::spawn_blocking(move || restore_locale(&restore_paths))
            .await
            .map_err(|error| ActivationError::BackgroundTask(error.to_string()))??;

        let restarted = if let Some(app) = app.as_ref() {
            self.platform.stop_desktop_app(app).await?;
            self.platform.launch_desktop_app(app).await?;
            true
        } else {
            false
        };

        Ok(LocaleRestoreResult {
            app,
            locale: restored.status,
            restored_files: restored.restored_files,
            restarted,
        })
    }
}

async fn detect_apps(
    discovery: SystemDesktopDiscovery,
) -> Result<Vec<DesktopApp>, ActivationError> {
    tokio::task::spawn_blocking(move || discovery.detect())
        .await
        .map_err(|error| ActivationError::BackgroundTask(error.to_string()))?
        .map_err(ActivationError::from)
}

async fn inspect_paths(paths: LocalePaths) -> Result<shared_types::LocaleStatus, ActivationError> {
    tokio::task::spawn_blocking(move || inspect_locale(&paths))
        .await
        .map_err(|error| ActivationError::BackgroundTask(error.to_string()))?
        .map_err(ActivationError::from)
}

fn select_app(
    apps: Vec<DesktopApp>,
    selected_executable_path: Option<&str>,
) -> Result<DesktopApp, ActivationError> {
    let Some(selected) = selected_executable_path else {
        return apps
            .into_iter()
            .next()
            .ok_or(ActivationError::DesktopAppNotFound);
    };
    apps.into_iter()
        .find(|app| paths_equal(&app.executable_path, selected))
        .ok_or(ActivationError::SelectedAppNotFound)
}

fn paths_equal(left: &str, right: &str) -> bool {
    if cfg!(target_os = "windows") {
        left.replace('/', "\\")
            .eq_ignore_ascii_case(&right.replace('/', "\\"))
    } else {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::{Arc, Mutex};

    #[test]
    fn rejects_skipping_required_activation_steps() {
        let mut machine = ActivationMachine::default();
        let error = machine
            .transition(ActivationPhase::WritingLocale)
            .expect_err("invalid transition");
        assert!(matches!(error, ActivationError::InvalidTransition { .. }));
    }

    #[test]
    fn supports_proxy_backed_activation_flow_until_manual_recovery() {
        let mut machine = ActivationMachine::default();
        for phase in [
            ActivationPhase::DetectingApp,
            ActivationPhase::FetchingProxyConfig,
            ActivationPhase::FilteringProxyNodes,
            ActivationPhase::TestingProxyNodes,
            ActivationPhase::SelectingProxyNode,
            ActivationPhase::StartingLocalProxy,
            ActivationPhase::SavingNetworkState,
            ActivationPhase::WritingLocale,
            ActivationPhase::StoppingDesktopApp,
            ActivationPhase::LaunchingDesktopApp,
            ActivationPhase::Verifying,
            ActivationPhase::Succeeded,
        ] {
            machine.transition(phase).expect("valid transition");
        }
        assert_eq!(machine.phase(), ActivationPhase::Succeeded);
    }

    #[test]
    fn recovery_store_round_trips_and_clears_private_network_state() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("recovery.json");
        let store = NetworkRecoveryStore::new(path.clone());
        let state = NetworkState::from_serialized(
            r#"{"services":[{"name":"Wi-Fi","server":"127.0.0.1"}]}"#.to_owned(),
        );

        store
            .save(OperatingSystem::MacOs, &state)
            .expect("save recovery state");
        let loaded = store
            .load(OperatingSystem::MacOs)
            .expect("load recovery state")
            .expect("recovery record");

        assert_eq!(loaded.operating_system, OperatingSystem::MacOs);
        assert_eq!(loaded.network_state(), state);
        assert!(!format!("{loaded:?}").contains("127.0.0.1"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path)
                .expect("recovery metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }

        store.clear().expect("clear recovery state");
        store.clear().expect("idempotent clear");
        assert!(store
            .load(OperatingSystem::MacOs)
            .expect("missing recovery state")
            .is_none());
    }

    #[test]
    fn recovery_store_rejects_other_operating_system() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = NetworkRecoveryStore::new(directory.path().join("recovery.json"));
        let state = NetworkState::from_serialized(r#"{"proxyEnabled":false}"#.to_owned());
        store
            .save(OperatingSystem::MacOs, &state)
            .expect("save recovery state");

        let error = store
            .load(OperatingSystem::Windows)
            .expect_err("operating system mismatch");
        assert!(matches!(error, RecoveryError::OperatingSystemMismatch));
    }

    #[tokio::test]
    async fn network_recovery_service_keeps_record_until_restore_succeeds() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("recovery.json");
        let platform = MockPlatform::default();
        let service = NetworkRecoveryService::new(
            platform.clone(),
            NetworkRecoveryStore::new(path.clone()),
            OperatingSystem::MacOs,
        );
        let settings = local_proxy_settings();

        service
            .save_and_apply_proxy(&settings)
            .await
            .expect("apply local proxy");
        assert!(path.exists());
        assert!(service.restore_pending().await.expect("restore network"));
        assert!(!path.exists());

        let state = platform.state.lock().expect("mock state");
        assert_eq!(state.save_calls, 1);
        assert_eq!(state.apply_calls, 1);
        assert_eq!(state.restore_calls, 1);
    }

    #[tokio::test]
    async fn failed_proxy_apply_restores_immediately_and_clears_record() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("recovery.json");
        let platform = MockPlatform::with_failures(true, false);
        let service = NetworkRecoveryService::new(
            platform.clone(),
            NetworkRecoveryStore::new(path.clone()),
            OperatingSystem::MacOs,
        );

        let error = service
            .save_and_apply_proxy(&local_proxy_settings())
            .await
            .expect_err("proxy apply failure");

        assert!(matches!(error, NetworkSafetyError::ApplyFailed(_)));
        assert!(!path.exists());
        assert_eq!(platform.state.lock().expect("mock state").restore_calls, 1);
    }

    #[tokio::test]
    async fn failed_restore_preserves_recovery_record_for_next_launch() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("recovery.json");
        let platform = MockPlatform::with_failures(true, true);
        let service = NetworkRecoveryService::new(
            platform,
            NetworkRecoveryStore::new(path.clone()),
            OperatingSystem::MacOs,
        );

        let error = service
            .save_and_apply_proxy(&local_proxy_settings())
            .await
            .expect_err("apply and restore failure");

        assert!(matches!(
            error,
            NetworkSafetyError::ApplyAndRestoreFailed { .. }
        ));
        assert!(path.exists());
    }

    fn local_proxy_settings() -> LocalProxySettings {
        LocalProxySettings::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 17_892),
            vec!["*.local".to_owned()],
        )
        .expect("valid local proxy settings")
    }

    #[derive(Debug, Default)]
    struct MockPlatformState {
        save_calls: usize,
        apply_calls: usize,
        restore_calls: usize,
        apply_fails: bool,
        restore_fails: bool,
    }

    #[derive(Debug, Clone, Default)]
    struct MockPlatform {
        state: Arc<Mutex<MockPlatformState>>,
    }

    impl MockPlatform {
        fn with_failures(apply_fails: bool, restore_fails: bool) -> Self {
            Self {
                state: Arc::new(Mutex::new(MockPlatformState {
                    apply_fails,
                    restore_fails,
                    ..MockPlatformState::default()
                })),
            }
        }
    }

    #[async_trait]
    impl PlatformAdapter for MockPlatform {
        async fn stop_desktop_app(&self, _app: &DesktopApp) -> Result<(), PlatformError> {
            Ok(())
        }

        async fn launch_desktop_app(&self, _app: &DesktopApp) -> Result<(), PlatformError> {
            Ok(())
        }

        async fn desktop_app_uses_locale(
            &self,
            _app: &DesktopApp,
            _locale: &str,
        ) -> Result<bool, PlatformError> {
            Ok(true)
        }

        async fn save_network_state(&self) -> Result<NetworkState, PlatformError> {
            self.state.lock().expect("mock state").save_calls += 1;
            Ok(NetworkState::from_serialized(
                r#"{"proxyEnabled":false}"#.to_owned(),
            ))
        }

        async fn apply_local_proxy(
            &self,
            _settings: &LocalProxySettings,
        ) -> Result<(), PlatformError> {
            let mut state = self.state.lock().expect("mock state");
            state.apply_calls += 1;
            if state.apply_fails {
                Err(PlatformError::Operation("mock apply failure".to_owned()))
            } else {
                Ok(())
            }
        }

        async fn restore_network_state(&self, _state: &NetworkState) -> Result<(), PlatformError> {
            let mut state = self.state.lock().expect("mock state");
            state.restore_calls += 1;
            if state.restore_fails {
                Err(PlatformError::Operation("mock restore failure".to_owned()))
            } else {
                Ok(())
            }
        }
    }
}
