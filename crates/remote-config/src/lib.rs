use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use ed25519_dalek::pkcs8::DecodePublicKey;
use ed25519_dalek::{Signature, VerifyingKey};
use reqwest::Client;
use rustls::pki_types::CertificateDer;
use rustls::RootCertStore;
use serde::{Deserialize, Serialize};
use shared_types::{ClientConfig, SignedClientConfig};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use url::Url;

const CONFIG_CACHE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_CLOCK_SKEW: ChronoDuration = ChronoDuration::minutes(5);

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("配置请求失败：{0}")]
    Request(#[from] reqwest::Error),
    #[error("配置签名公钥无效")]
    InvalidPublicKey,
    #[error("配置签名编码无效")]
    InvalidSignatureEncoding,
    #[error("配置签名验证失败")]
    InvalidSignature,
    #[error("配置已经过期")]
    Expired,
    #[error("配置生成时间超出允许的时钟偏差")]
    GeneratedInFuture,
    #[error("配置内容无效：{0}")]
    InvalidConfig(String),
    #[error("配置缓存路径无效")]
    InvalidCachePath,
    #[error("配置缓存文件操作失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("配置 JSON 格式无效：{0}")]
    Json(#[from] serde_json::Error),
    #[error("不支持的配置缓存版本：{0}")]
    UnsupportedCacheVersion(u32),
}

impl ConfigError {
    #[must_use]
    pub fn permits_cache_fallback(&self) -> bool {
        match self {
            Self::Request(error) => {
                error.is_timeout()
                    || error.is_connect()
                    || error
                        .status()
                        .is_some_and(|status| status.is_server_error())
            }
            Self::InvalidPublicKey
            | Self::InvalidSignatureEncoding
            | Self::InvalidSignature
            | Self::Expired
            | Self::GeneratedInFuture
            | Self::InvalidConfig(_)
            | Self::InvalidCachePath
            | Self::Io(_)
            | Self::Json(_)
            | Self::UnsupportedCacheVersion(_) => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigClient {
    client: Client,
    endpoint: Url,
    timeout: Duration,
}

impl ConfigClient {
    #[must_use]
    pub fn new(endpoint: Url) -> Self {
        Self {
            client: Client::new(),
            endpoint,
            timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub async fn fetch(&self) -> Result<SignedClientConfig, ConfigError> {
        let response = self
            .client
            .get(self.endpoint.clone())
            .timeout(self.timeout)
            .send()
            .await?;
        Ok(response.error_for_status()?.json().await?)
    }

    pub async fn fetch_verified(
        &self,
        verifier: &ConfigVerifier,
    ) -> Result<ClientConfig, ConfigError> {
        let signed = self.fetch().await?;
        verifier.verify(&signed, Utc::now())
    }
}

#[derive(Debug, Clone)]
pub struct ConfigVerifier {
    public_key: VerifyingKey,
}

impl ConfigVerifier {
    #[must_use]
    pub fn new(public_key: VerifyingKey) -> Self {
        Self { public_key }
    }

    pub fn from_public_key_pem(pem: &str) -> Result<Self, ConfigError> {
        let public_key =
            VerifyingKey::from_public_key_pem(pem).map_err(|_| ConfigError::InvalidPublicKey)?;
        Ok(Self { public_key })
    }

    pub fn verify(
        &self,
        signed: &SignedClientConfig,
        now: DateTime<Utc>,
    ) -> Result<ClientConfig, ConfigError> {
        let signature = STANDARD
            .decode(signed.signature.as_bytes())
            .map_err(|_| ConfigError::InvalidSignatureEncoding)?;
        let signature =
            Signature::from_slice(&signature).map_err(|_| ConfigError::InvalidSignatureEncoding)?;
        let payload = serde_json::to_vec(&signed.payload)?;
        self.public_key
            .verify_strict(&payload, &signature)
            .map_err(|_| ConfigError::InvalidSignature)?;
        validate_config(&signed.payload, now)?;
        Ok(signed.payload.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignedConfigCache {
    schema_version: u32,
    cached_at: DateTime<Utc>,
    config: SignedClientConfig,
}

pub fn save_signed_config_cache(
    path: &Path,
    config: &SignedClientConfig,
) -> Result<(), ConfigError> {
    let cache = SignedConfigCache {
        schema_version: CONFIG_CACHE_SCHEMA_VERSION,
        cached_at: Utc::now(),
        config: config.clone(),
    };
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
    Ok(())
}

pub fn load_verified_config_cache(
    path: &Path,
    verifier: &ConfigVerifier,
    now: DateTime<Utc>,
) -> Result<ClientConfig, ConfigError> {
    let cache: SignedConfigCache = serde_json::from_slice(&fs::read(path)?)?;
    if cache.schema_version != CONFIG_CACHE_SCHEMA_VERSION {
        return Err(ConfigError::UnsupportedCacheVersion(cache.schema_version));
    }
    verifier.verify(&cache.config, now)
}

fn validate_config(config: &ClientConfig, now: DateTime<Utc>) -> Result<(), ConfigError> {
    if config.version.trim().is_empty() {
        return Err(ConfigError::InvalidConfig("版本不能为空".to_owned()));
    }
    if config.generated_at > now + MAX_CLOCK_SKEW {
        return Err(ConfigError::GeneratedInFuture);
    }
    if config
        .expires_at
        .is_some_and(|expires_at| expires_at <= now)
    {
        return Err(ConfigError::Expired);
    }
    if config.subscription_endpoints.is_empty() {
        return Err(ConfigError::InvalidConfig(
            "至少需要一个订阅接口".to_owned(),
        ));
    }
    let mut identifiers = HashSet::new();
    for endpoint in &config.subscription_endpoints {
        if !identifiers.insert(endpoint.id) {
            return Err(ConfigError::InvalidConfig(
                "订阅接口标识不能重复".to_owned(),
            ));
        }
        if endpoint.url.scheme() != "https"
            || endpoint.url.host_str().is_none()
            || endpoint.url.username() != ""
            || endpoint.url.password().is_some()
            || endpoint.url.query().is_some()
            || endpoint.url.fragment().is_some()
            || endpoint.url.path() != "/v1/client/subscription"
        {
            return Err(ConfigError::InvalidConfig(
                "订阅接口必须是无内嵌凭据的固定 HTTPS 地址".to_owned(),
            ));
        }
        validate_endpoint_certificate(endpoint)?;
    }
    Ok(())
}

fn validate_endpoint_certificate(
    endpoint: &shared_types::SubscriptionEndpoint,
) -> Result<(), ConfigError> {
    let host_is_ip = endpoint
        .url
        .host_str()
        .is_some_and(|host| host.parse::<IpAddr>().is_ok());
    let Some(encoded_certificate) = endpoint.tls_certificate_der_base64.as_deref() else {
        if host_is_ip {
            return Err(ConfigError::InvalidConfig(
                "IP 订阅接口必须提供固定 TLS 证书".to_owned(),
            ));
        }
        return Ok(());
    };
    let certificate = STANDARD
        .decode(encoded_certificate.as_bytes())
        .map_err(|_| ConfigError::InvalidConfig("固定 TLS 证书编码无效".to_owned()))?;
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(certificate))
        .map_err(|_| ConfigError::InvalidConfig("固定 TLS 证书格式无效".to_owned()))?;
    Ok(())
}

fn cache_temporary_path(path: &Path, parent: &Path) -> Result<PathBuf, ConfigError> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(ConfigError::InvalidCachePath)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use shared_types::SubscriptionEndpoint;
    use uuid::Uuid;

    #[test]
    fn verifies_valid_signed_config_and_rejects_tampering() {
        let signing_key = signing_key();
        let verifier = ConfigVerifier::new(signing_key.verifying_key());
        let now = Utc::now();
        let mut signed = signed_config(&signing_key, now);

        assert!(verifier.verify(&signed, now).is_ok());
        signed.payload.version = "tampered".to_owned();
        assert!(matches!(
            verifier.verify(&signed, now),
            Err(ConfigError::InvalidSignature)
        ));
    }

    #[test]
    fn rejects_expired_or_insecure_config_after_signature_verification() {
        let signing_key = signing_key();
        let verifier = ConfigVerifier::new(signing_key.verifying_key());
        let now = Utc::now();
        let mut expired_payload = client_config(now);
        expired_payload.expires_at = Some(now - ChronoDuration::seconds(1));
        let expired = sign_payload(&signing_key, expired_payload);
        let mut insecure_payload = client_config(now);
        insecure_payload.subscription_endpoints[0].url =
            Url::parse("http://subscription.example.com/v1/client/subscription")
                .expect("valid URL");
        let insecure = sign_payload(&signing_key, insecure_payload);
        let mut empty_payload = client_config(now);
        empty_payload.subscription_endpoints.clear();
        let empty = sign_payload(&signing_key, empty_payload);

        assert!(matches!(
            verifier.verify(&expired, now),
            Err(ConfigError::Expired)
        ));
        assert!(matches!(
            verifier.verify(&insecure, now),
            Err(ConfigError::InvalidConfig(_))
        ));
        assert!(matches!(
            verifier.verify(&empty, now),
            Err(ConfigError::InvalidConfig(_))
        ));
    }

    #[test]
    fn signed_cache_is_private_and_reverified_on_load() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("client-config.json");
        let signing_key = signing_key();
        let verifier = ConfigVerifier::new(signing_key.verifying_key());
        let now = Utc::now();
        let signed = signed_config(&signing_key, now);

        save_signed_config_cache(&path, &signed).expect("save signed cache");
        let loaded =
            load_verified_config_cache(&path, &verifier, now).expect("load verified cache");

        assert_eq!(loaded.version, signed.payload.version);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(path)
                .expect("cache metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    fn signed_config(signing_key: &SigningKey, now: DateTime<Utc>) -> SignedClientConfig {
        sign_payload(signing_key, client_config(now))
    }

    fn sign_payload(signing_key: &SigningKey, payload: ClientConfig) -> SignedClientConfig {
        let bytes = serde_json::to_vec(&payload).expect("serialize payload");
        let signature = signing_key.sign(&bytes);
        SignedClientConfig {
            payload,
            signature: STANDARD.encode(signature.to_bytes()),
        }
    }

    fn client_config(now: DateTime<Utc>) -> ClientConfig {
        ClientConfig {
            version: "2026.07.10.1".to_owned(),
            generated_at: now,
            expires_at: Some(now + ChronoDuration::hours(1)),
            subscription_endpoints: vec![SubscriptionEndpoint {
                id: Uuid::new_v4(),
                url: Url::parse("https://subscription.example.com/v1/client/subscription")
                    .expect("valid URL"),
                tls_certificate_der_base64: None,
            }],
        }
    }

    #[test]
    fn accepts_ip_subscription_endpoint_only_with_valid_pinned_certificate() {
        let signing_key = signing_key();
        let verifier = ConfigVerifier::new(signing_key.verifying_key());
        let now = Utc::now();
        let mut missing_pin = client_config(now);
        missing_pin.subscription_endpoints[0].url =
            Url::parse("https://192.0.2.10:18443/v1/client/subscription").expect("valid URL");
        let missing_pin = sign_payload(&signing_key, missing_pin);
        let certified = rcgen::generate_simple_self_signed(vec!["192.0.2.10".to_owned()])
            .expect("self-signed certificate");
        let mut pinned = client_config(now);
        pinned.subscription_endpoints[0].url =
            Url::parse("https://192.0.2.10:18443/v1/client/subscription").expect("valid URL");
        pinned.subscription_endpoints[0].tls_certificate_der_base64 =
            Some(STANDARD.encode(certified.cert.der().as_ref()));
        let pinned = sign_payload(&signing_key, pinned);

        assert!(matches!(
            verifier.verify(&missing_pin, now),
            Err(ConfigError::InvalidConfig(_))
        ));
        assert!(verifier.verify(&pinned, now).is_ok());
    }

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7_u8; 32])
    }
}
