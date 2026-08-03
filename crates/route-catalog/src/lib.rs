use chrono::{DateTime, Utc};
use proxy_core::{NodeRegion, ParsedSubscription, ProxyNode, ProxyProtocol};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use uuid::Uuid;

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
    use proxy_core::parse_subscription;

    const US_NODE: &str =
        "vless://00000000-0000-0000-0000-000000000001@example.com:443?security=reality#US-test";
    const JP_NODE: &str = "hysteria2://password@example.net:8443?sni=example.net#JP-test";

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
}
