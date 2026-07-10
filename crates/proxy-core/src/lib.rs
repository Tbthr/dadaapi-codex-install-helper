use async_trait::async_trait;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use percent_encoding::percent_decode_str;
use reqwest::{Client, Proxy};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use url::Url;

mod direct_selection;
mod embedded_connector;
mod http_connect;
mod hysteria2_connector;
mod node_config;
mod vless_connector;

pub use direct_selection::{
    ActivationNodeProbe, DirectNodeSelectionReport, DirectNodeSelector, DirectSelectionError,
    DirectSelectionOptions, DirectVerifiedNode, HttpActivationNodeProbe,
};
pub use embedded_connector::EmbeddedNodeConnector;
pub use http_connect::{
    BoxedNodeStream, ConnectTarget, HttpConnectProxyEngine, NodeConnector, NodeStream,
};
pub use hysteria2_connector::Hysteria2NodeConnector;
pub use node_config::{
    parse_node_connection_config, Hysteria2NodeConfig, Hysteria2Obfuscation, NodeConfigError,
    NodeConnectionConfig, SecretUuid, SecretValue, VlessNodeConfig, VlessSecurity, VlessTransport,
};
pub use vless_connector::VlessNodeConnector;

const PROXY_CACHE_SCHEMA_VERSION: u32 = 1;
const FALLBACK_NODE_LIMIT: usize = 2;
const MISSING_TARGET_PENALTY: u128 = 1_000_000_000;
const FAILED_ATTEMPT_PENALTY: u128 = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyProtocol {
    Vless,
    Hysteria2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeRegion {
    HongKong,
    MainlandChina,
    Taiwan,
    Japan,
    UnitedStates,
    Singapore,
    Netherlands,
    France,
    Germany,
    UnitedKingdom,
    Brazil,
    Other,
}

impl NodeRegion {
    #[must_use]
    pub fn excluded_for_activation(self) -> bool {
        matches!(self, Self::HongKong | Self::MainlandChina)
    }
}

#[derive(Clone)]
pub struct SecretNodeUri(String);

impl SecretNodeUri {
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretNodeUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretNodeUri([REDACTED])")
    }
}

#[derive(Debug, Clone)]
pub struct ProxyNode {
    pub index: usize,
    pub name: String,
    pub protocol: ProxyProtocol,
    pub region: NodeRegion,
    pub server: String,
    pub port: u16,
    pub uri: SecretNodeUri,
}

impl ProxyNode {
    #[must_use]
    pub fn activation_candidate(&self) -> bool {
        !self.region.excluded_for_activation() && !is_metadata_name(&self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionReason {
    Metadata,
    ExcludedRegion,
    UnsupportedProtocol,
    InvalidNode,
}

#[derive(Debug, Clone)]
pub struct RejectedNode {
    pub index: usize,
    pub name: Option<String>,
    pub reason: RejectionReason,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedSubscription {
    pub candidates: Vec<ProxyNode>,
    pub rejected: Vec<RejectedNode>,
}

#[derive(Debug, Error)]
pub enum SubscriptionError {
    #[error("订阅内容为空")]
    Empty,
    #[error("订阅内容既不是节点列表，也不是有效的 Base64 编码")]
    InvalidEncoding,
    #[error("订阅解码后不是有效的 UTF-8 文本")]
    InvalidUtf8,
}

#[derive(Debug, Error)]
pub enum ProxyProbeError {
    #[error("代理检测请求失败：{0}")]
    Request(#[from] reqwest::Error),
    #[error("代理检测返回错误状态：{0}")]
    HttpStatus(reqwest::StatusCode),
    #[error("本地代理地址无效")]
    InvalidProxyUrl,
    #[error("代理出口检测没有返回国家或地区代码")]
    ExitCountryMissing,
}

#[derive(Debug, Error)]
pub enum ProxyCacheError {
    #[error("代理缓存路径无效")]
    InvalidPath,
    #[error("代理缓存文件操作失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("代理缓存格式无效：{0}")]
    Json(#[from] serde_json::Error),
    #[error("不支持的代理缓存版本：{0}")]
    UnsupportedVersion(u32),
    #[error("代理缓存缺少已选节点")]
    MissingSelectedNode,
}

#[derive(Debug, Error)]
pub enum LocalProxyError {
    #[error("本地代理启动失败：{0}")]
    Start(String),
    #[error("本地代理没有监听有效的回环地址")]
    InvalidEndpoint,
    #[error("本地代理就绪检查失败：{0}")]
    Readiness(String),
    #[error("本地代理关闭失败：{0}")]
    Shutdown(String),
    #[error("本地代理启动失败，并且清理也失败：启动={start}; 清理={cleanup}")]
    StartAndCleanupFailed { start: String, cleanup: String },
}

#[async_trait]
pub trait LocalProxySession: Send {
    fn endpoint(&self) -> SocketAddr;
    async fn wait_until_ready(&mut self, timeout: Duration) -> Result<(), LocalProxyError>;
    async fn shutdown(&mut self) -> Result<(), LocalProxyError>;
    fn abort(&mut self);
}

#[async_trait]
pub trait LocalProxyEngine: Send + Sync {
    async fn start(&self, node: &ProxyNode) -> Result<Box<dyn LocalProxySession>, LocalProxyError>;
}

#[derive(Debug, Clone, Copy)]
pub struct LocalProxySupervisor<E> {
    engine: E,
    readiness_timeout: Duration,
}

impl<E> LocalProxySupervisor<E>
where
    E: LocalProxyEngine,
{
    #[must_use]
    pub fn new(engine: E, readiness_timeout: Duration) -> Self {
        Self {
            engine,
            readiness_timeout,
        }
    }

    pub async fn start(&self, node: &ProxyNode) -> Result<SupervisedProxySession, LocalProxyError> {
        let mut session = self.engine.start(node).await?;
        let endpoint = session.endpoint();
        if !endpoint.ip().is_loopback() || endpoint.port() == 0 {
            session.abort();
            return Err(LocalProxyError::InvalidEndpoint);
        }
        if let Err(start_error) = session.wait_until_ready(self.readiness_timeout).await {
            return match session.shutdown().await {
                Ok(()) => Err(start_error),
                Err(cleanup_error) => {
                    session.abort();
                    Err(LocalProxyError::StartAndCleanupFailed {
                        start: start_error.to_string(),
                        cleanup: cleanup_error.to_string(),
                    })
                }
            };
        }
        Ok(SupervisedProxySession {
            endpoint,
            session: Some(session),
        })
    }
}

pub struct SupervisedProxySession {
    endpoint: SocketAddr,
    session: Option<Box<dyn LocalProxySession>>,
}

impl SupervisedProxySession {
    #[must_use]
    pub fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }

    pub async fn shutdown(mut self) -> Result<(), LocalProxyError> {
        let Some(mut session) = self.session.take() else {
            return Ok(());
        };
        if let Err(error) = session.shutdown().await {
            session.abort();
            return Err(error);
        }
        Ok(())
    }
}

impl fmt::Debug for SupervisedProxySession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SupervisedProxySession")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl Drop for SupervisedProxySession {
    fn drop(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.abort();
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeBenchmark {
    pub name: String,
    pub protocol: String,
    pub region: NodeRegion,
    pub success_count: usize,
    pub attempt_count: usize,
    pub median_delay_ms: Option<u32>,
    pub jitter_ms: Option<u32>,
    pub score: u64,
}

#[derive(Debug, Clone)]
pub struct ProxyExitProbe {
    pub country_code: String,
    pub latency_ms: u128,
}

#[derive(Debug, Clone)]
pub struct ActivationProbeTarget {
    pub name: String,
    pub url: Url,
    pub accepted_statuses: Vec<u16>,
}

impl ActivationProbeTarget {
    #[must_use]
    pub fn new(name: impl Into<String>, url: Url) -> Self {
        Self {
            name: name.into(),
            url,
            accepted_statuses: Vec::new(),
        }
    }

    #[must_use]
    pub fn accepting_statuses(mut self, statuses: impl IntoIterator<Item = u16>) -> Self {
        self.accepted_statuses = statuses.into_iter().collect();
        self
    }

    #[must_use]
    pub fn accepts(&self, status: reqwest::StatusCode) -> bool {
        if self.accepted_statuses.is_empty() {
            return status.is_success() || status.is_redirection();
        }
        self.accepted_statuses.contains(&status.as_u16())
    }
}

#[derive(Debug, Clone)]
pub struct HttpProbe {
    pub status: u16,
    pub latency_ms: u128,
}

#[derive(Debug, Clone)]
pub struct TargetBenchmark {
    pub name: String,
    pub success_count: usize,
    pub attempt_count: usize,
    pub median_delay_ms: Option<u128>,
    pub jitter_ms: Option<u128>,
}

impl TargetBenchmark {
    #[must_use]
    pub fn covered(&self) -> bool {
        self.success_count > 0 && self.success_count * 2 >= self.attempt_count
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedActivationNode {
    pub name: String,
    pub protocol: String,
    pub region: NodeRegion,
    pub country_code: String,
    pub exit_success_count: usize,
    pub exit_attempt_count: usize,
    pub successful_targets: usize,
    pub target_count: usize,
    pub success_count: usize,
    pub attempt_count: usize,
    pub median_delay_ms: u128,
    pub jitter_ms: u128,
    pub score: u128,
    pub targets: Vec<TargetBenchmark>,
}

#[derive(Debug, Clone)]
pub struct NodeSelectionReport {
    pub selected: VerifiedActivationNode,
    pub verified: Vec<VerifiedActivationNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedActivationNode {
    pub name: String,
    pub protocol: String,
    pub region: NodeRegion,
    pub country_code: String,
    pub successful_targets: usize,
    pub target_count: usize,
    pub success_count: usize,
    pub attempt_count: usize,
    pub median_delay_ms: u128,
    pub jitter_ms: u128,
    pub score: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySelectionCache {
    pub schema_version: u32,
    pub saved_at_unix_ms: u128,
    pub selected: CachedActivationNode,
    pub fallbacks: Vec<CachedActivationNode>,
}

impl ProxyExitProbe {
    #[must_use]
    pub fn excluded_region(&self) -> bool {
        matches!(self.country_code.as_str(), "CN" | "HK")
    }
}

pub(crate) async fn verify_selected_node(
    result: &NodeBenchmark,
    local_proxy_url: &Url,
    exit_target_url: &Url,
    probe_targets: &[ActivationProbeTarget],
    minimum_target_coverage: usize,
    attempts: usize,
    timeout: Duration,
) -> Option<VerifiedActivationNode> {
    let mut exit_delays = Vec::with_capacity(attempts);
    let mut country_code = None;
    for _ in 0..attempts {
        let Ok(exit) = probe_proxy_exit(local_proxy_url, exit_target_url, timeout).await else {
            continue;
        };
        if exit.excluded_region() {
            return None;
        }
        if country_code
            .as_ref()
            .is_some_and(|country| country != &exit.country_code)
        {
            return None;
        }
        country_code = Some(exit.country_code);
        exit_delays.push(exit.latency_ms);
    }
    tracing::info!(
        successful = exit_delays.len(),
        attempts,
        "activation exit probe completed"
    );

    let mut target_samples = Vec::with_capacity(probe_targets.len());
    for target in probe_targets {
        let mut delays = Vec::with_capacity(attempts);
        for _ in 0..attempts {
            if let Ok(probe) = probe_proxy_target(local_proxy_url, target, timeout).await {
                delays.push(probe.latency_ms);
            }
        }
        tracing::info!(
            target = target.name,
            successful = delays.len(),
            attempts,
            "activation target probe completed"
        );
        target_samples.push((target.name.clone(), delays));
    }

    verified_from_probe_samples(
        result,
        country_code?,
        exit_delays,
        attempts,
        target_samples,
        minimum_target_coverage,
    )
}

fn verified_from_probe_samples(
    result: &NodeBenchmark,
    country_code: String,
    mut exit_delays: Vec<u128>,
    attempts: usize,
    target_samples: Vec<(String, Vec<u128>)>,
    minimum_target_coverage: usize,
) -> Option<VerifiedActivationNode> {
    if exit_delays.is_empty() || exit_delays.len() * 2 < attempts || target_samples.is_empty() {
        return None;
    }

    let exit_success_count = exit_delays.len();
    let mut all_delays = Vec::new();
    all_delays.append(&mut exit_delays);
    let targets: Vec<_> = target_samples
        .into_iter()
        .map(|(name, mut delays)| {
            all_delays.extend(delays.iter().copied());
            delays.sort_unstable();
            TargetBenchmark {
                name,
                success_count: delays.len(),
                attempt_count: attempts,
                median_delay_ms: delays.get(delays.len() / 2).copied(),
                jitter_ms: delays
                    .first()
                    .zip(delays.last())
                    .map(|(first, last)| last.saturating_sub(*first)),
            }
        })
        .collect();
    let successful_targets = targets.iter().filter(|target| target.covered()).count();
    if successful_targets < minimum_target_coverage {
        return None;
    }

    all_delays.sort_unstable();
    let median_delay_ms = all_delays[all_delays.len() / 2];
    let jitter_ms = all_delays
        .first()
        .zip(all_delays.last())
        .map_or(0, |(first, last)| last.saturating_sub(*first));
    let success_count = targets.iter().map(|target| target.success_count).sum();
    let attempt_count = attempts.saturating_mul(targets.len());
    let failures = attempt_count
        .saturating_sub(success_count)
        .saturating_add(attempts.saturating_sub(exit_success_count));
    let missing_targets = targets.len().saturating_sub(successful_targets);
    let score = (missing_targets as u128)
        .saturating_mul(MISSING_TARGET_PENALTY)
        .saturating_add((failures as u128).saturating_mul(FAILED_ATTEMPT_PENALTY))
        .saturating_add(median_delay_ms)
        .saturating_add(jitter_ms / 2);

    Some(VerifiedActivationNode {
        name: result.name.clone(),
        protocol: result.protocol.clone(),
        region: result.region,
        country_code,
        exit_success_count,
        exit_attempt_count: attempts,
        successful_targets,
        target_count: targets.len(),
        success_count,
        attempt_count,
        median_delay_ms,
        jitter_ms,
        score,
        targets,
    })
}

pub async fn probe_proxy_target(
    proxy_url: &Url,
    target: &ActivationProbeTarget,
    timeout: Duration,
) -> Result<HttpProbe, ProxyProbeError> {
    if !matches!(proxy_url.scheme(), "http" | "https" | "socks5") {
        return Err(ProxyProbeError::InvalidProxyUrl);
    }
    let client = Client::builder()
        .proxy(Proxy::all(proxy_url.as_str())?)
        .timeout(timeout)
        .build()?;
    let started = std::time::Instant::now();
    let response = client.get(target.url.clone()).send().await?;
    let latency_ms = started.elapsed().as_millis();
    if !target.accepts(response.status()) {
        return Err(ProxyProbeError::HttpStatus(response.status()));
    }
    Ok(HttpProbe {
        status: response.status().as_u16(),
        latency_ms,
    })
}

pub async fn probe_proxy_exit(
    proxy_url: &Url,
    target_url: &Url,
    timeout: Duration,
) -> Result<ProxyExitProbe, ProxyProbeError> {
    if !matches!(proxy_url.scheme(), "http" | "https" | "socks5") {
        return Err(ProxyProbeError::InvalidProxyUrl);
    }
    let client = Client::builder()
        .proxy(Proxy::all(proxy_url.as_str())?)
        .timeout(timeout)
        .build()?;
    let started = std::time::Instant::now();
    let response = client.get(target_url.clone()).send().await?;
    if !response.status().is_success() {
        return Err(ProxyProbeError::HttpStatus(response.status()));
    }
    let body = response.text().await?;
    let country_code = body
        .lines()
        .find_map(|line| line.strip_prefix("loc="))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(ProxyProbeError::ExitCountryMissing)?
        .to_ascii_uppercase();
    Ok(ProxyExitProbe {
        country_code,
        latency_ms: started.elapsed().as_millis(),
    })
}

#[must_use]
pub fn build_proxy_selection_cache(report: &NodeSelectionReport) -> ProxySelectionCache {
    let selected = CachedActivationNode::from(&report.selected);
    let fallbacks = report
        .verified
        .iter()
        .filter(|node| node.name != report.selected.name)
        .take(FALLBACK_NODE_LIMIT)
        .map(CachedActivationNode::from)
        .collect();
    let saved_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    ProxySelectionCache {
        schema_version: PROXY_CACHE_SCHEMA_VERSION,
        saved_at_unix_ms,
        selected,
        fallbacks,
    }
}

pub fn save_proxy_selection_cache(
    path: &Path,
    report: &NodeSelectionReport,
) -> Result<ProxySelectionCache, ProxyCacheError> {
    let cache = build_proxy_selection_cache(report);
    if cache.selected.name.trim().is_empty() {
        return Err(ProxyCacheError::MissingSelectedNode);
    }

    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary_path = cache_temporary_path(path, parent)?;
    let payload = serde_json::to_vec_pretty(&cache)?;
    let write_result = write_private_cache_file(&temporary_path, &payload)
        .and_then(|()| replace_cache_file(&temporary_path, path));
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    write_result?;
    Ok(cache)
}

pub fn load_proxy_selection_cache(path: &Path) -> Result<ProxySelectionCache, ProxyCacheError> {
    let cache: ProxySelectionCache = serde_json::from_slice(&fs::read(path)?)?;
    if cache.schema_version != PROXY_CACHE_SCHEMA_VERSION {
        return Err(ProxyCacheError::UnsupportedVersion(cache.schema_version));
    }
    if cache.selected.name.trim().is_empty() {
        return Err(ProxyCacheError::MissingSelectedNode);
    }
    Ok(cache)
}

impl From<&VerifiedActivationNode> for CachedActivationNode {
    fn from(node: &VerifiedActivationNode) -> Self {
        Self {
            name: node.name.clone(),
            protocol: node.protocol.clone(),
            region: node.region,
            country_code: node.country_code.clone(),
            successful_targets: node.successful_targets,
            target_count: node.target_count,
            success_count: node.success_count,
            attempt_count: node.attempt_count,
            median_delay_ms: node.median_delay_ms,
            jitter_ms: node.jitter_ms,
            score: node.score,
        }
    }
}

fn cache_temporary_path(path: &Path, parent: &Path) -> Result<PathBuf, ProxyCacheError> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(ProxyCacheError::InvalidPath)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce)))
}

fn write_private_cache_file(path: &Path, payload: &[u8]) -> Result<(), std::io::Error> {
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

fn replace_cache_file(temporary_path: &Path, path: &Path) -> Result<(), std::io::Error> {
    #[cfg(target_os = "windows")]
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary_path, path)
}

pub fn parse_subscription(payload: &[u8]) -> Result<ParsedSubscription, SubscriptionError> {
    let text = decode_subscription_text(payload)?;
    let mut parsed = ParsedSubscription::default();

    for (offset, raw_line) in text.lines().enumerate() {
        let index = offset + 1;
        let raw_line = raw_line.trim();
        if raw_line.is_empty() {
            continue;
        }
        let Ok(url) = Url::parse(raw_line) else {
            parsed.rejected.push(RejectedNode {
                index,
                name: None,
                reason: RejectionReason::InvalidNode,
            });
            continue;
        };
        let protocol = match url.scheme().to_ascii_lowercase().as_str() {
            "vless" => ProxyProtocol::Vless,
            "hysteria2" | "hy2" => ProxyProtocol::Hysteria2,
            _ => {
                parsed.rejected.push(RejectedNode {
                    index,
                    name: decoded_fragment(&url),
                    reason: RejectionReason::UnsupportedProtocol,
                });
                continue;
            }
        };
        let name = decoded_fragment(&url).unwrap_or_else(|| format!("node-{index}"));
        if is_metadata_name(&name) {
            parsed.rejected.push(RejectedNode {
                index,
                name: Some(name),
                reason: RejectionReason::Metadata,
            });
            continue;
        }
        let Some(port) = url.port() else {
            parsed.rejected.push(RejectedNode {
                index,
                name: Some(name),
                reason: RejectionReason::InvalidNode,
            });
            continue;
        };
        if url.host_str().is_none() || url.username().is_empty() {
            parsed.rejected.push(RejectedNode {
                index,
                name: Some(name),
                reason: RejectionReason::InvalidNode,
            });
            continue;
        }
        let region = classify_region(&name);
        if region.excluded_for_activation() {
            parsed.rejected.push(RejectedNode {
                index,
                name: Some(name),
                reason: RejectionReason::ExcludedRegion,
            });
            continue;
        }
        parsed.candidates.push(ProxyNode {
            index,
            name,
            protocol,
            region,
            server: url.host_str().unwrap_or_default().to_owned(),
            port,
            uri: SecretNodeUri(raw_line.to_owned()),
        });
    }

    Ok(parsed)
}

fn decode_subscription_text(payload: &[u8]) -> Result<String, SubscriptionError> {
    let raw = std::str::from_utf8(payload).map_err(|_| SubscriptionError::InvalidUtf8)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(SubscriptionError::Empty);
    }
    if contains_supported_uri(trimmed) {
        return Ok(normalize_lines(trimmed));
    }

    let compact: String = trimmed
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let decoded = [STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD]
        .into_iter()
        .find_map(|engine| engine.decode(compact.as_bytes()).ok())
        .ok_or(SubscriptionError::InvalidEncoding)?;
    let decoded = String::from_utf8(decoded).map_err(|_| SubscriptionError::InvalidUtf8)?;
    if !contains_supported_uri(&decoded) {
        return Err(SubscriptionError::InvalidEncoding);
    }
    Ok(normalize_lines(&decoded))
}

fn contains_supported_uri(value: &str) -> bool {
    value.contains("vless://") || value.contains("hysteria2://") || value.contains("hy2://")
}

fn normalize_lines(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn decoded_fragment(url: &Url) -> Option<String> {
    let fragment = url.fragment()?.trim();
    if fragment.is_empty() {
        return None;
    }
    percent_decode_str(fragment)
        .decode_utf8()
        .ok()
        .map(|value| value.replace(['\r', '\n'], " ").trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[must_use]
pub fn classify_region(name: &str) -> NodeRegion {
    let lower = name.to_ascii_lowercase();
    if contains_any(name, &['🇭', '港']) || lower.contains("hong kong") || has_token(&lower, "hk")
    {
        return NodeRegion::HongKong;
    }
    if name.contains("中国大陆")
        || name.contains("大陆")
        || name.contains("国内")
        || name.contains("北京")
        || name.contains("上海")
        || name.contains("广州")
        || name.contains("深圳")
        || lower.contains("mainland china")
        || has_token(&lower, "cn")
    {
        return NodeRegion::MainlandChina;
    }
    if name.contains('台') || name.contains("台湾") || lower.contains("taiwan") {
        return NodeRegion::Taiwan;
    }
    if name.contains('日') || lower.contains("japan") || has_token(&lower, "jp") {
        return NodeRegion::Japan;
    }
    if name.contains("美国")
        || name.contains("洛杉矶")
        || name.contains("纽约")
        || name.contains("圣何塞")
        || lower.contains("united states")
        || has_token(&lower, "us")
    {
        return NodeRegion::UnitedStates;
    }
    if name.contains("新加坡") || lower.contains("singapore") || has_token(&lower, "sg") {
        return NodeRegion::Singapore;
    }
    if name.contains("荷兰") || lower.contains("netherlands") || has_token(&lower, "nl") {
        return NodeRegion::Netherlands;
    }
    if name.contains("法国") || lower.contains("france") || has_token(&lower, "fr") {
        return NodeRegion::France;
    }
    if name.contains("德国") || lower.contains("germany") || has_token(&lower, "de") {
        return NodeRegion::Germany;
    }
    if name.contains("英国") || lower.contains("united kingdom") || has_token(&lower, "uk") {
        return NodeRegion::UnitedKingdom;
    }
    if name.contains("巴西") || lower.contains("brazil") || has_token(&lower, "br") {
        return NodeRegion::Brazil;
    }
    NodeRegion::Other
}

fn contains_any(value: &str, characters: &[char]) -> bool {
    characters
        .iter()
        .any(|character| value.contains(*character))
}

fn has_token(value: &str, token: &str) -> bool {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|part| part == token)
}

fn is_metadata_name(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase();
    name.contains("剩余流量")
        || name.contains("套餐到期")
        || name.contains("距离下次重置")
        || name.contains("官网")
        || name.contains("客服")
        || normalized.starts_with("traffic")
        || normalized.starts_with("expire")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::{Arc, Mutex};

    const JP_NODE: &str = "vless://00000000-0000-0000-0000-000000000001@example.com:443?security=reality#%F0%9F%87%AF%F0%9F%87%B5%E6%97%A5%E6%9C%AC";
    const HK_NODE: &str = "hysteria2://password@example.net:8443?sni=example.net#%F0%9F%87%AD%F0%9F%87%B0%E9%A6%99%E6%B8%AF";

    #[test]
    fn parses_base64_subscription_and_excludes_hong_kong() {
        let plain = format!("剩余流量\r\n{JP_NODE}\r\n{HK_NODE}\r\n");
        let encoded = STANDARD.encode(plain.as_bytes());
        let parsed = parse_subscription(encoded.as_bytes()).expect("valid subscription");
        assert_eq!(parsed.candidates.len(), 1);
        assert_eq!(parsed.candidates[0].region, NodeRegion::Japan);
        assert!(parsed
            .rejected
            .iter()
            .any(|node| node.reason == RejectionReason::ExcludedRegion));
    }

    #[test]
    fn classifies_supported_regions_without_excluding_taiwan() {
        assert_eq!(classify_region("🇭🇰香港"), NodeRegion::HongKong);
        assert_eq!(classify_region("深圳国内"), NodeRegion::MainlandChina);
        assert_eq!(classify_region("🇹🇼台湾"), NodeRegion::Taiwan);
        assert!(!NodeRegion::Taiwan.excluded_for_activation());
    }

    #[test]
    fn secret_uri_is_redacted_from_debug_output() {
        let secret = SecretNodeUri("vless://credential@example.com:443".to_owned());
        assert_eq!(format!("{secret:?}"), "SecretNodeUri([REDACTED])");
    }

    #[test]
    fn verified_score_penalizes_jitter() {
        let benchmark = benchmark("US test");
        let stable = verified_from_probe_samples(
            &benchmark,
            "US".to_owned(),
            vec![200, 210, 205],
            3,
            vec![("chatgpt-web".to_owned(), vec![300, 320, 310])],
            1,
        )
        .expect("stable node");
        let unstable = verified_from_probe_samples(
            &benchmark,
            "US".to_owned(),
            vec![200, 210, 205],
            3,
            vec![("chatgpt-web".to_owned(), vec![300, 2_000, 310])],
            1,
        )
        .expect("unstable node");
        assert!(stable.score < unstable.score);
    }

    #[test]
    fn verified_score_prioritizes_openai_target_coverage() {
        let benchmark = benchmark("US test");
        let full_coverage = verified_from_probe_samples(
            &benchmark,
            "US".to_owned(),
            vec![800, 810, 820],
            3,
            vec![
                ("chatgpt-web".to_owned(), vec![900, 910, 920]),
                ("openai-api".to_owned(), vec![950, 960, 970]),
            ],
            1,
        )
        .expect("fully covered node");
        let partial_coverage = verified_from_probe_samples(
            &benchmark,
            "US".to_owned(),
            vec![100, 110, 120],
            3,
            vec![
                ("chatgpt-web".to_owned(), vec![150, 160, 170]),
                ("openai-api".to_owned(), Vec::new()),
            ],
            1,
        )
        .expect("partially covered node");

        assert_eq!(full_coverage.successful_targets, 2);
        assert_eq!(partial_coverage.successful_targets, 1);
        assert!(full_coverage.score < partial_coverage.score);
    }

    #[test]
    fn openai_api_probe_accepts_expected_unauthorized_response() {
        let target = ActivationProbeTarget::new(
            "openai-api",
            Url::parse("https://api.openai.com/v1/models").expect("valid URL"),
        )
        .accepting_statuses([200, 401]);

        assert!(target.accepts(reqwest::StatusCode::UNAUTHORIZED));
        assert!(!target.accepts(reqwest::StatusCode::FORBIDDEN));
    }

    #[test]
    fn selection_cache_only_keeps_metadata_and_two_fallbacks() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("proxy-cache.json");
        let nodes: Vec<_> = ["selected", "fallback-1", "fallback-2", "fallback-3"]
            .into_iter()
            .map(|name| {
                verified_from_probe_samples(
                    &benchmark(name),
                    "US".to_owned(),
                    vec![200, 210, 220],
                    3,
                    vec![("chatgpt-web".to_owned(), vec![300, 310, 320])],
                    1,
                )
                .expect("verified node")
            })
            .collect();
        let report = NodeSelectionReport {
            selected: nodes[0].clone(),
            verified: nodes,
        };

        save_proxy_selection_cache(&path, &report).expect("save cache");
        let loaded = load_proxy_selection_cache(&path).expect("load cache");
        let raw = fs::read_to_string(path).expect("read cache");

        assert_eq!(loaded.selected.name, "selected");
        assert_eq!(loaded.fallbacks.len(), 2);
        assert!(!raw.contains("vless://"));
        assert!(!raw.contains("password"));
    }

    #[tokio::test]
    async fn local_proxy_supervisor_waits_for_readiness_and_shuts_down() {
        let engine = FakeLocalProxyEngine::new(loopback_endpoint(), false, false);
        let state = engine.state.clone();
        let supervisor = LocalProxySupervisor::new(engine, Duration::from_secs(2));

        let session = supervisor
            .start(&proxy_node())
            .await
            .expect("ready proxy session");
        assert_eq!(session.endpoint(), loopback_endpoint());
        session.shutdown().await.expect("proxy shutdown");

        let state = state.lock().expect("fake proxy state");
        assert_eq!(state.readiness_calls, 1);
        assert_eq!(state.shutdown_calls, 1);
        assert_eq!(state.abort_calls, 0);
    }

    #[tokio::test]
    async fn local_proxy_supervisor_rejects_non_loopback_endpoint() {
        let endpoint = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 20)), 17_892);
        let engine = FakeLocalProxyEngine::new(endpoint, false, false);
        let state = engine.state.clone();
        let error = LocalProxySupervisor::new(engine, Duration::from_secs(2))
            .start(&proxy_node())
            .await
            .expect_err("remote endpoint must be rejected");

        assert!(matches!(error, LocalProxyError::InvalidEndpoint));
        assert_eq!(state.lock().expect("fake proxy state").abort_calls, 1);
    }

    #[tokio::test]
    async fn readiness_failure_triggers_graceful_cleanup() {
        let engine = FakeLocalProxyEngine::new(loopback_endpoint(), true, false);
        let state = engine.state.clone();
        let error = LocalProxySupervisor::new(engine, Duration::from_secs(2))
            .start(&proxy_node())
            .await
            .expect_err("readiness failure");

        assert!(matches!(error, LocalProxyError::Readiness(_)));
        let state = state.lock().expect("fake proxy state");
        assert_eq!(state.shutdown_calls, 1);
        assert_eq!(state.abort_calls, 0);
    }

    #[tokio::test]
    async fn dropping_active_proxy_session_aborts_it() {
        let engine = FakeLocalProxyEngine::new(loopback_endpoint(), false, false);
        let state = engine.state.clone();
        let session = LocalProxySupervisor::new(engine, Duration::from_secs(2))
            .start(&proxy_node())
            .await
            .expect("ready proxy session");

        drop(session);

        assert_eq!(state.lock().expect("fake proxy state").abort_calls, 1);
    }

    fn benchmark(name: &str) -> NodeBenchmark {
        NodeBenchmark {
            name: name.to_owned(),
            protocol: "Vless".to_owned(),
            region: NodeRegion::UnitedStates,
            success_count: 3,
            attempt_count: 3,
            median_delay_ms: Some(300),
            jitter_ms: Some(20),
            score: 310,
        }
    }

    fn proxy_node() -> ProxyNode {
        ProxyNode {
            index: 1,
            name: "US test".to_owned(),
            protocol: ProxyProtocol::Vless,
            region: NodeRegion::UnitedStates,
            server: "example.com".to_owned(),
            port: 443,
            uri: SecretNodeUri(
                "vless://00000000-0000-0000-0000-000000000001@example.com:443".to_owned(),
            ),
        }
    }

    fn loopback_endpoint() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 17_892)
    }

    #[derive(Debug, Default)]
    struct FakeLocalProxyState {
        readiness_calls: usize,
        shutdown_calls: usize,
        abort_calls: usize,
    }

    #[derive(Debug, Clone)]
    struct FakeLocalProxyEngine {
        endpoint: SocketAddr,
        readiness_fails: bool,
        shutdown_fails: bool,
        state: Arc<Mutex<FakeLocalProxyState>>,
    }

    impl FakeLocalProxyEngine {
        fn new(endpoint: SocketAddr, readiness_fails: bool, shutdown_fails: bool) -> Self {
            Self {
                endpoint,
                readiness_fails,
                shutdown_fails,
                state: Arc::new(Mutex::new(FakeLocalProxyState::default())),
            }
        }
    }

    #[async_trait]
    impl LocalProxyEngine for FakeLocalProxyEngine {
        async fn start(
            &self,
            _node: &ProxyNode,
        ) -> Result<Box<dyn LocalProxySession>, LocalProxyError> {
            Ok(Box::new(FakeLocalProxySession {
                endpoint: self.endpoint,
                readiness_fails: self.readiness_fails,
                shutdown_fails: self.shutdown_fails,
                state: self.state.clone(),
            }))
        }
    }

    struct FakeLocalProxySession {
        endpoint: SocketAddr,
        readiness_fails: bool,
        shutdown_fails: bool,
        state: Arc<Mutex<FakeLocalProxyState>>,
    }

    #[async_trait]
    impl LocalProxySession for FakeLocalProxySession {
        fn endpoint(&self) -> SocketAddr {
            self.endpoint
        }

        async fn wait_until_ready(&mut self, _timeout: Duration) -> Result<(), LocalProxyError> {
            self.state.lock().expect("fake proxy state").readiness_calls += 1;
            if self.readiness_fails {
                Err(LocalProxyError::Readiness("mock failure".to_owned()))
            } else {
                Ok(())
            }
        }

        async fn shutdown(&mut self) -> Result<(), LocalProxyError> {
            self.state.lock().expect("fake proxy state").shutdown_calls += 1;
            if self.shutdown_fails {
                Err(LocalProxyError::Shutdown("mock failure".to_owned()))
            } else {
                Ok(())
            }
        }

        fn abort(&mut self) {
            self.state.lock().expect("fake proxy state").abort_calls += 1;
        }
    }
}
