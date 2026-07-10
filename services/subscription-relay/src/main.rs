use anyhow::{bail, Context};
use axum_server::tls_rustls::RustlsConfig;
use route_catalog::{SecretSubscriptionUrl, SubscriptionClient};
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use subscription_relay::{refresh_once, router, run_refresh_loop, RelayState};
use url::Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let settings = Settings::from_environment()?;
    let source = SubscriptionClient::new(SecretSubscriptionUrl::new(settings.subscription_url)?)?;
    let state = RelayState::default();
    match refresh_once(&state, &source).await {
        Ok(()) => tracing::info!("initial subscription loaded"),
        Err(error) => {
            tracing::warn!(error = %error, "initial subscription load failed; endpoint will return 503");
        }
    }
    tokio::spawn(run_refresh_loop(
        state.clone(),
        source,
        settings.refresh_interval,
    ));
    let tls = RustlsConfig::from_pem_file(settings.certificate_file, settings.private_key_file)
        .await
        .context("invalid subscription relay TLS certificate or key")?;
    tracing::info!(address = %settings.bind, "subscription relay listening with TLS");
    axum_server::bind_rustls(settings.bind, tls)
        .serve(router(state).into_make_service())
        .await?;
    Ok(())
}

struct Settings {
    bind: SocketAddr,
    subscription_url: Url,
    certificate_file: PathBuf,
    private_key_file: PathBuf,
    refresh_interval: Duration,
}

impl Settings {
    fn from_environment() -> anyhow::Result<Self> {
        let bind = env::var("WOCAO_SUBSCRIPTION_BIND")
            .unwrap_or_else(|_| "0.0.0.0:18443".to_owned())
            .parse()
            .context("invalid WOCAO_SUBSCRIPTION_BIND")?;
        let subscription_url = required_env("SUBSCRIPTION_URL")?
            .parse()
            .context("invalid SUBSCRIPTION_URL")?;
        let certificate_file = PathBuf::from(required_env("SUBSCRIPTION_TLS_CERT_FILE")?);
        let private_key_file = PathBuf::from(required_env("SUBSCRIPTION_TLS_KEY_FILE")?);
        let refresh_seconds = env::var("SUBSCRIPTION_REFRESH_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(3600);
        if refresh_seconds < 30 {
            bail!("SUBSCRIPTION_REFRESH_SECONDS must be at least 30");
        }
        Ok(Self {
            bind,
            subscription_url,
            certificate_file,
            private_key_file,
            refresh_interval: Duration::from_secs(refresh_seconds),
        })
    }
}

fn required_env(name: &str) -> anyhow::Result<String> {
    let value = env::var(name).unwrap_or_default();
    if value.trim().is_empty() {
        bail!("missing required environment variable {name}");
    }
    Ok(value)
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .try_init();
}
