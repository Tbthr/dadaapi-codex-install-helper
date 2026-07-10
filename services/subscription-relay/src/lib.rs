use axum::body::Body;
use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderValue, Response, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use route_catalog::{SubscriptionClient, SubscriptionFetchError, SubscriptionPayload};
use shared_types::{CommandError, HealthResponse};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tower_http::trace::TraceLayer;

#[derive(Debug, Clone, Default)]
pub struct RelayState {
    payload: Arc<RwLock<Option<SubscriptionPayload>>>,
}

impl RelayState {
    pub fn replace(&self, payload: SubscriptionPayload) -> Result<(), RelayStateError> {
        let mut current = self
            .payload
            .write()
            .map_err(|_| RelayStateError::Unavailable)?;
        *current = Some(payload);
        Ok(())
    }

    pub fn current(&self) -> Result<Option<SubscriptionPayload>, RelayStateError> {
        self.payload
            .read()
            .map(|payload| payload.clone())
            .map_err(|_| RelayStateError::Unavailable)
    }

    #[must_use]
    pub fn available(&self) -> bool {
        self.current().ok().flatten().is_some()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RelayStateError {
    #[error("订阅中继状态不可用")]
    Unavailable,
}

pub fn router(state: RelayState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/client/subscription", get(subscription))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

pub async fn refresh_once(
    state: &RelayState,
    client: &SubscriptionClient,
) -> Result<(), RefreshError> {
    let payload = client.fetch_payload().await?;
    state.replace(payload)?;
    Ok(())
}

pub async fn run_refresh_loop(
    state: RelayState,
    client: SubscriptionClient,
    refresh_interval: Duration,
) {
    let mut interval = tokio::time::interval(refresh_interval.max(Duration::from_secs(30)));
    interval.tick().await;
    loop {
        interval.tick().await;
        match refresh_once(&state, &client).await {
            Ok(()) => tracing::info!("subscription refreshed"),
            Err(error) => {
                tracing::warn!(error = %error, "subscription refresh failed; retaining last good payload");
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    #[error(transparent)]
    Fetch(#[from] SubscriptionFetchError),
    #[error(transparent)]
    State(#[from] RelayStateError),
}

async fn health(State(state): State<RelayState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: state.available(),
        service: "wocao-hub-subscription-relay".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

async fn subscription(
    State(state): State<RelayState>,
) -> Result<Response<Body>, (StatusCode, Json<CommandError>)> {
    let payload = state
        .current()
        .map_err(|_| internal_error())?
        .ok_or_else(subscription_unavailable)?;
    let mut response = Response::new(Body::from(payload.to_vec()));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

fn subscription_unavailable() -> (StatusCode, Json<CommandError>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(CommandError {
            code: "SUBSCRIPTION_UNAVAILABLE".to_owned(),
            message: "订阅内容暂时不可用".to_owned(),
        }),
    )
}

fn internal_error() -> (StatusCode, Json<CommandError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(CommandError {
            code: "SUBSCRIPTION_STATE_UNAVAILABLE".to_owned(),
            message: "订阅中继状态不可用".to_owned(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_server::tls_rustls::RustlsConfig;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use route_catalog::{RemoteSubscriptionClient, RouteCatalog};
    use shared_types::SubscriptionEndpoint;
    use std::net::TcpListener;
    use url::Url;
    use uuid::Uuid;

    const TEST_NODE: &str = "vless://00000000-0000-0000-0000-000000000001@example.com:443#US-test";

    #[tokio::test]
    async fn pinned_tls_client_fetches_and_parses_subscription_locally() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let certified = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()])
            .expect("self-signed certificate");
        let certificate_der = certified.cert.der().as_ref().to_vec();
        let tls = RustlsConfig::from_der(
            vec![certificate_der.clone()],
            certified.key_pair.serialize_der(),
        )
        .await
        .expect("TLS config");
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("TLS listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking TLS listener");
        let address = listener.local_addr().expect("TLS address");
        let state = RelayState::default();
        state
            .replace(SubscriptionPayload::new(TEST_NODE.as_bytes().to_vec()))
            .expect("relay payload");
        let server = axum_server::from_tcp_rustls(listener, tls)
            .expect("TLS server")
            .serve(router(state).into_make_service());
        let server_task = tokio::spawn(server);

        let endpoint = SubscriptionEndpoint {
            id: Uuid::new_v4(),
            url: Url::parse(&format!("https://{address}/v1/client/subscription"))
                .expect("endpoint URL"),
            tls_certificate_der_base64: Some(STANDARD.encode(certificate_der)),
        };
        let client = RemoteSubscriptionClient::new(endpoint).expect("remote client");
        let catalog = RouteCatalog::default();
        let snapshot = client
            .refresh_catalog(&catalog)
            .await
            .expect("local route catalog");

        assert_eq!(snapshot.routes.len(), 1);
        assert_eq!(snapshot.routes[0].name, "US-test");
        server_task.abort();
    }
}
