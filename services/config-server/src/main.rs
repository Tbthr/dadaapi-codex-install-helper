use anyhow::Context;
use config_server::{router, ConfigPublisher};
use std::{env, net::SocketAddr};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let address = env::var("WOCAO_HUB_BIND").unwrap_or_else(|_| "127.0.0.1:18081".to_owned());
    let address: SocketAddr = address.parse().context("invalid WOCAO_HUB_BIND")?;
    let publisher = ConfigPublisher::from_environment()?;
    let listener = TcpListener::bind(address).await?;
    tracing::info!(%address, config_enabled = publisher.is_some(), "configuration server listening");

    axum::serve(listener, router(publisher)).await?;
    Ok(())
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .try_init();
}
