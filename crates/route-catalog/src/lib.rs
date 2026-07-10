use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use proxy_core::{parse_subscription, NodeRegion, ParsedSubscription, ProxyNode, ProxyProtocol};
use reqwest::redirect::Policy;
use reqwest::{Certificate, Client};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shared_types::SubscriptionEndpoint;
use std::collections::HashMap;
use std::fmt;
use std::net::IpAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const DEFAULT_MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

#[derive(Clone)]
pub struct SecretSubscriptionUrl(Url);

impl SecretSubscriptionUrl {
    pub fn new(url: Url) -> Result<Self, SubscriptionFetchError> {
        if url.scheme() != "https"
            || url.host_str().is_none()
            || url.username() != ""
            || url.password().is_some()
        {
            return Err(SubscriptionFetchError::InvalidUrl);
        }
        Ok(Self(url))
    }

    fn expose(&self) -> &Url {
        &self.0
    }
}

impl fmt::Debug for SecretSubscriptionUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretSubscriptionUrl([REDACTED])")
    }
}

#[derive(Debug, Error)]
pub enum SubscriptionFetchError {
    #[error("订阅地址必须使用有效的 HTTPS URL")]
    InvalidUrl,
    #[error("订阅 HTTP 客户端初始化失败")]
    ClientBuild,
    #[error("订阅接口固定证书无效")]
    InvalidPinnedCertificate,
    #[error("订阅请求超时")]
    Timeout,
    #[error("无法连接订阅服务器")]
    Connect,
    #[error("订阅请求失败")]
    Request,
    #[error("订阅服务器返回状态码 {0}")]
    HttpStatus(u16),
    #[error("订阅响应超过大小限制 {limit_bytes} 字节")]
    BodyTooLarge { limit_bytes: usize },
    #[error("订阅内容无法解析")]
    InvalidSubscription,
}

#[derive(Clone)]
pub struct SubscriptionPayload(Arc<[u8]>);

impl SubscriptionPayload {
    #[must_use]
    pub fn new(body: Vec<u8>) -> Self {
        Self(body.into())
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl fmt::Debug for SubscriptionPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubscriptionPayload([REDACTED])")
    }
}

#[derive(Clone)]
pub struct SubscriptionClient {
    client: Client,
    url: SecretSubscriptionUrl,
    timeout: Duration,
    max_body_bytes: usize,
}

impl SubscriptionClient {
    pub fn new(url: SecretSubscriptionUrl) -> Result<Self, SubscriptionFetchError> {
        let client = Client::builder()
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    return attempt.stop();
                }
                if attempt.url().scheme() == "https" {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .map_err(|_| SubscriptionFetchError::ClientBuild)?;
        Ok(Self {
            client,
            url,
            timeout: DEFAULT_REQUEST_TIMEOUT,
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
        })
    }

    #[must_use]
    pub fn with_limits(mut self, timeout: Duration, max_body_bytes: usize) -> Self {
        self.timeout = timeout;
        self.max_body_bytes = max_body_bytes.max(1);
        self
    }

    pub async fn fetch(&self) -> Result<ParsedSubscription, SubscriptionFetchError> {
        let payload = self.fetch_payload().await?;
        parse_subscription(payload.as_bytes())
            .map_err(|_| SubscriptionFetchError::InvalidSubscription)
    }

    pub async fn fetch_payload(&self) -> Result<SubscriptionPayload, SubscriptionFetchError> {
        fetch_bounded(
            &self.client,
            self.url.expose().clone(),
            self.timeout,
            self.max_body_bytes,
        )
        .await
    }

    pub async fn refresh_catalog(
        &self,
        catalog: &RouteCatalog,
    ) -> Result<RouteCatalogSnapshot, SubscriptionFetchError> {
        let parsed = self.fetch().await?;
        catalog
            .replace(parsed, Utc::now())
            .map_err(|_| SubscriptionFetchError::InvalidSubscription)
    }
}

#[derive(Clone)]
pub struct RemoteSubscriptionClient {
    client: Client,
    endpoint: SubscriptionEndpoint,
    timeout: Duration,
    max_body_bytes: usize,
}

impl RemoteSubscriptionClient {
    pub fn new(endpoint: SubscriptionEndpoint) -> Result<Self, SubscriptionFetchError> {
        validate_remote_endpoint(&endpoint)?;
        let mut builder = Client::builder().redirect(Policy::none());
        if let Some(encoded_certificate) = endpoint.tls_certificate_der_base64.as_deref() {
            let certificate = STANDARD
                .decode(encoded_certificate.as_bytes())
                .map_err(|_| SubscriptionFetchError::InvalidPinnedCertificate)?;
            let certificate = Certificate::from_der(&certificate)
                .map_err(|_| SubscriptionFetchError::InvalidPinnedCertificate)?;
            builder = builder
                .tls_built_in_root_certs(false)
                .add_root_certificate(certificate);
        }
        let client = builder
            .build()
            .map_err(|_| SubscriptionFetchError::ClientBuild)?;
        Ok(Self {
            client,
            endpoint,
            timeout: DEFAULT_REQUEST_TIMEOUT,
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
        })
    }

    #[must_use]
    pub fn with_limits(mut self, timeout: Duration, max_body_bytes: usize) -> Self {
        self.timeout = timeout;
        self.max_body_bytes = max_body_bytes.max(1);
        self
    }

    pub async fn fetch_payload(&self) -> Result<SubscriptionPayload, SubscriptionFetchError> {
        fetch_bounded(
            &self.client,
            self.endpoint.url.clone(),
            self.timeout,
            self.max_body_bytes,
        )
        .await
    }

    pub async fn fetch(&self) -> Result<ParsedSubscription, SubscriptionFetchError> {
        let payload = self.fetch_payload().await?;
        parse_subscription(payload.as_bytes())
            .map_err(|_| SubscriptionFetchError::InvalidSubscription)
    }

    pub async fn refresh_catalog(
        &self,
        catalog: &RouteCatalog,
    ) -> Result<RouteCatalogSnapshot, SubscriptionFetchError> {
        let parsed = self.fetch().await?;
        catalog
            .replace(parsed, Utc::now())
            .map_err(|_| SubscriptionFetchError::InvalidSubscription)
    }
}

impl fmt::Debug for RemoteSubscriptionClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoteSubscriptionClient")
            .field("endpoint_id", &self.endpoint.id)
            .field("endpoint_url", &"[REDACTED]")
            .field("timeout", &self.timeout)
            .field("max_body_bytes", &self.max_body_bytes)
            .finish()
    }
}

impl fmt::Debug for SubscriptionClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubscriptionClient")
            .field("url", &self.url)
            .field("timeout", &self.timeout)
            .field("max_body_bytes", &self.max_body_bytes)
            .finish()
    }
}

fn classify_request_error(error: reqwest::Error) -> SubscriptionFetchError {
    if error.is_timeout() {
        SubscriptionFetchError::Timeout
    } else if error.is_connect() {
        SubscriptionFetchError::Connect
    } else {
        SubscriptionFetchError::Request
    }
}

async fn fetch_bounded(
    client: &Client,
    url: Url,
    timeout: Duration,
    max_body_bytes: usize,
) -> Result<SubscriptionPayload, SubscriptionFetchError> {
    let response = client
        .get(url)
        .timeout(timeout)
        .send()
        .await
        .map_err(classify_request_error)?;
    if !response.status().is_success() {
        return Err(SubscriptionFetchError::HttpStatus(
            response.status().as_u16(),
        ));
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(classify_request_error)?;
        if body.len().saturating_add(chunk.len()) > max_body_bytes {
            return Err(SubscriptionFetchError::BodyTooLarge {
                limit_bytes: max_body_bytes,
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(SubscriptionPayload::new(body))
}

fn validate_remote_endpoint(endpoint: &SubscriptionEndpoint) -> Result<(), SubscriptionFetchError> {
    if endpoint.url.scheme() != "https"
        || endpoint.url.host_str().is_none()
        || endpoint.url.username() != ""
        || endpoint.url.password().is_some()
        || endpoint.url.query().is_some()
        || endpoint.url.fragment().is_some()
        || endpoint.url.path() != "/v1/client/subscription"
    {
        return Err(SubscriptionFetchError::InvalidUrl);
    }
    let host_is_ip = endpoint
        .url
        .host_str()
        .is_some_and(|host| host.parse::<IpAddr>().is_ok());
    if host_is_ip && endpoint.tls_certificate_der_base64.is_none() {
        return Err(SubscriptionFetchError::InvalidPinnedCertificate);
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum RouteCatalogError {
    #[error("节点路由目录暂时不可用")]
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteMetadata {
    pub id: Uuid,
    pub name: String,
    pub protocol: ProxyProtocol,
    pub region: NodeRegion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteCatalogSnapshot {
    pub updated_at: DateTime<Utc>,
    pub routes: Vec<RouteMetadata>,
    pub rejected_count: usize,
}

#[derive(Clone)]
struct RouteEntry {
    metadata: RouteMetadata,
    node: ProxyNode,
}

impl fmt::Debug for RouteEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RouteEntry")
            .field("metadata", &self.metadata)
            .field("node", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Default)]
struct RouteCatalogState {
    updated_at: Option<DateTime<Utc>>,
    rejected_count: usize,
    routes: HashMap<Uuid, RouteEntry>,
}

#[derive(Clone, Default)]
pub struct RouteCatalog {
    state: Arc<RwLock<RouteCatalogState>>,
}

impl RouteCatalog {
    pub fn replace(
        &self,
        parsed: ParsedSubscription,
        updated_at: DateTime<Utc>,
    ) -> Result<RouteCatalogSnapshot, RouteCatalogError> {
        let rejected_count = parsed.rejected.len();
        let routes = parsed
            .candidates
            .into_iter()
            .map(|node| {
                let id = stable_route_id(&node);
                let metadata = RouteMetadata {
                    id,
                    name: node.name.clone(),
                    protocol: node.protocol,
                    region: node.region,
                };
                (id, RouteEntry { metadata, node })
            })
            .collect();
        let mut state = self
            .state
            .write()
            .map_err(|_| RouteCatalogError::Unavailable)?;
        state.updated_at = Some(updated_at);
        state.rejected_count = rejected_count;
        state.routes = routes;
        snapshot_from_state(&state)
    }

    pub fn snapshot(&self) -> Result<Option<RouteCatalogSnapshot>, RouteCatalogError> {
        let state = self
            .state
            .read()
            .map_err(|_| RouteCatalogError::Unavailable)?;
        if state.updated_at.is_none() {
            return Ok(None);
        }
        snapshot_from_state(&state).map(Some)
    }

    pub fn node(&self, id: Uuid) -> Result<Option<ProxyNode>, RouteCatalogError> {
        let state = self
            .state
            .read()
            .map_err(|_| RouteCatalogError::Unavailable)?;
        Ok(state.routes.get(&id).map(|entry| entry.node.clone()))
    }

    pub fn nodes(&self) -> Result<Vec<ProxyNode>, RouteCatalogError> {
        let state = self
            .state
            .read()
            .map_err(|_| RouteCatalogError::Unavailable)?;
        let mut nodes: Vec<_> = state
            .routes
            .values()
            .map(|entry| entry.node.clone())
            .collect();
        nodes.sort_by_key(|node| node.index);
        Ok(nodes)
    }

    pub fn contains(&self, id: Uuid) -> Result<bool, RouteCatalogError> {
        let state = self
            .state
            .read()
            .map_err(|_| RouteCatalogError::Unavailable)?;
        Ok(state.routes.contains_key(&id))
    }
}

impl fmt::Debug for RouteCatalog {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self
            .state
            .read()
            .map(|state| state.routes.len())
            .unwrap_or_default();
        formatter
            .debug_struct("RouteCatalog")
            .field("route_count", &count)
            .finish()
    }
}

fn snapshot_from_state(
    state: &RouteCatalogState,
) -> Result<RouteCatalogSnapshot, RouteCatalogError> {
    let updated_at = state.updated_at.ok_or(RouteCatalogError::Unavailable)?;
    let mut routes: Vec<_> = state
        .routes
        .values()
        .map(|entry| entry.metadata.clone())
        .collect();
    routes.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    Ok(RouteCatalogSnapshot {
        updated_at,
        routes,
        rejected_count: state.rejected_count,
    })
}

fn stable_route_id(node: &ProxyNode) -> Uuid {
    let digest = Sha256::digest(node.uri.expose().as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const US_NODE: &str =
        "vless://00000000-0000-0000-0000-000000000001@example.com:443?security=reality#US-test";
    const JP_NODE: &str = "hysteria2://password@example.net:8443?sni=example.net#JP-test";

    #[test]
    fn subscription_url_and_client_debug_are_redacted() {
        let secret = SecretSubscriptionUrl(
            Url::parse("https://example.com/subscribe?token=secret-value").expect("valid URL"),
        );
        let client = SubscriptionClient::new(secret.clone()).expect("subscription client");

        assert_eq!(format!("{secret:?}"), "SecretSubscriptionUrl([REDACTED])");
        let output = format!("{client:?}");
        assert!(!output.contains("secret-value"));
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn remote_ip_endpoint_requires_a_valid_pinned_certificate() {
        let endpoint = SubscriptionEndpoint {
            id: Uuid::new_v4(),
            url: Url::parse("https://192.0.2.10:18443/v1/client/subscription").expect("valid URL"),
            tls_certificate_der_base64: None,
        };

        assert!(matches!(
            RemoteSubscriptionClient::new(endpoint),
            Err(SubscriptionFetchError::InvalidPinnedCertificate)
        ));
    }

    #[tokio::test]
    async fn fetches_and_parses_bounded_subscription_without_exposing_url() {
        let payload = STANDARD.encode(format!("{US_NODE}\n{JP_NODE}\n"));
        let (url, server) = mock_http_response(payload.into_bytes()).await;
        let client = SubscriptionClient::new(SecretSubscriptionUrl(url))
            .expect("subscription client")
            .with_limits(Duration::from_secs(2), 1024 * 1024);

        let parsed = client.fetch().await.expect("parsed subscription");
        server.await.expect("mock server");

        assert_eq!(parsed.candidates.len(), 2);
    }

    #[tokio::test]
    async fn rejects_subscription_body_over_limit() {
        let (url, server) = mock_http_response(vec![b'a'; 128]).await;
        let client = SubscriptionClient::new(SecretSubscriptionUrl(url))
            .expect("subscription client")
            .with_limits(Duration::from_secs(2), 32);

        let error = client.fetch().await.expect_err("oversized subscription");
        server.await.expect("mock server");

        assert!(matches!(
            error,
            SubscriptionFetchError::BodyTooLarge { limit_bytes: 32 }
        ));
    }

    #[test]
    fn catalog_assigns_stable_ids_and_keeps_node_credentials_redacted() {
        let catalog = RouteCatalog::default();
        let parsed =
            parse_subscription(format!("{US_NODE}\n{JP_NODE}\n").as_bytes()).expect("subscription");
        let first = catalog
            .replace(parsed, Utc::now())
            .expect("replace catalog");
        let repeated = catalog
            .replace(
                parse_subscription(format!("{US_NODE}\n{JP_NODE}\n").as_bytes())
                    .expect("subscription"),
                Utc::now(),
            )
            .expect("replace catalog");

        assert_eq!(
            first
                .routes
                .iter()
                .map(|route| route.id)
                .collect::<Vec<_>>(),
            repeated
                .routes
                .iter()
                .map(|route| route.id)
                .collect::<Vec<_>>()
        );
        let node = catalog
            .node(first.routes[0].id)
            .expect("catalog access")
            .expect("route node");
        assert_eq!(format!("{:?}", node.uri), "SecretNodeUri([REDACTED])");
        assert!(!format!("{catalog:?}").contains("password"));
        assert_eq!(first.routes.len(), 2);
        assert_eq!(catalog.nodes().expect("catalog nodes").len(), 2);
    }

    async fn mock_http_response(body: Vec<u8>) -> (Url, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("mock listener");
        let address = listener.local_addr().expect("mock address");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("mock request");
            let mut request = vec![0_u8; 2048];
            let _ = stream.read(&mut request).await.expect("request bytes");
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .expect("response header");
            stream.write_all(&body).await.expect("response body");
        });
        (
            Url::parse(&format!("http://{address}/subscribe?token=secret")).expect("mock URL"),
            server,
        )
    }
}
