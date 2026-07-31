use async_trait::async_trait;
use chrono::Utc;
use proxy_core::{
    save_proxy_selection_cache, DirectNodeSelectionReport, DirectNodeSelector,
    DirectSelectionError, DirectSelectionOptions, ProxyNode,
};
use route_bundle::{RouteBundleClient, RouteBundleError};
use route_catalog::{RouteCatalog, RouteCatalogError, SubscriptionPayload};
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
        preflight_timeout: Duration::from_secs(3),
        selection_timeout: Duration::from_secs(15),
        candidate_limit: 8,
    })
}

#[derive(Debug, Error)]
pub enum ProxyPreparationError {
    #[error(transparent)]
    RouteBundle(#[from] RouteBundleError),
    #[error("代理测速缓存后台任务失败：{0}")]
    CacheTask(String),
    #[error("路由包中没有可用节点")]
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
