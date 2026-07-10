use async_trait::async_trait;
use chrono::Utc;
use proxy_core::{
    save_proxy_selection_cache, DirectNodeSelectionReport, DirectNodeSelector,
    DirectSelectionError, DirectSelectionOptions, ProxyNode,
};
use remote_config::{
    load_verified_config_cache, save_signed_config_cache, ConfigClient, ConfigError, ConfigVerifier,
};
use route_bundle::{RouteBundleClient, RouteBundleError};
use route_catalog::{
    RemoteSubscriptionClient, RouteCatalog, RouteCatalogError, SubscriptionPayload,
};
use shared_types::ClientConfig;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;
use url::Url;

pub fn default_activation_selection_options() -> Result<DirectSelectionOptions, url::ParseError> {
    Ok(DirectSelectionOptions {
        exit_target_url: Url::parse("https://chatgpt.com/cdn-cgi/trace")?,
        probe_targets: vec![
            proxy_core::ActivationProbeTarget::new(
                "chatgpt-web",
                Url::parse("https://chatgpt.com/")?,
            ),
            proxy_core::ActivationProbeTarget::new(
                "openai-auth",
                Url::parse("https://auth.openai.com/")?,
            ),
            proxy_core::ActivationProbeTarget::new(
                "openai-api",
                Url::parse("https://api.openai.com/v1/models")?,
            )
            .accepting_statuses([200, 401]),
        ],
        minimum_target_coverage: 3,
        attempts: 3,
        timeout: Duration::from_secs(8),
        candidate_limit: 8,
    })
}

#[derive(Debug, Error)]
pub enum ProxyPreparationError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    RouteBundle(#[from] RouteBundleError),
    #[error("远程配置不可用，并且本地签名缓存也无法使用：远程={fetch}; 缓存={cache}")]
    ConfigAndCacheUnavailable {
        fetch: Box<ConfigError>,
        cache: Box<ConfigError>,
    },
    #[error("配置缓存后台任务失败：{0}")]
    CacheTask(String),
    #[error("远程配置没有提供可用的订阅接口")]
    MissingSubscriptionEndpoint,
    #[error("所有订阅接口都暂时不可用")]
    SubscriptionUnavailable,
    #[error(transparent)]
    Catalog(#[from] RouteCatalogError),
    #[error(transparent)]
    Selection(#[from] DirectSelectionError),
}

#[async_trait]
pub trait ProxyPreparationService: Send + Sync {
    type PreparedSource: Send + Sync;

    async fn fetch_proxy_config(&self) -> Result<Self::PreparedSource, ProxyPreparationError>;

    async fn load_proxy_nodes(
        &self,
        source: &Self::PreparedSource,
    ) -> Result<Vec<ProxyNode>, ProxyPreparationError>;

    async fn select_proxy_node(
        &self,
        nodes: &[ProxyNode],
    ) -> Result<DirectNodeSelectionReport, ProxyPreparationError>;
}

pub struct RemoteProxyPreparationService {
    config_client: ConfigClient,
    config_verifier: ConfigVerifier,
    catalog: RouteCatalog,
    selector: DirectNodeSelector,
    selection_options: DirectSelectionOptions,
    config_cache_path: PathBuf,
    proxy_cache_path: PathBuf,
}

impl RemoteProxyPreparationService {
    #[must_use]
    pub fn new(
        config_client: ConfigClient,
        config_verifier: ConfigVerifier,
        selection_options: DirectSelectionOptions,
        config_cache_path: PathBuf,
        proxy_cache_path: PathBuf,
    ) -> Self {
        Self {
            config_client,
            config_verifier,
            catalog: RouteCatalog::default(),
            selector: DirectNodeSelector::default(),
            selection_options,
            config_cache_path,
            proxy_cache_path,
        }
    }
}

#[async_trait]
impl ProxyPreparationService for RemoteProxyPreparationService {
    type PreparedSource = ClientConfig;

    async fn fetch_proxy_config(&self) -> Result<ClientConfig, ProxyPreparationError> {
        let config = match self.config_client.fetch().await {
            Ok(signed) => {
                let config = self.config_verifier.verify(&signed, Utc::now())?;
                let cache_path = self.config_cache_path.clone();
                let cache_result = tokio::task::spawn_blocking(move || {
                    save_signed_config_cache(&cache_path, &signed)
                })
                .await
                .map_err(|error| ProxyPreparationError::CacheTask(error.to_string()))?;
                if cache_result.is_err() {
                    tracing::warn!("could not update signed client config cache");
                }
                config
            }
            Err(fetch_error) if fetch_error.permits_cache_fallback() => {
                let cache_path = self.config_cache_path.clone();
                let verifier = self.config_verifier.clone();
                let cache_result = tokio::task::spawn_blocking(move || {
                    load_verified_config_cache(&cache_path, &verifier, Utc::now())
                })
                .await
                .map_err(|error| ProxyPreparationError::CacheTask(error.to_string()))?;
                match cache_result {
                    Ok(config) => config,
                    Err(cache_error) => {
                        return Err(ProxyPreparationError::ConfigAndCacheUnavailable {
                            fetch: Box::new(fetch_error),
                            cache: Box::new(cache_error),
                        });
                    }
                }
            }
            Err(error) => return Err(ProxyPreparationError::Config(error)),
        };
        if config.subscription_endpoints.is_empty() {
            return Err(ProxyPreparationError::MissingSubscriptionEndpoint);
        }
        Ok(config)
    }

    async fn load_proxy_nodes(
        &self,
        config: &ClientConfig,
    ) -> Result<Vec<ProxyNode>, ProxyPreparationError> {
        if config.subscription_endpoints.is_empty() {
            return Err(ProxyPreparationError::MissingSubscriptionEndpoint);
        }

        for endpoint in config.subscription_endpoints.iter().cloned() {
            let client = match RemoteSubscriptionClient::new(endpoint) {
                Ok(client) => client,
                Err(_) => continue,
            };
            let parsed = match client.fetch().await {
                Ok(parsed) => parsed,
                Err(_) => continue,
            };
            self.catalog.replace(parsed, Utc::now())?;
            return self.catalog.nodes().map_err(ProxyPreparationError::from);
        }

        Err(ProxyPreparationError::SubscriptionUnavailable)
    }

    async fn select_proxy_node(
        &self,
        nodes: &[ProxyNode],
    ) -> Result<DirectNodeSelectionReport, ProxyPreparationError> {
        let report = self
            .selector
            .select(nodes, &self.selection_options)
            .await
            .map_err(ProxyPreparationError::from)?;
        let cache_path = self.proxy_cache_path.clone();
        let metrics = report.metrics_report();
        let cache_result =
            tokio::task::spawn_blocking(move || save_proxy_selection_cache(&cache_path, &metrics))
                .await
                .map_err(|error| ProxyPreparationError::CacheTask(error.to_string()))?;
        if cache_result.is_err() {
            tracing::warn!("could not update proxy selection cache");
        }
        Ok(report)
    }
}

pub struct StaticRouteProxyPreparationService {
    route_client: RouteBundleClient,
    catalog: RouteCatalog,
    selector: DirectNodeSelector,
    selection_options: DirectSelectionOptions,
    proxy_cache_path: PathBuf,
}

impl StaticRouteProxyPreparationService {
    #[must_use]
    pub fn new(
        route_client: RouteBundleClient,
        selection_options: DirectSelectionOptions,
        proxy_cache_path: PathBuf,
    ) -> Self {
        Self {
            route_client,
            catalog: RouteCatalog::default(),
            selector: DirectNodeSelector::default(),
            selection_options,
            proxy_cache_path,
        }
    }
}

#[async_trait]
impl ProxyPreparationService for StaticRouteProxyPreparationService {
    type PreparedSource = SubscriptionPayload;

    async fn fetch_proxy_config(&self) -> Result<SubscriptionPayload, ProxyPreparationError> {
        self.route_client
            .fetch_payload()
            .await
            .map_err(ProxyPreparationError::from)
    }

    async fn load_proxy_nodes(
        &self,
        source: &SubscriptionPayload,
    ) -> Result<Vec<ProxyNode>, ProxyPreparationError> {
        let parsed = proxy_core::parse_subscription(source.as_bytes())
            .map_err(|_| ProxyPreparationError::SubscriptionUnavailable)?;
        self.catalog.replace(parsed, Utc::now())?;
        self.catalog.nodes().map_err(ProxyPreparationError::from)
    }

    async fn select_proxy_node(
        &self,
        nodes: &[ProxyNode],
    ) -> Result<DirectNodeSelectionReport, ProxyPreparationError> {
        let report = self
            .selector
            .select(nodes, &self.selection_options)
            .await
            .map_err(ProxyPreparationError::from)?;
        save_selection_report(&self.proxy_cache_path, &report).await?;
        Ok(report)
    }
}

async fn save_selection_report(
    proxy_cache_path: &std::path::Path,
    report: &DirectNodeSelectionReport,
) -> Result<(), ProxyPreparationError> {
    let cache_path = proxy_cache_path.to_path_buf();
    let metrics = report.metrics_report();
    let cache_result =
        tokio::task::spawn_blocking(move || save_proxy_selection_cache(&cache_path, &metrics))
            .await
            .map_err(|error| ProxyPreparationError::CacheTask(error.to_string()))?;
    if cache_result.is_err() {
        tracing::warn!("could not update proxy selection cache");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use chrono::Duration as ChronoDuration;
    use ed25519_dalek::{Signer, SigningKey};
    use shared_types::{ClientConfig, SignedClientConfig, SubscriptionEndpoint};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use url::Url;
    use uuid::Uuid;

    #[tokio::test]
    async fn falls_back_to_a_still_valid_signed_cache_on_connection_failure() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let signing_key = signing_key();
        let signed = signed_config(&signing_key);
        let config_cache_path = directory.path().join("client-config.json");
        save_signed_config_cache(&config_cache_path, &signed).expect("signed cache");
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("temporary listener");
        let address = listener.local_addr().expect("listener address");
        drop(listener);
        let service = service(
            Url::parse(&format!("http://{address}/v1/client/config")).expect("config URL"),
            signing_key.verifying_key(),
            config_cache_path,
            directory.path().join("proxy-cache.json"),
        );

        let config = service
            .fetch_proxy_config()
            .await
            .expect("verified cached config");

        assert_eq!(config.version, signed.payload.version);
    }

    #[tokio::test]
    async fn rejects_an_expired_cache_during_connection_failure() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let signing_key = signing_key();
        let mut payload = signed_config(&signing_key).payload;
        payload.expires_at = Some(Utc::now() - ChronoDuration::seconds(1));
        let expired = sign_payload(&signing_key, payload);
        let config_cache_path = directory.path().join("client-config.json");
        save_signed_config_cache(&config_cache_path, &expired).expect("expired signed cache");
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("temporary listener");
        let address = listener.local_addr().expect("listener address");
        drop(listener);
        let service = service(
            Url::parse(&format!("http://{address}/v1/client/config")).expect("config URL"),
            signing_key.verifying_key(),
            config_cache_path,
            directory.path().join("proxy-cache.json"),
        );

        let error = service
            .fetch_proxy_config()
            .await
            .expect_err("expired cache must fail");

        assert!(matches!(
            error,
            ProxyPreparationError::ConfigAndCacheUnavailable { cache, .. }
                if matches!(*cache, ConfigError::Expired)
        ));
    }

    #[tokio::test]
    async fn does_not_hide_an_invalid_remote_signature_with_cached_config() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let signing_key = signing_key();
        let cached = signed_config(&signing_key);
        let config_cache_path = directory.path().join("client-config.json");
        save_signed_config_cache(&config_cache_path, &cached).expect("signed cache");
        let mut tampered = cached.clone();
        tampered.payload.version = "tampered".to_owned();
        let (config_url, server) = mock_json_response(&tampered).await;
        let service = service(
            config_url,
            signing_key.verifying_key(),
            config_cache_path,
            directory.path().join("proxy-cache.json"),
        );

        let error = service
            .fetch_proxy_config()
            .await
            .expect_err("tampered remote config must fail");
        server.await.expect("mock server");

        assert!(matches!(
            error,
            ProxyPreparationError::Config(ConfigError::InvalidSignature)
        ));
    }

    #[tokio::test]
    async fn does_not_hide_a_malformed_remote_response_with_cached_config() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let signing_key = signing_key();
        let cached = signed_config(&signing_key);
        let config_cache_path = directory.path().join("client-config.json");
        save_signed_config_cache(&config_cache_path, &cached).expect("signed cache");
        let (config_url, server) = mock_response(b"{not-valid-json".to_vec()).await;
        let service = service(
            config_url,
            signing_key.verifying_key(),
            config_cache_path,
            directory.path().join("proxy-cache.json"),
        );

        let error = service
            .fetch_proxy_config()
            .await
            .expect_err("malformed remote response must fail");
        server.await.expect("mock server");

        assert!(matches!(
            error,
            ProxyPreparationError::Config(ConfigError::Request(_))
        ));
    }

    #[tokio::test]
    async fn stores_a_successfully_verified_remote_config() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let signing_key = signing_key();
        let signed = signed_config(&signing_key);
        let (config_url, server) = mock_json_response(&signed).await;
        let config_cache_path = directory.path().join("client-config.json");
        let verifier = ConfigVerifier::new(signing_key.verifying_key());
        let service = RemoteProxyPreparationService::new(
            ConfigClient::new(config_url).with_timeout(Duration::from_secs(1)),
            verifier.clone(),
            selection_options(),
            config_cache_path.clone(),
            directory.path().join("proxy-cache.json"),
        );

        let config = service.fetch_proxy_config().await.expect("remote config");
        server.await.expect("mock server");
        let cached = load_verified_config_cache(&config_cache_path, &verifier, Utc::now())
            .expect("verified cache");

        assert_eq!(config.version, signed.payload.version);
        assert_eq!(cached.version, signed.payload.version);
    }

    fn service(
        config_url: Url,
        verifying_key: ed25519_dalek::VerifyingKey,
        config_cache_path: PathBuf,
        proxy_cache_path: PathBuf,
    ) -> RemoteProxyPreparationService {
        RemoteProxyPreparationService::new(
            ConfigClient::new(config_url).with_timeout(Duration::from_millis(250)),
            ConfigVerifier::new(verifying_key),
            selection_options(),
            config_cache_path,
            proxy_cache_path,
        )
    }

    fn selection_options() -> DirectSelectionOptions {
        let mut options = default_activation_selection_options().expect("selection options");
        options.probe_targets.truncate(1);
        options.minimum_target_coverage = 1;
        options.attempts = 1;
        options.timeout = Duration::from_secs(1);
        options.candidate_limit = 1;
        options
    }

    fn signed_config(signing_key: &SigningKey) -> SignedClientConfig {
        let now = Utc::now();
        let payload = ClientConfig {
            version: "test-config".to_owned(),
            generated_at: now,
            expires_at: Some(now + ChronoDuration::minutes(10)),
            subscription_endpoints: vec![SubscriptionEndpoint {
                id: Uuid::new_v4(),
                url: Url::parse("https://subscription.example.com/v1/client/subscription")
                    .expect("subscription endpoint"),
                tls_certificate_der_base64: None,
            }],
        };
        sign_payload(signing_key, payload)
    }

    fn sign_payload(signing_key: &SigningKey, payload: ClientConfig) -> SignedClientConfig {
        let bytes = serde_json::to_vec(&payload).expect("serialize signed payload");
        SignedClientConfig {
            payload,
            signature: STANDARD.encode(signing_key.sign(&bytes).to_bytes()),
        }
    }

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[11_u8; 32])
    }

    async fn mock_json_response<T: serde::Serialize>(
        body: &T,
    ) -> (Url, tokio::task::JoinHandle<()>) {
        let body = serde_json::to_vec(body).expect("JSON body");
        mock_response(body).await
    }

    async fn mock_response(body: Vec<u8>) -> (Url, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("mock listener");
        let address = listener.local_addr().expect("mock address");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("mock request");
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).await.expect("request bytes");
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .expect("response headers");
            stream.write_all(&body).await.expect("response body");
        });
        (
            Url::parse(&format!("http://{address}/v1/client/config")).expect("mock config URL"),
            server,
        )
    }
}
