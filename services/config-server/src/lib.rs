use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::{Signer, SigningKey};
use rustls::pki_types::CertificateDer;
use rustls::RootCertStore;
use serde::Serialize;
use shared_types::{
    ClientConfig, CommandError, HealthResponse, SignedClientConfig, SubscriptionEndpoint,
};
use std::env;
use std::fmt;
use std::fs;
use std::net::IpAddr;
use std::sync::Arc;
use thiserror::Error;
use tower_http::trace::TraceLayer;
use url::Url;
use uuid::Uuid;

const DEFAULT_CONFIG_LIFETIME_MINUTES: i64 = 15;

#[derive(Debug, Error)]
pub enum PublisherError {
    #[error("配置签名私钥读取失败：{0}")]
    KeyFile(#[from] std::io::Error),
    #[error("配置签名私钥格式无效")]
    InvalidSigningKey,
    #[error("订阅接口地址无效")]
    InvalidSubscriptionEndpointUrl,
    #[error("订阅接口标识无效")]
    InvalidEndpointId,
    #[error("客户端配置版本不能为空")]
    EmptyVersion,
    #[error("客户端配置序列化失败：{0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct PublisherSettings {
    pub version: String,
    pub endpoint_id: Uuid,
    pub subscription_endpoint_url: Url,
    pub subscription_endpoint_certificate_der_base64: Option<String>,
    pub lifetime: ChronoDuration,
}

impl PublisherSettings {
    pub fn new(
        version: String,
        endpoint_id: Uuid,
        subscription_endpoint_url: Url,
        subscription_endpoint_certificate_der_base64: Option<String>,
    ) -> Result<Self, PublisherError> {
        if version.trim().is_empty() {
            return Err(PublisherError::EmptyVersion);
        }
        validate_subscription_endpoint_url(&subscription_endpoint_url)?;
        validate_subscription_endpoint_certificate(
            &subscription_endpoint_url,
            subscription_endpoint_certificate_der_base64.as_deref(),
        )?;
        Ok(Self {
            version,
            endpoint_id,
            subscription_endpoint_url,
            subscription_endpoint_certificate_der_base64,
            lifetime: ChronoDuration::minutes(DEFAULT_CONFIG_LIFETIME_MINUTES),
        })
    }
}

#[derive(Clone)]
pub struct ConfigPublisher {
    signing_key: Arc<SigningKey>,
    settings: PublisherSettings,
}

impl ConfigPublisher {
    #[must_use]
    pub fn new(signing_key: SigningKey, settings: PublisherSettings) -> Self {
        Self {
            signing_key: Arc::new(signing_key),
            settings,
        }
    }

    pub fn from_environment() -> Result<Option<Self>, PublisherError> {
        let signing_key_pem = match (
            env::var("CONFIG_SIGNING_KEY_PEM").ok(),
            env::var("CONFIG_SIGNING_KEY_FILE").ok(),
        ) {
            (Some(pem), _) if !pem.trim().is_empty() => pem,
            (_, Some(path)) if !path.trim().is_empty() => fs::read_to_string(path)?,
            _ => return Ok(None),
        };
        let subscription_endpoint_url = env::var("SUBSCRIPTION_ENDPOINT_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or(PublisherError::InvalidSubscriptionEndpointUrl)?;
        let endpoint_id = env::var("SUBSCRIPTION_ENDPOINT_ID")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or(PublisherError::InvalidEndpointId)?;
        let version = env::var("CLIENT_CONFIG_VERSION")
            .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned());
        let subscription_endpoint_certificate_der_base64 =
            env::var("SUBSCRIPTION_ENDPOINT_CERT_DER_BASE64")
                .ok()
                .filter(|value| !value.trim().is_empty());
        let signing_key = SigningKey::from_pkcs8_pem(&signing_key_pem)
            .map_err(|_| PublisherError::InvalidSigningKey)?;
        let subscription_endpoint_url = Url::parse(&subscription_endpoint_url)
            .map_err(|_| PublisherError::InvalidSubscriptionEndpointUrl)?;
        let endpoint_id =
            Uuid::parse_str(&endpoint_id).map_err(|_| PublisherError::InvalidEndpointId)?;
        let settings = PublisherSettings::new(
            version,
            endpoint_id,
            subscription_endpoint_url,
            subscription_endpoint_certificate_der_base64,
        )?;
        Ok(Some(Self::new(signing_key, settings)))
    }

    pub fn publish(&self, now: DateTime<Utc>) -> Result<SignedClientConfig, PublisherError> {
        let payload = ClientConfig {
            version: self.settings.version.clone(),
            generated_at: now,
            expires_at: Some(now + self.settings.lifetime),
            subscription_endpoints: vec![SubscriptionEndpoint {
                id: self.settings.endpoint_id,
                url: self.settings.subscription_endpoint_url.clone(),
                tls_certificate_der_base64: self
                    .settings
                    .subscription_endpoint_certificate_der_base64
                    .clone(),
            }],
        };
        let bytes = serde_json::to_vec(&payload)?;
        let signature = self.signing_key.sign(&bytes);
        Ok(SignedClientConfig {
            payload,
            signature: STANDARD.encode(signature.to_bytes()),
        })
    }

    #[must_use]
    pub fn version(&self) -> &str {
        &self.settings.version
    }
}

impl fmt::Debug for ConfigPublisher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigPublisher")
            .field("signing_key", &"[REDACTED]")
            .field("settings", &self.settings)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct AppState {
    publisher: Option<ConfigPublisher>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigVersionResponse {
    version: String,
}

pub fn router(publisher: Option<ConfigPublisher>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/client/config", get(client_config))
        .route("/v1/client/config/version", get(config_version))
        .with_state(AppState { publisher })
        .layer(TraceLayer::new_for_http())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "wocao-hub-config-server".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

async fn client_config(
    State(state): State<AppState>,
) -> Result<Json<SignedClientConfig>, (StatusCode, Json<CommandError>)> {
    let publisher = state.publisher.as_ref().ok_or_else(config_unavailable)?;
    publisher
        .publish(Utc::now())
        .map(Json)
        .map_err(|_| internal_error())
}

async fn config_version(
    State(state): State<AppState>,
) -> Result<Json<ConfigVersionResponse>, (StatusCode, Json<CommandError>)> {
    let publisher = state.publisher.as_ref().ok_or_else(config_unavailable)?;
    Ok(Json(ConfigVersionResponse {
        version: publisher.version().to_owned(),
    }))
}

fn config_unavailable() -> (StatusCode, Json<CommandError>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(CommandError {
            code: "CONFIG_UNAVAILABLE".to_owned(),
            message: "客户端配置尚未启用".to_owned(),
        }),
    )
}

fn internal_error() -> (StatusCode, Json<CommandError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(CommandError {
            code: "CONFIG_PUBLISH_FAILED".to_owned(),
            message: "客户端配置发布失败".to_owned(),
        }),
    )
}

fn validate_subscription_endpoint_url(url: &Url) -> Result<(), PublisherError> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/v1/client/subscription"
    {
        return Err(PublisherError::InvalidSubscriptionEndpointUrl);
    }
    Ok(())
}

fn validate_subscription_endpoint_certificate(
    url: &Url,
    encoded_certificate: Option<&str>,
) -> Result<(), PublisherError> {
    let host_is_ip = url
        .host_str()
        .is_some_and(|host| host.parse::<IpAddr>().is_ok());
    let Some(encoded_certificate) = encoded_certificate else {
        return if host_is_ip {
            Err(PublisherError::InvalidSubscriptionEndpointUrl)
        } else {
            Ok(())
        };
    };
    let certificate = STANDARD
        .decode(encoded_certificate.as_bytes())
        .map_err(|_| PublisherError::InvalidSubscriptionEndpointUrl)?;
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(certificate))
        .map_err(|_| PublisherError::InvalidSubscriptionEndpointUrl)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use remote_config::ConfigVerifier;

    #[test]
    fn published_config_is_accepted_by_desktop_verifier() {
        let signing_key = SigningKey::from_bytes(&[9_u8; 32]);
        let verifier = ConfigVerifier::new(signing_key.verifying_key());
        let publisher = ConfigPublisher::new(signing_key, settings());
        let now = Utc::now();

        let signed = publisher.publish(now).expect("signed config");
        let verified = verifier.verify(&signed, now).expect("verified config");

        assert_eq!(verified.version, "2026.07.10.1");
        assert_eq!(verified.subscription_endpoints.len(), 1);
        assert_eq!(
            verified.subscription_endpoints[0].url.as_str(),
            "https://subscription.example.com/v1/client/subscription"
        );
        assert!(!format!("{publisher:?}").contains("SigningKey"));
    }

    #[test]
    fn publisher_rejects_insecure_subscription_endpoint_url() {
        let result = PublisherSettings::new(
            "1".to_owned(),
            Uuid::new_v4(),
            Url::parse("http://subscription.example.com/v1/client/subscription")
                .expect("valid URL"),
            None,
        );

        assert!(matches!(
            result,
            Err(PublisherError::InvalidSubscriptionEndpointUrl)
        ));
    }

    fn settings() -> PublisherSettings {
        PublisherSettings::new(
            "2026.07.10.1".to_owned(),
            Uuid::new_v4(),
            Url::parse("https://subscription.example.com/v1/client/subscription")
                .expect("valid URL"),
            None,
        )
        .expect("publisher settings")
    }

    #[test]
    fn ip_subscription_endpoint_requires_pinned_certificate() {
        let url = Url::parse("https://192.0.2.10:18443/v1/client/subscription").expect("valid URL");
        let missing = PublisherSettings::new("1".to_owned(), Uuid::new_v4(), url.clone(), None);
        let certified = rcgen::generate_simple_self_signed(vec!["192.0.2.10".to_owned()])
            .expect("self-signed certificate");
        let pinned = PublisherSettings::new(
            "1".to_owned(),
            Uuid::new_v4(),
            url,
            Some(STANDARD.encode(certified.cert.der().as_ref())),
        );

        assert!(missing.is_err());
        assert!(pinned.is_ok());
    }
}
