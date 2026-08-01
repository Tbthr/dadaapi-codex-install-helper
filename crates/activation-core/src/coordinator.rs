use crate::{
    ActivationError, ActivationMachine, NetworkRecoveryService, NetworkRecoveryStore,
    NetworkSafetyError,
};
use async_trait::async_trait;
use desktop_discovery::DesktopDiscovery;
use locale_config::{apply_chinese_locale, inspect_locale, LocalePaths};
use platform::{LocalProxySettings, PlatformAdapter};
use proxy_core::{LocalProxyEngine, LocalProxySupervisor, SupervisedProxySession};
use shared_types::{
    ActivationPhase, DesktopApp, LocaleActivationResult, LocaleStatus, NetworkRecoveryStatus,
    OperatingSystem,
};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};

use crate::proxy_preparation::ProxyPreparationService;

#[derive(Debug, Error)]
pub enum ChineseEffectError {
    #[error("ChatGPT/Codex 中文界面验证未通过")]
    NotEffective,
    #[error("无法验证 ChatGPT/Codex 中文界面：{0}")]
    Inspection(String),
}

#[async_trait]
pub trait ChineseEffectVerifier: Send + Sync {
    async fn verify(
        &self,
        app: &DesktopApp,
        locale_paths: &LocalePaths,
    ) -> Result<LocaleStatus, ChineseEffectError>;
}

#[derive(Debug, Clone)]
pub struct RuntimeChineseEffectVerifier<P> {
    platform: P,
    timeout: Duration,
    poll_interval: Duration,
}

impl<P> RuntimeChineseEffectVerifier<P> {
    #[must_use]
    pub fn new(platform: P, timeout: Duration, poll_interval: Duration) -> Self {
        Self {
            platform,
            timeout,
            poll_interval,
        }
    }
}

#[async_trait]
impl<P> ChineseEffectVerifier for RuntimeChineseEffectVerifier<P>
where
    P: PlatformAdapter,
{
    async fn verify(
        &self,
        app: &DesktopApp,
        locale_paths: &LocalePaths,
    ) -> Result<LocaleStatus, ChineseEffectError> {
        let inspect_paths = locale_paths.clone();
        let status = tokio::task::spawn_blocking(move || inspect_locale(&inspect_paths))
            .await
            .map_err(|error| ChineseEffectError::Inspection(error.to_string()))?
            .map_err(|error| ChineseEffectError::Inspection(error.to_string()))?;
        if !status.chinese_enabled {
            return Err(ChineseEffectError::NotEffective);
        }

        let deadline = Instant::now() + self.timeout;
        loop {
            let uses_locale = self
                .platform
                .desktop_app_uses_locale(app, "zh-CN")
                .await
                .map_err(|error| ChineseEffectError::Inspection(error.to_string()))?;
            if uses_locale {
                return Ok(status);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(ChineseEffectError::NotEffective);
            }
            sleep(self.poll_interval.min(deadline - now)).await;
        }
    }
}

pub struct ActivationCoordinator<D, P, R, V, E> {
    discovery: D,
    platform: P,
    proxy_preparation: R,
    effect_verifier: V,
    proxy_supervisor: LocalProxySupervisor<E>,
    recovery: NetworkRecoveryService<P>,
    locale_paths: LocalePaths,
    activation_lock: Mutex<()>,
    active_proxy: Mutex<Option<SupervisedProxySession>>,
}

impl<D, P, R, V, E> ActivationCoordinator<D, P, R, V, E>
where
    D: DesktopDiscovery + Clone + Send + Sync + 'static,
    P: PlatformAdapter + Clone,
    R: ProxyPreparationService,
    V: ChineseEffectVerifier,
    E: LocalProxyEngine,
{
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        discovery: D,
        platform: P,
        proxy_preparation: R,
        effect_verifier: V,
        proxy_engine: E,
        proxy_readiness_timeout: Duration,
        recovery_store: NetworkRecoveryStore,
        operating_system: OperatingSystem,
        locale_paths: LocalePaths,
    ) -> Self {
        Self {
            discovery,
            platform: platform.clone(),
            proxy_preparation,
            effect_verifier,
            proxy_supervisor: LocalProxySupervisor::new(proxy_engine, proxy_readiness_timeout),
            recovery: NetworkRecoveryService::new(platform, recovery_store, operating_system),
            locale_paths,
            activation_lock: Mutex::new(()),
            active_proxy: Mutex::new(None),
        }
    }

    pub async fn activate(
        &self,
        selected_executable_path: Option<String>,
    ) -> Result<LocaleActivationResult, ActivationError> {
        self.activate_with_progress(selected_executable_path, |_| {})
            .await
    }

    pub async fn activate_with_progress<F>(
        &self,
        selected_executable_path: Option<String>,
        progress: F,
    ) -> Result<LocaleActivationResult, ActivationError>
    where
        F: Fn(ActivationPhase) + Send + Sync,
    {
        let _activation_guard = self.activation_lock.lock().await;
        let mut machine = ActivationMachine::default();
        let result = match self.recovery.has_pending() {
            Err(error) => Err(ActivationError::NetworkSafety(error)),
            Ok(true) => Err(ActivationError::PendingNetworkRecovery),
            Ok(false) if self.active_proxy.lock().await.is_some() => {
                Err(ActivationError::PendingNetworkRecovery)
            }
            Ok(false) => {
                self.run_activation(&mut machine, selected_executable_path.as_deref(), &progress)
                    .await
            }
        };
        if result.is_err() && machine.phase() != ActivationPhase::Failed {
            let _ = transition_with_progress(&mut machine, ActivationPhase::Failed, &progress);
        }
        result
    }

    async fn run_activation<F>(
        &self,
        machine: &mut ActivationMachine,
        selected_executable_path: Option<&str>,
        progress: &F,
    ) -> Result<LocaleActivationResult, ActivationError>
    where
        F: Fn(ActivationPhase) + Send + Sync + ?Sized,
    {
        transition_with_progress(machine, ActivationPhase::DetectingApp, progress)?;
        let apps = detect_apps(self.discovery.clone()).await?;
        let app = select_app(apps, selected_executable_path)?;

        transition_with_progress(machine, ActivationPhase::FetchingProxyConfig, progress)?;
        let config = self.proxy_preparation.fetch_proxy_config().await?;

        transition_with_progress(machine, ActivationPhase::FilteringProxyNodes, progress)?;
        let nodes = self.proxy_preparation.load_proxy_nodes(&config).await?;

        transition_with_progress(machine, ActivationPhase::TestingProxyNodes, progress)?;
        let selection = self.proxy_preparation.select_proxy_node(&nodes).await?;
        transition_with_progress(machine, ActivationPhase::SelectingProxyNode, progress)?;

        transition_with_progress(machine, ActivationPhase::StartingLocalProxy, progress)?;
        let session = self
            .proxy_supervisor
            .start(&selection.selected.node)
            .await?;
        let endpoint = session.endpoint();
        *self.active_proxy.lock().await = Some(session);
        let settings = match LocalProxySettings::new(endpoint, default_proxy_bypass_domains()) {
            Ok(settings) => settings,
            Err(error) => {
                return self
                    .finish_without_network(machine, ActivationError::Platform(error), progress)
                    .await;
            }
        };

        transition_with_progress(machine, ActivationPhase::SavingNetworkState, progress)?;
        if let Err(error) = self.recovery.save_and_apply_proxy(&settings).await {
            let recovery_failed = matches!(error, NetworkSafetyError::ApplyAndRestoreFailed { .. });
            let primary = ActivationError::NetworkSafety(error);
            if recovery_failed {
                let _ = transition_with_progress(machine, ActivationPhase::Failed, progress);
                return Err(primary);
            }
            return self
                .finish_without_network(machine, primary, progress)
                .await;
        }

        let operation = self.run_networked_activation(machine, app, progress).await;
        self.finish_networked(machine, operation, progress).await
    }

    pub async fn network_recovery_status(&self) -> Result<NetworkRecoveryStatus, ActivationError> {
        let recovery_record_pending = self.recovery.has_pending()?;
        let local_proxy_active = self.active_proxy.lock().await.is_some();
        Ok(NetworkRecoveryStatus {
            pending: recovery_record_pending || local_proxy_active,
            local_proxy_active,
        })
    }

    pub async fn restore_network(&self) -> Result<NetworkRecoveryStatus, ActivationError> {
        let _activation_guard = self.activation_lock.lock().await;
        let restored = self.recovery.restore_pending_network().await?;
        let local_proxy_active = self.active_proxy.lock().await.is_some();
        if !restored && local_proxy_active {
            return Err(ActivationError::NetworkSafety(
                NetworkSafetyError::MissingRecoveryRecord,
            ));
        }
        if restored && local_proxy_active {
            self.stop_active_proxy().await?;
        }
        if restored {
            self.recovery.clear_pending()?;
        }
        self.network_recovery_status().await
    }

    async fn run_networked_activation<F>(
        &self,
        machine: &mut ActivationMachine,
        mut app: DesktopApp,
        progress: &F,
    ) -> Result<LocaleActivationResult, ActivationError>
    where
        F: Fn(ActivationPhase) + Send + Sync + ?Sized,
    {
        transition_with_progress(machine, ActivationPhase::WritingLocale, progress)?;
        let apply_paths = self.locale_paths.clone();
        let applied = tokio::task::spawn_blocking(move || apply_chinese_locale(&apply_paths))
            .await
            .map_err(|error| ActivationError::BackgroundTask(error.to_string()))??;

        transition_with_progress(machine, ActivationPhase::StoppingDesktopApp, progress)?;
        self.platform.stop_desktop_app(&app).await?;
        transition_with_progress(machine, ActivationPhase::LaunchingDesktopApp, progress)?;
        self.platform.launch_desktop_app(&app).await?;
        app.running = true;

        transition_with_progress(machine, ActivationPhase::Verifying, progress)?;
        let locale = self
            .effect_verifier
            .verify(&app, &self.locale_paths)
            .await?;
        if !locale.chinese_enabled {
            return Err(ActivationError::VerificationFailed);
        }

        Ok(LocaleActivationResult {
            app,
            locale,
            config_changed: applied.config_changed,
            global_state_changed: applied.global_state_changed,
            restarted: true,
        })
    }

    async fn finish_networked<T, F>(
        &self,
        machine: &mut ActivationMachine,
        operation: Result<T, ActivationError>,
        progress: &F,
    ) -> Result<T, ActivationError>
    where
        F: Fn(ActivationPhase) + Send + Sync + ?Sized,
    {
        match operation {
            Ok(value) => {
                transition_with_progress(machine, ActivationPhase::Succeeded, progress)?;
                Ok(value)
            }
            Err(error) => {
                transition_with_progress(machine, ActivationPhase::Failed, progress)?;
                Err(error)
            }
        }
    }

    async fn finish_without_network<T, F>(
        &self,
        machine: &mut ActivationMachine,
        primary: ActivationError,
        progress: &F,
    ) -> Result<T, ActivationError>
    where
        F: Fn(ActivationPhase) + Send + Sync + ?Sized,
    {
        transition_with_progress(machine, ActivationPhase::StoppingLocalProxy, progress)?;
        if let Err(cleanup_error) = self.stop_active_proxy().await {
            let _ = transition_with_progress(machine, ActivationPhase::Failed, progress);
            return Err(combine_operation_and_cleanup(Some(primary), cleanup_error));
        }
        transition_with_progress(machine, ActivationPhase::Failed, progress)?;
        Err(primary)
    }

    async fn stop_active_proxy(&self) -> Result<(), ActivationError> {
        let session = self
            .active_proxy
            .lock()
            .await
            .take()
            .ok_or(ActivationError::LocalProxySessionMissing)?;
        session.shutdown().await.map_err(ActivationError::from)
    }
}

fn transition_with_progress<F>(
    machine: &mut ActivationMachine,
    next: ActivationPhase,
    progress: &F,
) -> Result<(), ActivationError>
where
    F: Fn(ActivationPhase) + ?Sized,
{
    machine.transition(next)?;
    progress(next);
    Ok(())
}

fn default_proxy_bypass_domains() -> Vec<String> {
    ["localhost", "127.0.0.1", "::1", "<local>"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

async fn detect_apps<D>(discovery: D) -> Result<Vec<DesktopApp>, ActivationError>
where
    D: DesktopDiscovery + Send + 'static,
{
    tokio::task::spawn_blocking(move || discovery.detect())
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

fn combine_operation_and_cleanup(
    operation: Option<ActivationError>,
    cleanup: ActivationError,
) -> ActivationError {
    match operation {
        Some(operation) => ActivationError::OperationAndCleanupFailed {
            operation: Box::new(operation),
            cleanup: Box::new(cleanup),
        },
        None => cleanup,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProxyPreparationError;
    use async_trait::async_trait;
    use desktop_discovery::DiscoveryError;
    use locale_config::inspect_locale;
    use platform::{NetworkState, PlatformError};
    use proxy_core::{
        parse_subscription, DirectNodeSelectionReport, DirectVerifiedNode, LocalProxyError,
        LocalProxySession, ProxyNode, TargetBenchmark, VerifiedActivationNode,
    };
    use route_catalog::SubscriptionPayload;
    use shared_types::DesktopProduct;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::{Arc, Mutex as StdMutex};

    #[tokio::test]
    async fn completes_full_activation_and_waits_for_manual_network_restore() {
        let fixture = Fixture::new(false, false, false);

        let result = fixture
            .coordinator
            .activate(None)
            .await
            .expect("full activation");

        assert!(result.locale.chinese_enabled);
        assert!(fixture.recovery_path.exists());
        assert_eq!(
            fixture.log(),
            [
                "detect",
                "fetch_proxy_config",
                "load_proxy_nodes",
                "select_proxy_node",
                "proxy_start",
                "proxy_ready",
                "save_network_state",
                "apply_local_proxy",
                "stop_desktop_app",
                "launch_desktop_app",
                "verify_chinese_effect",
            ]
        );

        let status = fixture
            .coordinator
            .restore_network()
            .await
            .expect("manual network restore");

        assert!(!status.pending);
        assert!(!status.local_proxy_active);
        assert!(!fixture.recovery_path.exists());
        assert_order(&fixture.log(), "restore_network_state", "proxy_shutdown");
    }

    #[tokio::test]
    async fn reports_real_activation_phases_in_execution_order() {
        let fixture = Fixture::new(false, false, false);
        let phases = Arc::new(StdMutex::new(Vec::new()));
        let reported = phases.clone();

        fixture
            .coordinator
            .activate_with_progress(None, move |phase| {
                reported.lock().expect("phase log").push(phase);
            })
            .await
            .expect("full activation");

        assert_eq!(
            phases.lock().expect("phase log").as_slice(),
            [
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
            ]
        );
    }

    #[tokio::test]
    async fn reports_failed_phase_after_activation_error() {
        let fixture = Fixture::new(true, false, false);
        let phases = Arc::new(StdMutex::new(Vec::new()));
        let reported = phases.clone();

        fixture
            .coordinator
            .activate_with_progress(None, move |phase| {
                reported.lock().expect("phase log").push(phase);
            })
            .await
            .expect_err("verification must fail");

        assert_eq!(
            phases.lock().expect("phase log").last(),
            Some(&ActivationPhase::Failed)
        );
    }

    #[tokio::test]
    async fn verification_failure_waits_for_manual_network_restore() {
        let fixture = Fixture::new(true, false, false);

        let error = fixture
            .coordinator
            .activate(None)
            .await
            .expect_err("verification must fail");

        assert!(matches!(error, ActivationError::ChineseEffect(_)));
        assert!(fixture.recovery_path.exists());
        assert!(!fixture.log().contains(&"restore_network_state"));
        assert!(!fixture.log().contains(&"proxy_shutdown"));
    }

    #[tokio::test]
    async fn proxy_apply_failure_restores_network_and_closes_proxy() {
        let fixture = Fixture::new(false, true, false);

        let error = fixture
            .coordinator
            .activate(None)
            .await
            .expect_err("proxy apply must fail");

        assert!(matches!(
            error,
            ActivationError::NetworkSafety(NetworkSafetyError::ApplyFailed(_))
        ));
        assert_order(&fixture.log(), "restore_network_state", "proxy_shutdown");
        assert!(!fixture.recovery_path.exists());
    }

    #[tokio::test]
    async fn manual_restore_failure_keeps_recovery_record_and_active_proxy() {
        let fixture = Fixture::new(true, false, true);

        fixture
            .coordinator
            .activate(None)
            .await
            .expect_err("verification must fail");

        let error = fixture
            .coordinator
            .restore_network()
            .await
            .expect_err("manual restore must fail");

        assert!(matches!(
            error,
            ActivationError::NetworkSafety(NetworkSafetyError::Platform(_))
        ));
        let message = error.to_string();
        assert!(message.contains("mock restore failure"));
        assert!(fixture.recovery_path.exists());
        assert!(!fixture.log().contains(&"proxy_shutdown"));
    }

    #[tokio::test]
    async fn proxy_shutdown_failure_keeps_recovery_record_after_network_restore() {
        let fixture = Fixture::with_proxy_shutdown_failure(false, false, false, true);

        fixture
            .coordinator
            .activate(None)
            .await
            .expect("activation must succeed before manual restore");

        let error = fixture
            .coordinator
            .restore_network()
            .await
            .expect_err("proxy shutdown must fail");

        assert!(matches!(error, ActivationError::LocalProxy(_)));
        assert_order(&fixture.log(), "restore_network_state", "proxy_shutdown");
        assert!(fixture.recovery_path.exists());
    }

    #[tokio::test]
    async fn runtime_verifier_requires_a_zh_cn_renderer_process() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let locale_paths = LocalePaths::from_codex_home(directory.path().join(".codex"));
        apply_chinese_locale(&locale_paths).expect("Chinese locale fixture");
        let platform = MockPlatform {
            operations: Arc::new(StdMutex::new(Vec::new())),
            apply_fails: false,
            restore_fails: false,
            runtime_locale_matches: false,
        };
        let verifier =
            RuntimeChineseEffectVerifier::new(platform, Duration::ZERO, Duration::from_millis(1));

        let error = verifier
            .verify(&test_app(), &locale_paths)
            .await
            .expect_err("renderer locale must be required");

        assert!(matches!(error, ChineseEffectError::NotEffective));
    }

    #[tokio::test]
    async fn runtime_verifier_accepts_config_and_renderer_locale_together() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let locale_paths = LocalePaths::from_codex_home(directory.path().join(".codex"));
        apply_chinese_locale(&locale_paths).expect("Chinese locale fixture");
        let platform = MockPlatform {
            operations: Arc::new(StdMutex::new(Vec::new())),
            apply_fails: false,
            restore_fails: false,
            runtime_locale_matches: true,
        };
        let verifier = RuntimeChineseEffectVerifier::new(
            platform,
            Duration::from_millis(10),
            Duration::from_millis(1),
        );

        let status = verifier
            .verify(&test_app(), &locale_paths)
            .await
            .expect("runtime Chinese effect");

        assert!(status.chinese_enabled);
    }

    fn assert_order(log: &[&'static str], first: &'static str, second: &'static str) {
        let first_index = log
            .iter()
            .position(|entry| *entry == first)
            .expect("first operation");
        let second_index = log
            .iter()
            .position(|entry| *entry == second)
            .expect("second operation");
        assert!(first_index < second_index);
    }

    type TestCoordinator = ActivationCoordinator<
        MockDiscovery,
        MockPlatform,
        MockPreparation,
        MockVerifier,
        MockProxyEngine,
    >;

    struct Fixture {
        _directory: tempfile::TempDir,
        coordinator: TestCoordinator,
        recovery_path: std::path::PathBuf,
        operations: Arc<StdMutex<Vec<&'static str>>>,
    }

    impl Fixture {
        fn new(verification_fails: bool, apply_fails: bool, restore_fails: bool) -> Self {
            Self::with_proxy_shutdown_failure(verification_fails, apply_fails, restore_fails, false)
        }

        fn with_proxy_shutdown_failure(
            verification_fails: bool,
            apply_fails: bool,
            restore_fails: bool,
            proxy_shutdown_fails: bool,
        ) -> Self {
            let directory = tempfile::tempdir().expect("temporary directory");
            let locale_paths = LocalePaths::from_codex_home(directory.path().join(".codex"));
            let recovery_path = directory.path().join("recovery.json");
            let operations = Arc::new(StdMutex::new(Vec::new()));
            let platform = MockPlatform {
                operations: operations.clone(),
                apply_fails,
                restore_fails,
                runtime_locale_matches: true,
            };
            let coordinator = ActivationCoordinator::new(
                MockDiscovery {
                    operations: operations.clone(),
                },
                platform,
                MockPreparation {
                    operations: operations.clone(),
                },
                MockVerifier {
                    operations: operations.clone(),
                    fails: verification_fails,
                },
                MockProxyEngine {
                    operations: operations.clone(),
                    shutdown_fails: proxy_shutdown_fails,
                },
                Duration::from_secs(1),
                NetworkRecoveryStore::new(recovery_path.clone()),
                OperatingSystem::MacOs,
                locale_paths,
            );
            Self {
                _directory: directory,
                coordinator,
                recovery_path,
                operations,
            }
        }

        fn log(&self) -> Vec<&'static str> {
            self.operations.lock().expect("operation log").clone()
        }
    }

    #[derive(Clone)]
    struct MockDiscovery {
        operations: Arc<StdMutex<Vec<&'static str>>>,
    }

    impl DesktopDiscovery for MockDiscovery {
        fn detect(&self) -> Result<Vec<DesktopApp>, DiscoveryError> {
            record(&self.operations, "detect");
            Ok(vec![test_app()])
        }
    }

    struct MockPreparation {
        operations: Arc<StdMutex<Vec<&'static str>>>,
    }

    #[async_trait]
    impl ProxyPreparationService for MockPreparation {
        type PreparedSource = SubscriptionPayload;

        async fn fetch_proxy_config(&self) -> Result<SubscriptionPayload, ProxyPreparationError> {
            record(&self.operations, "fetch_proxy_config");
            Ok(SubscriptionPayload::new(Vec::new()))
        }

        async fn load_proxy_nodes(
            &self,
            _config: &SubscriptionPayload,
        ) -> Result<Vec<ProxyNode>, ProxyPreparationError> {
            record(&self.operations, "load_proxy_nodes");
            Ok(vec![test_node()])
        }

        async fn select_proxy_node(
            &self,
            nodes: &[ProxyNode],
        ) -> Result<DirectNodeSelectionReport, ProxyPreparationError> {
            record(&self.operations, "select_proxy_node");
            let verification = test_verification();
            let selected = DirectVerifiedNode {
                node: nodes[0].clone(),
                verification: verification.clone(),
            };
            Ok(DirectNodeSelectionReport {
                selected: selected.clone(),
                verified: vec![selected],
            })
        }
    }

    struct MockVerifier {
        operations: Arc<StdMutex<Vec<&'static str>>>,
        fails: bool,
    }

    #[async_trait]
    impl ChineseEffectVerifier for MockVerifier {
        async fn verify(
            &self,
            _app: &DesktopApp,
            locale_paths: &LocalePaths,
        ) -> Result<LocaleStatus, ChineseEffectError> {
            record(&self.operations, "verify_chinese_effect");
            if self.fails {
                return Err(ChineseEffectError::NotEffective);
            }
            inspect_locale(locale_paths)
                .map_err(|error| ChineseEffectError::Inspection(error.to_string()))
        }
    }

    #[derive(Clone)]
    struct MockPlatform {
        operations: Arc<StdMutex<Vec<&'static str>>>,
        apply_fails: bool,
        restore_fails: bool,
        runtime_locale_matches: bool,
    }

    #[async_trait]
    impl PlatformAdapter for MockPlatform {
        async fn stop_desktop_app(&self, _app: &DesktopApp) -> Result<(), PlatformError> {
            record(&self.operations, "stop_desktop_app");
            Ok(())
        }

        async fn launch_desktop_app(&self, _app: &DesktopApp) -> Result<(), PlatformError> {
            record(&self.operations, "launch_desktop_app");
            Ok(())
        }

        async fn desktop_app_uses_locale(
            &self,
            _app: &DesktopApp,
            _locale: &str,
        ) -> Result<bool, PlatformError> {
            Ok(self.runtime_locale_matches)
        }

        async fn save_network_state(&self) -> Result<NetworkState, PlatformError> {
            record(&self.operations, "save_network_state");
            Ok(NetworkState::from_serialized(
                r#"{"proxyEnabled":false}"#.to_owned(),
            ))
        }

        async fn apply_local_proxy(
            &self,
            _settings: &LocalProxySettings,
        ) -> Result<(), PlatformError> {
            record(&self.operations, "apply_local_proxy");
            if self.apply_fails {
                Err(PlatformError::Operation("mock apply failure".to_owned()))
            } else {
                Ok(())
            }
        }

        async fn restore_network_state(&self, _state: &NetworkState) -> Result<(), PlatformError> {
            record(&self.operations, "restore_network_state");
            if self.restore_fails {
                Err(PlatformError::Operation("mock restore failure".to_owned()))
            } else {
                Ok(())
            }
        }
    }

    struct MockProxyEngine {
        operations: Arc<StdMutex<Vec<&'static str>>>,
        shutdown_fails: bool,
    }

    #[async_trait]
    impl LocalProxyEngine for MockProxyEngine {
        async fn start(
            &self,
            _node: &ProxyNode,
        ) -> Result<Box<dyn LocalProxySession>, LocalProxyError> {
            record(&self.operations, "proxy_start");
            Ok(Box::new(MockProxySession {
                operations: self.operations.clone(),
                shutdown_fails: self.shutdown_fails,
            }))
        }
    }

    struct MockProxySession {
        operations: Arc<StdMutex<Vec<&'static str>>>,
        shutdown_fails: bool,
    }

    #[async_trait]
    impl LocalProxySession for MockProxySession {
        fn endpoint(&self) -> SocketAddr {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 17_892)
        }

        async fn wait_until_ready(&mut self, _timeout: Duration) -> Result<(), LocalProxyError> {
            record(&self.operations, "proxy_ready");
            Ok(())
        }

        async fn shutdown(&mut self) -> Result<(), LocalProxyError> {
            record(&self.operations, "proxy_shutdown");
            if self.shutdown_fails {
                Err(LocalProxyError::Shutdown(
                    "mock shutdown failure".to_owned(),
                ))
            } else {
                Ok(())
            }
        }

        fn abort(&mut self) {
            record(&self.operations, "proxy_abort");
        }
    }

    fn record(log: &Arc<StdMutex<Vec<&'static str>>>, operation: &'static str) {
        log.lock().expect("operation log").push(operation);
    }

    fn test_app() -> DesktopApp {
        DesktopApp {
            product: DesktopProduct::ChatGpt,
            display_name: "ChatGPT".to_owned(),
            install_path: "/Applications/ChatGPT.app".to_owned(),
            executable_path: "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT".to_owned(),
            bundle_identifier: Some("com.openai.codex".to_owned()),
            version: Some("1.0".to_owned()),
            running: true,
        }
    }

    fn test_node() -> ProxyNode {
        parse_subscription(b"vless://00000000-0000-0000-0000-000000000001@example.com:443#US-test")
            .expect("test subscription")
            .candidates
            .into_iter()
            .next()
            .expect("test node")
    }

    fn test_verification() -> VerifiedActivationNode {
        VerifiedActivationNode {
            name: "US test".to_owned(),
            protocol: "vless".to_owned(),
            region: proxy_core::NodeRegion::UnitedStates,
            country_code: "US".to_owned(),
            exit_success_count: 2,
            exit_attempt_count: 2,
            successful_targets: 3,
            target_count: 3,
            success_count: 6,
            attempt_count: 6,
            median_delay_ms: 100,
            jitter_ms: 5,
            score: 105,
            targets: vec![TargetBenchmark {
                name: "chatgpt".to_owned(),
                success_count: 2,
                attempt_count: 2,
                median_delay_ms: Some(100),
                jitter_ms: Some(5),
            }],
        }
    }
}
