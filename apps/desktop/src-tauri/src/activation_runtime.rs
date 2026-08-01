#[cfg(not(feature = "e2e"))]
use activation_core::{default_activation_selection_options, StaticRouteProxyPreparationService};
use activation_core::{
    ActivationCoordinator, NetworkRecoveryService, RuntimeChineseEffectVerifier,
};
#[cfg(feature = "e2e")]
use activation_core::{ProxyPreparationError, ProxyPreparationService};
#[cfg(feature = "e2e")]
use async_trait::async_trait;
use desktop_discovery::SystemDesktopDiscovery;
#[cfg(any(not(feature = "e2e"), target_os = "windows"))]
use locale_config::default_locale_paths;
use locale_config::LocaleConfigError;
use platform::NativePlatformAdapter;
#[cfg(feature = "e2e")]
use proxy_core::{
    parse_subscription, DirectNodeSelectionReport, DirectVerifiedNode, ProxyNode, TargetBenchmark,
    VerifiedActivationNode,
};
use proxy_core::{EmbeddedNodeConnector, HttpConnectProxyEngine};
#[cfg(not(feature = "e2e"))]
use route_bundle::{decode_encryption_key, RouteBundleClient, RouteBundleError};
use std::path::PathBuf;
#[cfg(any(not(feature = "e2e"), target_os = "windows"))]
use std::time::Duration;
use thiserror::Error;
#[cfg(not(feature = "e2e"))]
use url::Url;

#[cfg(any(not(feature = "e2e"), target_os = "windows"))]
use activation_core::NetworkRecoveryStore;

const ROUTE_MANIFEST_URLS_ENV: &str = "DADAAPI_ROUTE_MANIFEST_URLS";
const ROUTE_PUBLIC_KEY_ENV: &str = "DADAAPI_ROUTE_PUBLIC_KEY_PEM";
const ROUTE_KEY_ENV: &str = "DADAAPI_ROUTE_KEY_B64";
const ROUTE_KEY_ID_ENV: &str = "DADAAPI_ROUTE_KEY_ID";
#[cfg(feature = "e2e")]
const E2E_LOCALE_HOME_ENV: &str = "DADA_E2E_LOCALE_HOME";
#[cfg(feature = "e2e")]
const E2E_LOOPBACK_SUBSCRIPTION: &str =
    "vless://00000000-0000-0000-0000-000000000001@127.0.0.1:1#E2E%20loopback";

#[cfg(not(feature = "e2e"))]
pub type DesktopActivationCoordinator = ActivationCoordinator<
    SystemDesktopDiscovery,
    NativePlatformAdapter,
    StaticRouteProxyPreparationService,
    RuntimeChineseEffectVerifier<NativePlatformAdapter>,
    HttpConnectProxyEngine<EmbeddedNodeConnector>,
>;
#[cfg(feature = "e2e")]
pub type DesktopActivationCoordinator = ActivationCoordinator<
    SystemDesktopDiscovery,
    NativePlatformAdapter,
    E2eProxyPreparationService,
    RuntimeChineseEffectVerifier<NativePlatformAdapter>,
    HttpConnectProxyEngine<EmbeddedNodeConnector>,
>;

pub struct DesktopActivationRuntime {
    pub coordinator: DesktopActivationCoordinator,
}

pub struct DesktopActivationState {
    runtime: Option<DesktopActivationRuntime>,
    fallback_recovery: NetworkRecoveryService<NativePlatformAdapter>,
}

impl DesktopActivationState {
    #[must_use]
    pub fn new(
        runtime: Option<DesktopActivationRuntime>,
        fallback_recovery: NetworkRecoveryService<NativePlatformAdapter>,
    ) -> Self {
        Self {
            runtime,
            fallback_recovery,
        }
    }

    #[must_use]
    pub fn is_available(&self) -> bool {
        self.runtime.is_some()
    }

    #[must_use]
    pub fn runtime(&self) -> Option<&DesktopActivationRuntime> {
        self.runtime.as_ref()
    }

    #[must_use]
    pub fn fallback_recovery(&self) -> &NetworkRecoveryService<NativePlatformAdapter> {
        &self.fallback_recovery
    }
}

#[derive(Debug, Error)]
pub enum ActivationRuntimeError {
    #[cfg(not(feature = "e2e"))]
    #[error("激活构建配置不完整，必须同时提供路由清单地址、验签公钥、解密密钥和密钥标识")]
    IncompleteBuildConfiguration,
    #[cfg(not(feature = "e2e"))]
    #[error("静态路由清单地址格式无效")]
    InvalidManifestUrl,
    #[cfg(not(feature = "e2e"))]
    #[error("内置节点检测地址格式无效")]
    InvalidProbeUrl,
    #[cfg(not(feature = "e2e"))]
    #[error(transparent)]
    RouteBundle(#[from] RouteBundleError),
    #[error(transparent)]
    Locale(#[from] LocaleConfigError),
    #[cfg(feature = "e2e")]
    #[error("E2E 中文配置目录未设置")]
    E2eLocaleHomeNotConfigured,
    #[cfg(feature = "e2e")]
    #[error("E2E 中文流程仅支持 Windows")]
    E2eWindowsOnly,
}

impl DesktopActivationRuntime {
    #[cfg(feature = "e2e")]
    pub fn from_build_environment(
        app_data_dir: PathBuf,
    ) -> Result<Option<Self>, ActivationRuntimeError> {
        Self::new_e2e(app_data_dir).map(Some)
    }

    #[cfg(not(feature = "e2e"))]
    pub fn from_build_environment(
        app_data_dir: PathBuf,
    ) -> Result<Option<Self>, ActivationRuntimeError> {
        let manifest_urls = option_env!("DADAAPI_ROUTE_MANIFEST_URLS")
            .or(option_env!("DADAAPI_ROUTE_MANIFEST_URL"));
        match (
            manifest_urls,
            option_env!("DADAAPI_ROUTE_PUBLIC_KEY_PEM"),
            option_env!("DADAAPI_ROUTE_KEY_B64"),
            option_env!("DADAAPI_ROUTE_KEY_ID"),
        ) {
            (None, None, None, None) => Ok(None),
            (Some(manifest_urls), Some(public_key_pem), Some(key_b64), Some(key_id)) => {
                Self::new_with_manifest_urls(
                    manifest_urls,
                    public_key_pem,
                    key_b64,
                    key_id,
                    app_data_dir,
                )
                .map(Some)
            }
            _ => Err(ActivationRuntimeError::IncompleteBuildConfiguration),
        }
    }

    #[cfg(not(feature = "e2e"))]
    pub fn new(
        manifest_url: &str,
        public_key_pem: &str,
        encryption_key_b64: &str,
        key_id: &str,
        app_data_dir: PathBuf,
    ) -> Result<Self, ActivationRuntimeError> {
        Self::new_with_manifest_urls(
            manifest_url,
            public_key_pem,
            encryption_key_b64,
            key_id,
            app_data_dir,
        )
    }

    #[cfg(not(feature = "e2e"))]
    pub fn new_with_manifest_urls(
        manifest_urls: &str,
        public_key_pem: &str,
        encryption_key_b64: &str,
        key_id: &str,
        app_data_dir: PathBuf,
    ) -> Result<Self, ActivationRuntimeError> {
        let manifest_urls = manifest_urls
            .split([',', ';', '\n'])
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| Url::parse(value).map_err(|_| ActivationRuntimeError::InvalidManifestUrl))
            .collect::<Result<Vec<_>, _>>()?;
        if manifest_urls.is_empty() {
            return Err(ActivationRuntimeError::InvalidManifestUrl);
        }
        let normalized_public_key = public_key_pem.replace("\\n", "\n");
        let route_client = RouteBundleClient::new_with_fallbacks(
            manifest_urls,
            &normalized_public_key,
            decode_encryption_key(encryption_key_b64)?,
            key_id.to_owned(),
            app_data_dir.join("route-bundles"),
        )?;
        let proxy_preparation = StaticRouteProxyPreparationService::new(
            route_client,
            default_activation_selection_options()
                .map_err(|_| ActivationRuntimeError::InvalidProbeUrl)?,
            app_data_dir.join("proxy-cache.json"),
        );
        let platform = NativePlatformAdapter;
        let coordinator = ActivationCoordinator::new(
            SystemDesktopDiscovery,
            platform,
            proxy_preparation,
            RuntimeChineseEffectVerifier::new(
                platform,
                Duration::from_secs(20),
                Duration::from_millis(250),
            ),
            HttpConnectProxyEngine::new(EmbeddedNodeConnector::default()),
            Duration::from_secs(3),
            NetworkRecoveryStore::new(app_data_dir.join("recovery.json")),
            super::current_operating_system(),
            default_locale_paths()?,
        );
        Ok(Self { coordinator })
    }

    #[cfg(all(feature = "e2e", target_os = "windows"))]
    fn new_e2e(app_data_dir: PathBuf) -> Result<Self, ActivationRuntimeError> {
        let locale_paths = default_locale_paths()?;
        let platform = NativePlatformAdapter;
        let coordinator = ActivationCoordinator::new(
            SystemDesktopDiscovery,
            platform,
            E2eProxyPreparationService,
            RuntimeChineseEffectVerifier::new(
                platform,
                Duration::from_secs(20),
                Duration::from_millis(250),
            ),
            HttpConnectProxyEngine::new(EmbeddedNodeConnector::default()),
            Duration::from_secs(3),
            NetworkRecoveryStore::new(app_data_dir.join("recovery.json")),
            super::current_operating_system(),
            locale_paths,
        );
        Ok(Self { coordinator })
    }

    #[cfg(all(feature = "e2e", not(target_os = "windows")))]
    fn new_e2e(_app_data_dir: PathBuf) -> Result<Self, ActivationRuntimeError> {
        Err(ActivationRuntimeError::E2eWindowsOnly)
    }
}

#[cfg(feature = "e2e")]
pub fn application_data_dir(default_path: PathBuf) -> Result<PathBuf, ActivationRuntimeError> {
    let _ = default_path;
    e2e_locale_home()
}

#[cfg(not(feature = "e2e"))]
pub fn application_data_dir(default_path: PathBuf) -> Result<PathBuf, ActivationRuntimeError> {
    Ok(default_path)
}

#[cfg(feature = "e2e")]
fn e2e_locale_home() -> Result<PathBuf, ActivationRuntimeError> {
    std::env::var_os(E2E_LOCALE_HOME_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or(ActivationRuntimeError::E2eLocaleHomeNotConfigured)
}

#[cfg(feature = "e2e")]
#[derive(Debug, Clone, Copy)]
pub struct E2eProxyPreparationService;

#[cfg(feature = "e2e")]
#[async_trait]
impl ProxyPreparationService for E2eProxyPreparationService {
    type PreparedSource = ();

    async fn fetch_proxy_config(&self) -> Result<Self::PreparedSource, ProxyPreparationError> {
        Ok(())
    }

    async fn load_proxy_nodes(
        &self,
        _source: &Self::PreparedSource,
    ) -> Result<Vec<ProxyNode>, ProxyPreparationError> {
        let parsed = parse_subscription(E2E_LOOPBACK_SUBSCRIPTION.as_bytes())
            .map_err(|_| ProxyPreparationError::SubscriptionUnavailable)?;
        if parsed.candidates.is_empty() {
            return Err(ProxyPreparationError::SubscriptionUnavailable);
        }
        Ok(parsed.candidates)
    }

    async fn select_proxy_node(
        &self,
        nodes: &[ProxyNode],
    ) -> Result<DirectNodeSelectionReport, ProxyPreparationError> {
        let node = nodes
            .first()
            .cloned()
            .ok_or(ProxyPreparationError::SubscriptionUnavailable)?;
        let selected = DirectVerifiedNode {
            verification: e2e_verification(&node),
            node,
        };
        Ok(DirectNodeSelectionReport {
            selected: selected.clone(),
            verified: vec![selected],
        })
    }
}

#[cfg(feature = "e2e")]
fn e2e_verification(node: &ProxyNode) -> VerifiedActivationNode {
    VerifiedActivationNode {
        name: node.name.clone(),
        protocol: "vless".to_owned(),
        region: node.region,
        country_code: "US".to_owned(),
        exit_success_count: 1,
        exit_attempt_count: 1,
        successful_targets: 3,
        target_count: 3,
        success_count: 3,
        attempt_count: 3,
        median_delay_ms: 1,
        jitter_ms: 0,
        score: 1,
        targets: vec![TargetBenchmark {
            name: "e2e-loopback".to_owned(),
            success_count: 1,
            attempt_count: 1,
            median_delay_ms: Some(1),
            jitter_ms: Some(0),
        }],
    }
}

#[must_use]
pub fn build_configuration_names() -> [&'static str; 4] {
    [
        ROUTE_MANIFEST_URLS_ENV,
        ROUTE_PUBLIC_KEY_ENV,
        ROUTE_KEY_ENV,
        ROUTE_KEY_ID_ENV,
    ]
}

#[cfg(all(test, not(feature = "e2e")))]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use ed25519_dalek::pkcs8::EncodePublicKey;
    use ed25519_dalek::SigningKey;

    #[test]
    fn exposes_dadaapi_build_configuration_names() {
        assert_eq!(
            build_configuration_names(),
            [
                "DADAAPI_ROUTE_MANIFEST_URLS",
                "DADAAPI_ROUTE_PUBLIC_KEY_PEM",
                "DADAAPI_ROUTE_KEY_B64",
                "DADAAPI_ROUTE_KEY_ID",
            ]
        );
    }

    #[test]
    fn accepts_static_github_route_configuration() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let runtime = DesktopActivationRuntime::new(
            "https://raw.githubusercontent.com/Tbthr/dadaapi-routes/main/public/manifest.json",
            &public_key_pem(),
            &encryption_key_b64(),
            "v1",
            directory.path().to_path_buf(),
        );

        assert!(runtime.is_ok());
    }

    #[test]
    fn accepts_gitee_primary_and_github_fallback_configuration() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let runtime = DesktopActivationRuntime::new_with_manifest_urls(
            "https://gitee.com/lyq_power/dadaapi-routes/raw/main/public/manifest.json,https://raw.githubusercontent.com/Tbthr/dadaapi-routes/main/public/manifest.json",
            &public_key_pem(),
            &encryption_key_b64(),
            "v1",
            directory.path().to_path_buf(),
        );

        assert!(runtime.is_ok());
    }

    #[test]
    fn rejects_route_urls_with_credentials_query_or_wrong_path() {
        for value in [
            "http://example.com/public/manifest.json",
            "https://user@example.com/public/manifest.json",
            "https://example.com/public/manifest.json?token=secret",
            "https://example.com/public/routes.enc",
        ] {
            assert!(matches!(
                DesktopActivationRuntime::new(
                    value,
                    &public_key_pem(),
                    &encryption_key_b64(),
                    "v1",
                    tempfile::tempdir()
                        .expect("temporary directory")
                        .path()
                        .to_path_buf(),
                ),
                Err(ActivationRuntimeError::RouteBundle(
                    RouteBundleError::InvalidManifestUrl
                ))
            ));
        }
    }

    #[test]
    fn accepts_escaped_newlines_in_build_public_key() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let escaped = public_key_pem().replace('\n', "\\n");

        let runtime = DesktopActivationRuntime::new(
            "https://example.com/public/manifest.json",
            &escaped,
            &encryption_key_b64(),
            "v1",
            directory.path().to_path_buf(),
        );

        assert!(runtime.is_ok());
    }

    #[test]
    fn activation_state_is_read_only_without_build_configuration() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let state = DesktopActivationState::new(
            None,
            NetworkRecoveryService::new(
                NativePlatformAdapter,
                NetworkRecoveryStore::new(directory.path().join("recovery.json")),
                shared_types::OperatingSystem::MacOs,
            ),
        );

        assert!(!state.is_available());
        assert!(state.runtime().is_none());
    }

    #[test]
    fn read_only_state_still_exposes_pending_network_recovery() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let recovery_store = NetworkRecoveryStore::new(directory.path().join("recovery.json"));
        recovery_store
            .save(
                shared_types::OperatingSystem::MacOs,
                &platform::NetworkState::from_serialized("safe-test-state".to_owned()),
            )
            .expect("save recovery record");
        let state = DesktopActivationState::new(
            None,
            NetworkRecoveryService::new(
                NativePlatformAdapter,
                recovery_store,
                shared_types::OperatingSystem::MacOs,
            ),
        );

        assert!(state
            .fallback_recovery()
            .has_pending()
            .expect("read pending recovery"));
    }

    fn public_key_pem() -> String {
        let document = SigningKey::from_bytes(&[19_u8; 32])
            .verifying_key()
            .to_public_key_der()
            .expect("public key DER");
        format!(
            "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
            STANDARD.encode(document.as_bytes())
        )
    }

    fn encryption_key_b64() -> String {
        STANDARD.encode([23_u8; 32])
    }
}

#[cfg(all(test, feature = "e2e"))]
mod e2e_tests {
    use super::*;

    #[tokio::test]
    async fn e2e_proxy_preparation_uses_only_the_deterministic_loopback_node() {
        let service = E2eProxyPreparationService;
        service
            .fetch_proxy_config()
            .await
            .expect("prepare E2E source");
        let nodes = service.load_proxy_nodes(&()).await.expect("load E2E node");
        let report = service
            .select_proxy_node(&nodes)
            .await
            .expect("select E2E node");

        assert_eq!(nodes.len(), 1);
        assert_eq!(report.selected.node.server, "127.0.0.1");
        assert_eq!(report.selected.node.port, 1);
    }
}
