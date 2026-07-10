use activation_core::{
    default_activation_selection_options, ProxyPreparationError, ProxyPreparationService,
    RemoteProxyPreparationService,
};
use axum_server::tls_rustls::RustlsConfig;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use config_server::{ConfigPublisher, PublisherError, PublisherSettings};
use ed25519_dalek::pkcs8::EncodePublicKey;
use ed25519_dalek::SigningKey;
use remote_config::{ConfigClient, ConfigError, ConfigVerifier};
use route_catalog::{
    RemoteSubscriptionClient, SecretSubscriptionUrl, SubscriptionClient, SubscriptionFetchError,
    SubscriptionPayload,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener as StdTcpListener};
use std::path::Path;
use std::time::Duration;
use subscription_relay::RelayState;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Instant};
use url::Url;
use uuid::Uuid;

const LOCAL_HOST: &str = "127.0.0.1";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Error)]
pub enum LocalE2eError {
    #[error("本地联调随机密钥生成失败")]
    Random,
    #[error("本地联调 TLS 证书生成失败")]
    Certificate,
    #[error("本地联调 TLS 服务初始化失败")]
    Tls,
    #[error("本地联调端口绑定失败：{0}")]
    Bind(#[from] std::io::Error),
    #[error(transparent)]
    Subscription(#[from] SubscriptionFetchError),
    #[error(transparent)]
    Publisher(#[from] PublisherError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("本地联调地址生成失败")]
    InvalidLocalUrl,
    #[error("本地联调签名公钥生成失败")]
    PublicKey,
    #[error("本地联调链路启动后未能通过自检")]
    StartupVerification,
    #[error("订阅中没有可解析的代理节点")]
    NoParsedNodes,
    #[error("内置节点检测地址格式无效")]
    InvalidProbeUrl,
    #[error(transparent)]
    Preparation(#[from] ProxyPreparationError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalE2eSummary {
    pub config_url: Url,
    pub public_key_pem: String,
    pub subscription_endpoint_id: Uuid,
    pub parsed_node_count: usize,
    pub rejected_node_count: usize,
    pub supported_node_count: usize,
    pub unsupported_node_count: usize,
    pub hysteria2_node_count: usize,
    pub vless_node_count: usize,
    pub hysteria2_obfuscated_count: usize,
    pub hysteria2_port_hopping_count: usize,
    pub hysteria2_insecure_count: usize,
}

struct StackVerification {
    parsed_node_count: usize,
    rejected_node_count: usize,
    supported_node_count: usize,
    unsupported_node_count: usize,
    hysteria2_node_count: usize,
    vless_node_count: usize,
    hysteria2_obfuscated_count: usize,
    hysteria2_port_hopping_count: usize,
    hysteria2_insecure_count: usize,
}

pub struct LocalE2eStack {
    summary: LocalE2eSummary,
    relay_task: JoinHandle<()>,
    config_task: JoinHandle<()>,
}

impl LocalE2eStack {
    pub async fn start_from_upstream(
        upstream: SecretSubscriptionUrl,
    ) -> Result<Self, LocalE2eError> {
        let client = SubscriptionClient::new(upstream)?;
        let payload = client.fetch_payload().await?;
        Self::start_with_payload(payload).await
    }

    pub async fn start_with_payload(payload: SubscriptionPayload) -> Result<Self, LocalE2eError> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let certified = rcgen::generate_simple_self_signed(vec![LOCAL_HOST.to_owned()])
            .map_err(|_| LocalE2eError::Certificate)?;
        let certificate_der = certified.cert.der().as_ref().to_vec();
        let tls = RustlsConfig::from_der(
            vec![certificate_der.clone()],
            certified.key_pair.serialize_der(),
        )
        .await
        .map_err(|_| LocalE2eError::Tls)?;

        let relay_state = RelayState::default();
        relay_state
            .replace(payload)
            .map_err(|_| LocalE2eError::StartupVerification)?;
        let relay_listener = StdTcpListener::bind((LOCAL_HOST, 0))?;
        relay_listener.set_nonblocking(true)?;
        let relay_address = relay_listener.local_addr()?;
        let relay_task = tokio::spawn(async move {
            let result = axum_server::from_tcp_rustls(relay_listener, tls).map(|server| {
                server.serve(subscription_relay::router(relay_state).into_make_service())
            });
            if let Ok(server) = result {
                let _ = server.await;
            }
        });

        let mut signing_key_bytes = [0_u8; 32];
        getrandom::fill(&mut signing_key_bytes).map_err(|_| LocalE2eError::Random)?;
        let signing_key = SigningKey::from_bytes(&signing_key_bytes);
        signing_key_bytes.fill(0);
        let public_key_pem = public_key_pem(&signing_key)?;
        let endpoint_id = Uuid::new_v4();
        let subscription_endpoint_url =
            local_url("https", relay_address, "/v1/client/subscription")?;
        let publisher_settings = PublisherSettings::new(
            "local-e2e".to_owned(),
            endpoint_id,
            subscription_endpoint_url,
            Some(STANDARD.encode(&certificate_der)),
        )?;
        let publisher = ConfigPublisher::new(signing_key, publisher_settings);

        let config_listener = TcpListener::bind((LOCAL_HOST, 0)).await?;
        let config_address = config_listener.local_addr()?;
        let config_task = tokio::spawn(async move {
            let _ = axum::serve(config_listener, config_server::router(Some(publisher))).await;
        });
        let config_url = local_url("http", config_address, "/v1/client/config")?;

        let verification = verify_started_stack(&config_url, &public_key_pem).await;
        let verification = match verification {
            Ok(verification) => verification,
            Err(error) => {
                relay_task.abort();
                config_task.abort();
                return Err(error);
            }
        };
        if verification.parsed_node_count == 0 {
            relay_task.abort();
            config_task.abort();
            return Err(LocalE2eError::NoParsedNodes);
        }

        Ok(Self {
            summary: LocalE2eSummary {
                config_url,
                public_key_pem,
                subscription_endpoint_id: endpoint_id,
                parsed_node_count: verification.parsed_node_count,
                rejected_node_count: verification.rejected_node_count,
                supported_node_count: verification.supported_node_count,
                unsupported_node_count: verification.unsupported_node_count,
                hysteria2_node_count: verification.hysteria2_node_count,
                vless_node_count: verification.vless_node_count,
                hysteria2_obfuscated_count: verification.hysteria2_obfuscated_count,
                hysteria2_port_hopping_count: verification.hysteria2_port_hopping_count,
                hysteria2_insecure_count: verification.hysteria2_insecure_count,
            },
            relay_task,
            config_task,
        })
    }

    #[must_use]
    pub fn summary(&self) -> &LocalE2eSummary {
        &self.summary
    }

    pub async fn select_best_node(
        &self,
        cache_directory: &Path,
        quick: bool,
        candidate_limit: Option<usize>,
    ) -> Result<proxy_core::VerifiedActivationNode, LocalE2eError> {
        let verifier = ConfigVerifier::from_public_key_pem(&self.summary.public_key_pem)?;
        let mut options =
            default_activation_selection_options().map_err(|_| LocalE2eError::InvalidProbeUrl)?;
        if quick {
            options.minimum_target_coverage = 2;
            options.attempts = 2;
            options.timeout = Duration::from_secs(5);
            options.candidate_limit = 4;
        }
        if let Some(candidate_limit) = candidate_limit {
            options.candidate_limit = candidate_limit.max(1);
        }
        let preparation = RemoteProxyPreparationService::new(
            ConfigClient::new(self.summary.config_url.clone()),
            verifier,
            options,
            cache_directory.join("client-config.json"),
            cache_directory.join("proxy-cache.json"),
        );
        let config = preparation.fetch_proxy_config().await?;
        let nodes = preparation.load_proxy_nodes(&config).await?;
        let report = preparation.select_proxy_node(&nodes).await?;
        Ok(report.selected.verification)
    }
}

impl Drop for LocalE2eStack {
    fn drop(&mut self) {
        self.relay_task.abort();
        self.config_task.abort();
    }
}

async fn verify_started_stack(
    config_url: &Url,
    public_key_pem: &str,
) -> Result<StackVerification, LocalE2eError> {
    let verifier = ConfigVerifier::from_public_key_pem(public_key_pem)?;
    let client = ConfigClient::new(config_url.clone()).with_timeout(Duration::from_millis(500));
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    loop {
        if let Ok(config) = client.fetch_verified(&verifier).await {
            if let Some(endpoint) = config.subscription_endpoints.into_iter().next() {
                if let Ok(remote) = RemoteSubscriptionClient::new(endpoint) {
                    if let Ok(parsed) = remote.fetch().await {
                        let connector = proxy_core::EmbeddedNodeConnector::default();
                        let supported_node_count = parsed
                            .candidates
                            .iter()
                            .filter(|node| connector.supports_node(node))
                            .count();
                        let hysteria2_node_count = parsed
                            .candidates
                            .iter()
                            .filter(|node| node.protocol == proxy_core::ProxyProtocol::Hysteria2)
                            .count();
                        let vless_node_count = parsed.candidates.len() - hysteria2_node_count;
                        let hysteria_configs: Vec<_> = parsed
                            .candidates
                            .iter()
                            .filter_map(|node| {
                                let parsed = proxy_core::parse_node_connection_config(node).ok()?;
                                match parsed {
                                    proxy_core::NodeConnectionConfig::Hysteria2(config) => {
                                        Some(config)
                                    }
                                    proxy_core::NodeConnectionConfig::Vless(_) => None,
                                }
                            })
                            .collect();
                        return Ok(StackVerification {
                            parsed_node_count: parsed.candidates.len(),
                            rejected_node_count: parsed.rejected.len(),
                            supported_node_count,
                            unsupported_node_count: parsed
                                .candidates
                                .len()
                                .saturating_sub(supported_node_count),
                            hysteria2_node_count,
                            vless_node_count,
                            hysteria2_obfuscated_count: hysteria_configs
                                .iter()
                                .filter(|config| config.obfuscation.is_some())
                                .count(),
                            hysteria2_port_hopping_count: hysteria_configs
                                .iter()
                                .filter(|config| config.port_hopping.is_some())
                                .count(),
                            hysteria2_insecure_count: hysteria_configs
                                .iter()
                                .filter(|config| config.allow_insecure)
                                .count(),
                        });
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(LocalE2eError::StartupVerification);
        }
        sleep(Duration::from_millis(50)).await;
    }
}

fn local_url(scheme: &str, address: SocketAddr, path: &str) -> Result<Url, LocalE2eError> {
    if address.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) {
        return Err(LocalE2eError::InvalidLocalUrl);
    }
    Url::parse(&format!("{scheme}://{address}{path}")).map_err(|_| LocalE2eError::InvalidLocalUrl)
}

fn public_key_pem(signing_key: &SigningKey) -> Result<String, LocalE2eError> {
    let document = signing_key
        .verifying_key()
        .to_public_key_der()
        .map_err(|_| LocalE2eError::PublicKey)?;
    Ok(format!(
        "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
        STANDARD.encode(document.as_bytes())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn starts_and_verifies_signed_config_and_pinned_subscription_chain() {
        let payload = SubscriptionPayload::new(
            b"vless://00000000-0000-0000-0000-000000000001@example.com:443#US-test".to_vec(),
        );

        let stack = LocalE2eStack::start_with_payload(payload)
            .await
            .expect("local E2E stack");

        assert_eq!(stack.summary().parsed_node_count, 1);
        assert_eq!(stack.summary().rejected_node_count, 0);
        assert_eq!(stack.summary().supported_node_count, 1);
        assert_eq!(stack.summary().unsupported_node_count, 0);
        assert_eq!(stack.summary().hysteria2_obfuscated_count, 0);
        assert_eq!(stack.summary().config_url.scheme(), "http");
        assert!(stack.summary().public_key_pem.contains("BEGIN PUBLIC KEY"));
    }
}
