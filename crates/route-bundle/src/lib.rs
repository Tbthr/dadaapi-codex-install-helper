use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use ed25519_dalek::pkcs8::DecodePublicKey;
use ed25519_dalek::{Signature, VerifyingKey};
use futures_util::StreamExt;
use reqwest::redirect::Policy;
use reqwest::Client;
use route_catalog::SubscriptionPayload;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use url::Url;
use zeroize::Zeroizing;

const MANIFEST_FILE_NAME: &str = "manifest.json";
const ROUTE_FILE_NAME: &str = "routes.enc";
const SIGNATURE_FILE_NAME: &str = "routes.sig";
const ROUTE_MAGIC: &[u8; 8] = b"DADAR002";
const ROUTE_AAD: &[u8] = b"dadaapi-routes/v2";
const CACHE_MAGIC: &[u8; 8] = b"DADAC002";
const MAX_MANIFEST_BYTES: usize = 64 * 1024;
const MAX_SIGNATURE_BYTES: usize = 1024;
const MAX_PLAINTEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_ROUTE_BYTES: usize = MAX_PLAINTEXT_BYTES + ROUTE_MAGIC.len() + 24 + 16;
const MAX_CACHE_BYTES: u64 =
    (MAX_MANIFEST_BYTES + MAX_SIGNATURE_BYTES + MAX_ROUTE_BYTES + 24) as u64;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_CLOCK_SKEW: ChronoDuration = ChronoDuration::minutes(5);
const MAX_REDIRECTS: usize = 5;
const CACHE_EXTENSION: &str = "bundle";
const MAX_CACHE_GENERATIONS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleResource {
    Manifest,
    Signature,
    Routes,
}

impl fmt::Display for BundleResource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Manifest => "路由清单",
            Self::Signature => "路由签名",
            Self::Routes => "加密路由",
        })
    }
}

#[derive(Debug, Error)]
pub enum RouteBundleError {
    #[error("静态路由清单必须是无凭据、无查询参数的 HTTPS manifest.json 地址")]
    InvalidManifestUrl,
    #[error("静态路由验签公钥无效")]
    InvalidPublicKey,
    #[error("静态路由解密密钥必须是 32 字节 Base64")]
    InvalidEncryptionKey,
    #[error("静态路由密钥标识不能为空")]
    InvalidKeyId,
    #[error("静态路由 HTTP 客户端初始化失败")]
    ClientBuild,
    #[error("{resource}请求超时")]
    Timeout { resource: BundleResource },
    #[error("无法连接{resource}服务")]
    Connect { resource: BundleResource },
    #[error("{resource}请求失败")]
    Request { resource: BundleResource },
    #[error("{resource}返回状态码 {status}")]
    HttpStatus {
        resource: BundleResource,
        status: u16,
    },
    #[error("{resource}超过大小限制 {limit_bytes} 字节")]
    BodyTooLarge {
        resource: BundleResource,
        limit_bytes: usize,
    },
    #[error("静态路由签名编码无效")]
    InvalidSignatureEncoding,
    #[error("静态路由签名验证失败")]
    InvalidSignature,
    #[error("静态路由清单 JSON 无效")]
    InvalidManifestJson,
    #[error("静态路由清单版本不受支持")]
    UnsupportedSchema,
    #[error("静态路由清单内容无效：{0}")]
    InvalidManifest(String),
    #[error("静态路由清单生成时间超出允许的时钟偏差")]
    GeneratedInFuture,
    #[error("静态路由包已经过期")]
    Expired,
    #[error("静态路由密钥标识不匹配")]
    KeyIdMismatch,
    #[error("加密路由长度与清单不一致")]
    RouteSizeMismatch,
    #[error("加密路由 SHA-256 校验失败")]
    RouteHashMismatch,
    #[error("加密路由文件格式无效")]
    InvalidRouteFormat,
    #[error("加密路由认证或解密失败")]
    DecryptionFailed,
    #[error("解密后的订阅超过 8MB 限制")]
    PlaintextTooLarge,
    #[error("静态路由后台任务失败")]
    BackgroundTask,
    #[error("静态路由密文缓存不可用")]
    CacheUnavailable,
    #[error("静态路由密文缓存文件操作失败")]
    CacheIo,
    #[error("静态路由密文缓存格式无效")]
    InvalidCacheFormat,
    #[error("远程静态路由不可用，并且本地密文缓存也无法使用：远程={remote}; 缓存={cache}")]
    RemoteAndCacheUnavailable {
        remote: Box<RouteBundleError>,
        cache: Box<RouteBundleError>,
    },
}

impl RouteBundleError {
    #[must_use]
    pub fn permits_cache_fallback(&self) -> bool {
        matches!(
            self,
            Self::Timeout { .. }
                | Self::Connect { .. }
                | Self::Request { .. }
                | Self::HttpStatus {
                    status: 500..=599,
                    ..
                }
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RouteManifest {
    pub schema_version: u32,
    pub version: String,
    pub generated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub route_file: String,
    pub route_sha256: String,
    pub route_size: u64,
    pub encryption: String,
    pub key_id: String,
}

#[derive(Clone)]
struct RouteBundleVerifier {
    public_key: VerifyingKey,
    encryption_key: Zeroizing<[u8; 32]>,
    expected_key_id: String,
}

impl RouteBundleVerifier {
    fn verify(
        &self,
        bundle: &RawRouteBundle,
        now: DateTime<Utc>,
    ) -> Result<SubscriptionPayload, RouteBundleError> {
        verify_manifest_signature(&self.public_key, &bundle.manifest, &bundle.signature)?;
        let manifest: RouteManifest = serde_json::from_slice(&bundle.manifest)
            .map_err(|_| RouteBundleError::InvalidManifestJson)?;
        validate_manifest(&manifest, &self.expected_key_id, now)?;
        validate_encrypted_routes(&manifest, &bundle.routes)?;
        decrypt_routes(&bundle.routes, &self.encryption_key)
    }
}

#[derive(Clone)]
struct RawRouteBundle {
    manifest: Vec<u8>,
    signature: Vec<u8>,
    routes: Vec<u8>,
}

#[derive(Clone)]
struct RouteBundleSource {
    manifest_url: Url,
    signature_url: Url,
    routes_url: Url,
}

impl RouteBundleSource {
    fn new(manifest_url: Url) -> Result<Self, RouteBundleError> {
        validate_manifest_url(&manifest_url)?;
        let signature_url = manifest_url
            .join(SIGNATURE_FILE_NAME)
            .map_err(|_| RouteBundleError::InvalidManifestUrl)?;
        let routes_url = manifest_url
            .join(ROUTE_FILE_NAME)
            .map_err(|_| RouteBundleError::InvalidManifestUrl)?;
        Ok(Self {
            manifest_url,
            signature_url,
            routes_url,
        })
    }
}

pub struct RouteBundleClient {
    client: Client,
    sources: Vec<RouteBundleSource>,
    verifier: RouteBundleVerifier,
    cache_directory: PathBuf,
    request_timeout: Duration,
}

impl fmt::Debug for RouteBundleClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RouteBundleClient")
            .field(
                "manifest_urls",
                &self
                    .sources
                    .iter()
                    .map(|source| &source.manifest_url)
                    .collect::<Vec<_>>(),
            )
            .field("expected_key_id", &self.verifier.expected_key_id)
            .field("encryption_key", &"[REDACTED]")
            .field("cache_directory", &self.cache_directory)
            .finish()
    }
}

impl RouteBundleClient {
    pub fn new(
        manifest_url: Url,
        public_key_pem: &str,
        encryption_key: Zeroizing<[u8; 32]>,
        expected_key_id: String,
        cache_directory: PathBuf,
    ) -> Result<Self, RouteBundleError> {
        Self::new_with_fallbacks(
            vec![manifest_url],
            public_key_pem,
            encryption_key,
            expected_key_id,
            cache_directory,
        )
    }

    pub fn new_with_fallbacks(
        manifest_urls: Vec<Url>,
        public_key_pem: &str,
        encryption_key: Zeroizing<[u8; 32]>,
        expected_key_id: String,
        cache_directory: PathBuf,
    ) -> Result<Self, RouteBundleError> {
        if manifest_urls.is_empty() {
            return Err(RouteBundleError::InvalidManifestUrl);
        }
        if expected_key_id.trim().is_empty() {
            return Err(RouteBundleError::InvalidKeyId);
        }
        let public_key = VerifyingKey::from_public_key_pem(public_key_pem)
            .map_err(|_| RouteBundleError::InvalidPublicKey)?;
        let sources = manifest_urls
            .into_iter()
            .map(RouteBundleSource::new)
            .collect::<Result<Vec<_>, _>>()?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS || attempt.url().scheme() != "https" {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|_| RouteBundleError::ClientBuild)?;
        Ok(Self {
            client,
            sources,
            verifier: RouteBundleVerifier {
                public_key,
                encryption_key,
                expected_key_id,
            },
            cache_directory,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        })
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    pub async fn fetch_payload(&self) -> Result<SubscriptionPayload, RouteBundleError> {
        let remote = self.fetch_verified_remote_payload().await;
        match remote {
            Ok((bundle, payload)) => {
                let cache_directory = self.cache_directory.clone();
                let cache_result = tokio::task::spawn_blocking(move || {
                    save_cached_bundle(&cache_directory, &bundle)
                })
                .await;
                match cache_result {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) | Err(_) => {
                        tracing::warn!("could not update encrypted route bundle cache");
                    }
                }
                Ok(payload)
            }
            Err(remote_error) if remote_error.permits_cache_fallback() => {
                let cache_directory = self.cache_directory.clone();
                let verifier = self.verifier.clone();
                let cached = tokio::task::spawn_blocking(move || {
                    load_latest_verified_cache(&cache_directory, &verifier, Utc::now())
                })
                .await
                .map_err(|_| RouteBundleError::BackgroundTask)?;
                cached.map_err(|cache_error| RouteBundleError::RemoteAndCacheUnavailable {
                    remote: Box::new(remote_error),
                    cache: Box::new(cache_error),
                })
            }
            Err(error) => Err(error),
        }
    }

    async fn fetch_verified_remote_payload(
        &self,
    ) -> Result<(RawRouteBundle, SubscriptionPayload), RouteBundleError> {
        let mut last_error = None;
        for (source_index, source) in self.sources.iter().enumerate() {
            let bundle = match self.fetch_remote_bundle(source).await {
                Ok(bundle) => bundle,
                Err(error) if error.permits_cache_fallback() => {
                    tracing::warn!(source_index, "route source temporarily unavailable");
                    last_error = Some(error);
                    continue;
                }
                Err(error) => return Err(error),
            };
            let verifier = self.verifier.clone();
            return tokio::task::spawn_blocking(move || {
                let payload = verifier.verify(&bundle, Utc::now())?;
                Ok::<_, RouteBundleError>((bundle, payload))
            })
            .await
            .map_err(|_| RouteBundleError::BackgroundTask)?;
        }
        Err(last_error.unwrap_or(RouteBundleError::InvalidManifestUrl))
    }

    async fn fetch_remote_bundle(
        &self,
        source: &RouteBundleSource,
    ) -> Result<RawRouteBundle, RouteBundleError> {
        let manifest = fetch_bounded(
            &self.client,
            source.manifest_url.clone(),
            BundleResource::Manifest,
            self.request_timeout,
            MAX_MANIFEST_BYTES,
        )
        .await?;
        let signature = fetch_bounded(
            &self.client,
            source.signature_url.clone(),
            BundleResource::Signature,
            self.request_timeout,
            MAX_SIGNATURE_BYTES,
        )
        .await?;

        verify_manifest_signature(&self.verifier.public_key, &manifest, &signature)?;
        let parsed: RouteManifest =
            serde_json::from_slice(&manifest).map_err(|_| RouteBundleError::InvalidManifestJson)?;
        validate_manifest(&parsed, &self.verifier.expected_key_id, Utc::now())?;

        let routes = fetch_bounded(
            &self.client,
            source.routes_url.clone(),
            BundleResource::Routes,
            self.request_timeout,
            MAX_ROUTE_BYTES,
        )
        .await?;
        Ok(RawRouteBundle {
            manifest,
            signature,
            routes,
        })
    }
}

pub fn decode_encryption_key(encoded: &str) -> Result<Zeroizing<[u8; 32]>, RouteBundleError> {
    let decoded = Zeroizing::new(
        STANDARD
            .decode(encoded.trim().as_bytes())
            .map_err(|_| RouteBundleError::InvalidEncryptionKey)?,
    );
    let key = decoded
        .as_slice()
        .try_into()
        .map_err(|_| RouteBundleError::InvalidEncryptionKey)?;
    Ok(Zeroizing::new(key))
}

fn validate_manifest_url(url: &Url) -> Result<(), RouteBundleError> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.path().ends_with(&format!("/{MANIFEST_FILE_NAME}"))
    {
        return Err(RouteBundleError::InvalidManifestUrl);
    }
    Ok(())
}

fn verify_manifest_signature(
    public_key: &VerifyingKey,
    manifest: &[u8],
    encoded_signature: &[u8],
) -> Result<(), RouteBundleError> {
    let encoded = std::str::from_utf8(encoded_signature)
        .map_err(|_| RouteBundleError::InvalidSignatureEncoding)?;
    let signature_bytes = STANDARD
        .decode(encoded.trim().as_bytes())
        .map_err(|_| RouteBundleError::InvalidSignatureEncoding)?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| RouteBundleError::InvalidSignatureEncoding)?;
    public_key
        .verify_strict(manifest, &signature)
        .map_err(|_| RouteBundleError::InvalidSignature)
}

fn validate_manifest(
    manifest: &RouteManifest,
    expected_key_id: &str,
    now: DateTime<Utc>,
) -> Result<(), RouteBundleError> {
    if manifest.schema_version != 2 {
        return Err(RouteBundleError::UnsupportedSchema);
    }
    if manifest.version.trim().is_empty()
        || manifest.route_file != ROUTE_FILE_NAME
        || manifest.encryption != "xchacha20poly1305"
        || manifest.route_sha256.len() != 64
        || !manifest
            .route_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || manifest.route_size < (ROUTE_MAGIC.len() + 24 + 16) as u64
        || manifest.route_size > MAX_ROUTE_BYTES as u64
        || manifest.expires_at <= manifest.generated_at
    {
        return Err(RouteBundleError::InvalidManifest(
            "字段值不符合 v2 路由包协议".to_owned(),
        ));
    }
    if manifest.generated_at > now + MAX_CLOCK_SKEW {
        return Err(RouteBundleError::GeneratedInFuture);
    }
    if manifest.expires_at <= now {
        return Err(RouteBundleError::Expired);
    }
    if manifest.key_id != expected_key_id {
        return Err(RouteBundleError::KeyIdMismatch);
    }
    Ok(())
}

fn validate_encrypted_routes(
    manifest: &RouteManifest,
    routes: &[u8],
) -> Result<(), RouteBundleError> {
    if manifest.route_size != routes.len() as u64 {
        return Err(RouteBundleError::RouteSizeMismatch);
    }
    let hash = encode_hex(&Sha256::digest(routes));
    if hash != manifest.route_sha256 {
        return Err(RouteBundleError::RouteHashMismatch);
    }
    Ok(())
}

fn decrypt_routes(
    encrypted: &[u8],
    encryption_key: &[u8; 32],
) -> Result<SubscriptionPayload, RouteBundleError> {
    if encrypted.len() < ROUTE_MAGIC.len() + 24 + 16
        || &encrypted[..ROUTE_MAGIC.len()] != ROUTE_MAGIC
    {
        return Err(RouteBundleError::InvalidRouteFormat);
    }
    let nonce_start = ROUTE_MAGIC.len();
    let ciphertext_start = nonce_start + 24;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(encryption_key));
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&encrypted[nonce_start..ciphertext_start]),
            Payload {
                msg: &encrypted[ciphertext_start..],
                aad: ROUTE_AAD,
            },
        )
        .map_err(|_| RouteBundleError::DecryptionFailed)?;
    if plaintext.len() > MAX_PLAINTEXT_BYTES {
        return Err(RouteBundleError::PlaintextTooLarge);
    }
    Ok(SubscriptionPayload::new(plaintext))
}

async fn fetch_bounded(
    client: &Client,
    url: Url,
    resource: BundleResource,
    timeout: Duration,
    max_body_bytes: usize,
) -> Result<Vec<u8>, RouteBundleError> {
    let response = client
        .get(url)
        .timeout(timeout)
        .send()
        .await
        .map_err(|error| classify_request_error(resource, &error))?;
    if !response.status().is_success() {
        return Err(RouteBundleError::HttpStatus {
            resource,
            status: response.status().as_u16(),
        });
    }
    if response
        .content_length()
        .is_some_and(|length| length > max_body_bytes as u64)
    {
        return Err(RouteBundleError::BodyTooLarge {
            resource,
            limit_bytes: max_body_bytes,
        });
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| classify_request_error(resource, &error))?;
        if body.len().saturating_add(chunk.len()) > max_body_bytes {
            return Err(RouteBundleError::BodyTooLarge {
                resource,
                limit_bytes: max_body_bytes,
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn classify_request_error(resource: BundleResource, error: &reqwest::Error) -> RouteBundleError {
    if error.is_timeout() {
        RouteBundleError::Timeout { resource }
    } else if error.is_connect() {
        RouteBundleError::Connect { resource }
    } else {
        RouteBundleError::Request { resource }
    }
}

fn save_cached_bundle(
    cache_directory: &Path,
    bundle: &RawRouteBundle,
) -> Result<(), RouteBundleError> {
    create_private_cache_directory(cache_directory)?;
    let payload = encode_cache(bundle)?;
    let digest = encode_hex(&Sha256::digest(&payload));
    let target = cache_directory.join(format!("route-{}.{}", &digest[..24], CACHE_EXTENSION));
    if target.is_file() {
        cleanup_old_cache_generations(cache_directory);
        return Ok(());
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = cache_directory.join(format!(".route.{}.{}.tmp", std::process::id(), nonce));
    let write_result = write_private_file(&temporary, &payload).and_then(|()| {
        if target.exists() {
            fs::remove_file(&temporary)
        } else {
            fs::rename(&temporary, &target)
        }
    });
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result.map_err(|_| RouteBundleError::CacheIo)?;
    cleanup_old_cache_generations(cache_directory);
    Ok(())
}

fn load_latest_verified_cache(
    cache_directory: &Path,
    verifier: &RouteBundleVerifier,
    now: DateTime<Utc>,
) -> Result<SubscriptionPayload, RouteBundleError> {
    let entries = fs::read_dir(cache_directory).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            RouteBundleError::CacheUnavailable
        } else {
            RouteBundleError::CacheIo
        }
    })?;
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| RouteBundleError::CacheIo)?;
        let file_type = entry.file_type().map_err(|_| RouteBundleError::CacheIo)?;
        if !file_type.is_file()
            || entry.path().extension().and_then(|value| value.to_str()) != Some(CACHE_EXTENSION)
        {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        candidates.push((modified, entry.path()));
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
    if candidates.is_empty() {
        return Err(RouteBundleError::CacheUnavailable);
    }

    let mut last_error = RouteBundleError::CacheUnavailable;
    for (_, path) in candidates {
        match read_cached_bundle(&path).and_then(|bundle| verifier.verify(&bundle, now)) {
            Ok(payload) => return Ok(payload),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

fn encode_cache(bundle: &RawRouteBundle) -> Result<Vec<u8>, RouteBundleError> {
    let manifest_length =
        u32::try_from(bundle.manifest.len()).map_err(|_| RouteBundleError::InvalidCacheFormat)?;
    let signature_length =
        u32::try_from(bundle.signature.len()).map_err(|_| RouteBundleError::InvalidCacheFormat)?;
    let routes_length =
        u64::try_from(bundle.routes.len()).map_err(|_| RouteBundleError::InvalidCacheFormat)?;
    let mut payload = Vec::with_capacity(
        24 + bundle.manifest.len() + bundle.signature.len() + bundle.routes.len(),
    );
    payload.extend_from_slice(CACHE_MAGIC);
    payload.extend_from_slice(&manifest_length.to_be_bytes());
    payload.extend_from_slice(&signature_length.to_be_bytes());
    payload.extend_from_slice(&routes_length.to_be_bytes());
    payload.extend_from_slice(&bundle.manifest);
    payload.extend_from_slice(&bundle.signature);
    payload.extend_from_slice(&bundle.routes);
    Ok(payload)
}

fn read_cached_bundle(path: &Path) -> Result<RawRouteBundle, RouteBundleError> {
    let metadata = fs::metadata(path).map_err(|_| RouteBundleError::CacheIo)?;
    if metadata.len() > MAX_CACHE_BYTES {
        return Err(RouteBundleError::InvalidCacheFormat);
    }
    let payload = fs::read(path).map_err(|_| RouteBundleError::CacheIo)?;
    if payload.len() < 24 || &payload[..CACHE_MAGIC.len()] != CACHE_MAGIC {
        return Err(RouteBundleError::InvalidCacheFormat);
    }
    let manifest_length = read_u32(&payload[8..12])? as usize;
    let signature_length = read_u32(&payload[12..16])? as usize;
    let routes_length = usize::try_from(read_u64(&payload[16..24])?)
        .map_err(|_| RouteBundleError::InvalidCacheFormat)?;
    if manifest_length > MAX_MANIFEST_BYTES
        || signature_length > MAX_SIGNATURE_BYTES
        || routes_length > MAX_ROUTE_BYTES
    {
        return Err(RouteBundleError::InvalidCacheFormat);
    }
    let manifest_end = 24_usize
        .checked_add(manifest_length)
        .ok_or(RouteBundleError::InvalidCacheFormat)?;
    let signature_end = manifest_end
        .checked_add(signature_length)
        .ok_or(RouteBundleError::InvalidCacheFormat)?;
    let routes_end = signature_end
        .checked_add(routes_length)
        .ok_or(RouteBundleError::InvalidCacheFormat)?;
    if routes_end != payload.len() {
        return Err(RouteBundleError::InvalidCacheFormat);
    }
    Ok(RawRouteBundle {
        manifest: payload[24..manifest_end].to_vec(),
        signature: payload[manifest_end..signature_end].to_vec(),
        routes: payload[signature_end..routes_end].to_vec(),
    })
}

fn read_u32(bytes: &[u8]) -> Result<u32, RouteBundleError> {
    let value: [u8; 4] = bytes
        .try_into()
        .map_err(|_| RouteBundleError::InvalidCacheFormat)?;
    Ok(u32::from_be_bytes(value))
}

fn read_u64(bytes: &[u8]) -> Result<u64, RouteBundleError> {
    let value: [u8; 8] = bytes
        .try_into()
        .map_err(|_| RouteBundleError::InvalidCacheFormat)?;
    Ok(u64::from_be_bytes(value))
}

fn create_private_cache_directory(path: &Path) -> Result<(), RouteBundleError> {
    fs::create_dir_all(path).map_err(|_| RouteBundleError::CacheIo)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| RouteBundleError::CacheIo)?;
    }
    Ok(())
}

fn write_private_file(path: &Path, payload: &[u8]) -> Result<(), std::io::Error> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(payload)?;
    file.sync_all()
}

fn cleanup_old_cache_generations(cache_directory: &Path) {
    let Ok(entries) = fs::read_dir(cache_directory) else {
        return;
    };
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        if entry.path().extension().and_then(|value| value.to_str()) != Some(CACHE_EXTENSION) {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        candidates.push((modified, entry.path()));
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
    for (_, path) in candidates.into_iter().skip(MAX_CACHE_GENERATIONS) {
        let _ = fs::remove_file(path);
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use chacha20poly1305::aead::Aead;
    use ed25519_dalek::pkcs8::EncodePublicKey;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn verifies_decrypts_and_reloads_only_encrypted_cache() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let plaintext = b"hysteria2://secret@example.com:443#US-test";
        let (bundle, verifier) = test_bundle(plaintext, Utc::now());

        let payload = verifier
            .verify(&bundle, Utc::now())
            .expect("verified route bundle");
        assert_eq!(payload.as_bytes(), plaintext);

        save_cached_bundle(directory.path(), &bundle).expect("encrypted cache");
        let cached = load_latest_verified_cache(directory.path(), &verifier, Utc::now())
            .expect("verified cache");
        assert_eq!(cached.as_bytes(), plaintext);
        let cache_path = fs::read_dir(directory.path())
            .expect("cache directory")
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.extension().and_then(|value| value.to_str()) == Some(CACHE_EXTENSION))
            .expect("cache file");
        let bytes = fs::read(cache_path).expect("cache bytes");
        assert!(bytes.starts_with(CACHE_MAGIC));
        assert!(!bytes
            .windows(plaintext.len())
            .any(|window| window == plaintext));
    }

    #[test]
    fn rejects_incompatible_cache_magic() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let cache_path = directory.path().join("route-legacy.bundle");
        let mut payload = b"LEGACY00".to_vec();
        payload.extend_from_slice(&[0_u8; 16]);
        fs::write(&cache_path, payload).expect("legacy cache fixture");

        assert!(matches!(
            read_cached_bundle(&cache_path),
            Err(RouteBundleError::InvalidCacheFormat)
        ));
    }

    #[test]
    fn rejects_pre_v2_route_magic() {
        let (mut bundle, verifier) = test_bundle(b"route payload", Utc::now());
        bundle.routes[..ROUTE_MAGIC.len()].copy_from_slice(b"LEGACY01");

        assert!(matches!(
            decrypt_routes(&bundle.routes, &verifier.encryption_key),
            Err(RouteBundleError::InvalidRouteFormat)
        ));
    }

    #[tokio::test]
    async fn falls_back_to_a_fully_reverified_cache_on_connection_failure() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let cache_directory = directory.path().join("routes");
        let plaintext = b"hysteria2://secret@example.com:443#US-test";
        let (bundle, _) = test_bundle(plaintext, Utc::now());
        save_cached_bundle(&cache_directory, &bundle).expect("encrypted cache");
        let client = RouteBundleClient::new(
            Url::parse("https://127.0.0.1:1/public/manifest.json").expect("manifest URL"),
            &test_public_key_pem(),
            Zeroizing::new([29_u8; 32]),
            "v1".to_owned(),
            cache_directory,
        )
        .expect("route client")
        .with_timeout(Duration::from_millis(100));

        let payload = client
            .fetch_payload()
            .await
            .expect("verified cache fallback");

        assert_eq!(payload.as_bytes(), plaintext);
    }

    #[tokio::test]
    async fn tries_multiple_remote_sources_before_verified_cache() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let cache_directory = directory.path().join("routes");
        let plaintext = b"hysteria2://secret@example.com:443#US-test";
        let (bundle, _) = test_bundle(plaintext, Utc::now());
        save_cached_bundle(&cache_directory, &bundle).expect("encrypted cache");
        let client = RouteBundleClient::new_with_fallbacks(
            vec![
                Url::parse("https://127.0.0.1:1/public/manifest.json")
                    .expect("primary manifest URL"),
                Url::parse("https://127.0.0.1:2/public/manifest.json")
                    .expect("fallback manifest URL"),
            ],
            &test_public_key_pem(),
            Zeroizing::new([29_u8; 32]),
            "v1".to_owned(),
            cache_directory,
        )
        .expect("route client")
        .with_timeout(Duration::from_millis(100));

        let payload = client
            .fetch_payload()
            .await
            .expect("verified cache fallback");

        assert_eq!(payload.as_bytes(), plaintext);
    }

    #[test]
    fn rejects_manifest_tampering_hash_mismatch_and_wrong_key() {
        let (mut bundle, verifier) = test_bundle(b"route payload", Utc::now());
        bundle.manifest.push(b' ');
        assert!(matches!(
            verifier.verify(&bundle, Utc::now()),
            Err(RouteBundleError::InvalidSignature)
        ));

        let (mut bundle, verifier) = test_bundle(b"route payload", Utc::now());
        let last = bundle.routes.len() - 1;
        bundle.routes[last] ^= 1;
        assert!(matches!(
            verifier.verify(&bundle, Utc::now()),
            Err(RouteBundleError::RouteHashMismatch)
        ));

        let (bundle, mut verifier) = test_bundle(b"route payload", Utc::now());
        verifier.encryption_key = Zeroizing::new([44_u8; 32]);
        assert!(matches!(
            verifier.verify(&bundle, Utc::now()),
            Err(RouteBundleError::DecryptionFailed)
        ));
    }

    #[test]
    fn rejects_expired_future_and_unexpected_key_id_manifests() {
        let now = Utc::now();
        let (expired, verifier) = test_bundle(b"route payload", now - ChronoDuration::hours(80));
        assert!(matches!(
            verifier.verify(&expired, now),
            Err(RouteBundleError::Expired)
        ));

        let (future, verifier) = test_bundle(b"route payload", now + ChronoDuration::minutes(6));
        assert!(matches!(
            verifier.verify(&future, now),
            Err(RouteBundleError::GeneratedInFuture)
        ));

        let (bundle, mut verifier) = test_bundle(b"route payload", now);
        verifier.expected_key_id = "v2".to_owned();
        assert!(matches!(
            verifier.verify(&bundle, now),
            Err(RouteBundleError::KeyIdMismatch)
        ));
    }

    #[test]
    fn rejects_pre_v2_manifest_schema() {
        let (bundle, _) = test_bundle(b"route payload", Utc::now());
        let mut manifest: RouteManifest =
            serde_json::from_slice(&bundle.manifest).expect("manifest fixture");
        manifest.schema_version = 1;

        assert!(matches!(
            validate_manifest(&manifest, "v1", Utc::now()),
            Err(RouteBundleError::UnsupportedSchema)
        ));
    }

    #[test]
    fn accepts_only_fixed_https_manifest_urls() {
        for invalid in [
            "http://example.com/public/manifest.json",
            "https://user@example.com/public/manifest.json",
            "https://example.com/public/manifest.json?token=secret",
            "https://example.com/public/routes.enc",
        ] {
            let url = Url::parse(invalid).expect("URL");
            assert!(matches!(
                validate_manifest_url(&url),
                Err(RouteBundleError::InvalidManifestUrl)
            ));
        }
        assert!(validate_manifest_url(
            &Url::parse("https://raw.githubusercontent.com/owner/repo/main/public/manifest.json")
                .expect("manifest URL")
        )
        .is_ok());
    }

    #[test]
    fn rejects_empty_or_invalid_fallback_source_lists() {
        let directory = tempfile::tempdir().expect("temporary directory");
        assert!(matches!(
            RouteBundleClient::new_with_fallbacks(
                Vec::new(),
                &test_public_key_pem(),
                Zeroizing::new([29_u8; 32]),
                "v1".to_owned(),
                directory.path().join("empty"),
            ),
            Err(RouteBundleError::InvalidManifestUrl)
        ));
        assert!(matches!(
            RouteBundleClient::new_with_fallbacks(
                vec![
                    Url::parse(
                        "https://gitee.com/lyq_power/dadaapi-routes/raw/main/public/manifest.json",
                    )
                    .expect("Gitee URL"),
                    Url::parse("http://example.com/public/manifest.json")
                        .expect("invalid fallback URL"),
                ],
                &test_public_key_pem(),
                Zeroizing::new([29_u8; 32]),
                "v1".to_owned(),
                directory.path().join("invalid"),
            ),
            Err(RouteBundleError::InvalidManifestUrl)
        ));
    }

    #[test]
    fn fallback_policy_excludes_integrity_and_client_errors() {
        assert!(RouteBundleError::Connect {
            resource: BundleResource::Manifest
        }
        .permits_cache_fallback());
        assert!(RouteBundleError::HttpStatus {
            resource: BundleResource::Routes,
            status: 503
        }
        .permits_cache_fallback());
        assert!(!RouteBundleError::HttpStatus {
            resource: BundleResource::Routes,
            status: 404
        }
        .permits_cache_fallback());
        assert!(!RouteBundleError::InvalidSignature.permits_cache_fallback());
        assert!(!RouteBundleError::RouteHashMismatch.permits_cache_fallback());
        assert!(!RouteBundleError::DecryptionFailed.permits_cache_fallback());
    }

    #[cfg(unix)]
    #[test]
    fn cache_directory_and_files_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().expect("temporary directory");
        let cache_directory = directory.path().join("routes");
        let (bundle, _) = test_bundle(b"route payload", Utc::now());
        save_cached_bundle(&cache_directory, &bundle).expect("encrypted cache");
        let directory_mode = fs::metadata(&cache_directory)
            .expect("cache metadata")
            .permissions()
            .mode()
            & 0o777;
        let cache_path = fs::read_dir(&cache_directory)
            .expect("cache directory")
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.extension().and_then(|value| value.to_str()) == Some(CACHE_EXTENSION))
            .expect("cache file");
        let file_mode = fs::metadata(cache_path)
            .expect("file metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(directory_mode, 0o700);
        assert_eq!(file_mode, 0o600);
    }

    fn test_bundle(
        plaintext: &[u8],
        generated_at: DateTime<Utc>,
    ) -> (RawRouteBundle, RouteBundleVerifier) {
        let signing_key = SigningKey::from_bytes(&[17_u8; 32]);
        let encryption_key = [29_u8; 32];
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&encryption_key));
        let nonce = [7_u8; 24];
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: ROUTE_AAD,
                },
            )
            .expect("encrypted fixture");
        let mut routes = Vec::new();
        routes.extend_from_slice(ROUTE_MAGIC);
        routes.extend_from_slice(&nonce);
        routes.extend_from_slice(&ciphertext);
        let manifest = RouteManifest {
            schema_version: 2,
            version: "test-v2".to_owned(),
            generated_at,
            expires_at: generated_at + ChronoDuration::hours(72),
            route_file: ROUTE_FILE_NAME.to_owned(),
            route_sha256: encode_hex(&Sha256::digest(&routes)),
            route_size: routes.len() as u64,
            encryption: "xchacha20poly1305".to_owned(),
            key_id: "v1".to_owned(),
        };
        let mut manifest = serde_json::to_vec_pretty(&manifest).expect("manifest JSON");
        manifest.push(b'\n');
        let signature = format!(
            "{}\n",
            STANDARD.encode(signing_key.sign(&manifest).to_bytes())
        )
        .into_bytes();
        let public_key_pem = test_public_key_pem();
        let public_key =
            VerifyingKey::from_public_key_pem(&public_key_pem).expect("public key PEM");
        (
            RawRouteBundle {
                manifest,
                signature,
                routes,
            },
            RouteBundleVerifier {
                public_key,
                encryption_key: Zeroizing::new(encryption_key),
                expected_key_id: "v1".to_owned(),
            },
        )
    }

    fn test_public_key_pem() -> String {
        let public_key_der = SigningKey::from_bytes(&[17_u8; 32])
            .verifying_key()
            .to_public_key_der()
            .expect("public key DER");
        format!(
            "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
            STANDARD.encode(public_key_der.as_bytes())
        )
    }
}
