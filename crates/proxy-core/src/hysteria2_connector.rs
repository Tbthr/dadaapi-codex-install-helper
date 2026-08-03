use crate::{
    parse_node_connection_config, BoxedNodeStream, ConnectTarget, Hysteria2NodeConfig,
    LocalProxyError, NodeConnectionConfig, NodeConnector, ProxyNode,
};
use async_trait::async_trait;
use rsteria2::{Config, ReconnectableClient};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::sync::Mutex;

const MAX_CACHED_CLIENTS: usize = 16;

#[derive(Clone, Default)]
pub struct Hysteria2NodeConnector {
    clients: Arc<Mutex<HashMap<[u8; 32], Arc<ReconnectableClient>>>>,
}

impl Hysteria2NodeConnector {
    async fn client_for(
        &self,
        node: &ProxyNode,
    ) -> Result<Arc<ReconnectableClient>, LocalProxyError> {
        let parsed = parse_node_connection_config(node)
            .map_err(|_| connector_error("Hysteria2 节点配置无效"))?;
        let NodeConnectionConfig::Hysteria2(config) = parsed else {
            return Err(connector_error("节点协议不是 Hysteria2"));
        };
        let key: [u8; 32] = Sha256::digest(node.uri.expose().as_bytes()).into();
        let mut clients = self.clients.lock().await;
        if let Some(client) = clients.get(&key) {
            return Ok(client.clone());
        }
        if clients.len() >= MAX_CACHED_CLIENTS {
            clients.clear();
        }
        let client = Arc::new(ReconnectableClient::new(to_rsteria_config(&config)));
        clients.insert(key, client.clone());
        Ok(client)
    }
}

impl fmt::Debug for Hysteria2NodeConnector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Hysteria2NodeConnector")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl NodeConnector for Hysteria2NodeConnector {
    async fn connect(
        &self,
        node: &ProxyNode,
        target: &ConnectTarget,
    ) -> Result<BoxedNodeStream, LocalProxyError> {
        let client = self.client_for(node).await?;
        let target = target_authority(target);
        let stream = client.tcp_connect(&target).await.map_err(|error| {
            tracing::debug!(
                category = hysteria_error_category(&error),
                "Hysteria2 node connection failed"
            );
            connector_error("Hysteria2 节点连接失败")
        })?;
        Ok(Box::new(stream))
    }
}

fn hysteria_error_category(error: &rsteria2::Error) -> &'static str {
    match error {
        rsteria2::Error::Io(_) => "io",
        rsteria2::Error::Config(_) => "config",
        rsteria2::Error::Tls(_) => "tls",
        rsteria2::Error::Connect(_) => "quic_connect",
        rsteria2::Error::Connection(error) => quinn_connection_error_category(error),
        rsteria2::Error::H3Connection(_) => "h3_connection",
        rsteria2::Error::H3Stream(_) => "h3_stream",
        rsteria2::Error::AuthFailed(_) => "auth_failed",
        rsteria2::Error::TcpRejected(_) => "tcp_rejected",
        rsteria2::Error::Protocol(_) => "protocol",
    }
}

fn quinn_connection_error_category(error: &quinn::ConnectionError) -> &'static str {
    match error {
        quinn::ConnectionError::VersionMismatch => "quic_version_mismatch",
        quinn::ConnectionError::TransportError(_) => "quic_transport",
        quinn::ConnectionError::ConnectionClosed(_) => "quic_peer_transport_close",
        quinn::ConnectionError::ApplicationClosed(_) => "quic_peer_application_close",
        quinn::ConnectionError::Reset => "quic_reset",
        quinn::ConnectionError::TimedOut => "quic_timed_out",
        quinn::ConnectionError::LocallyClosed => "quic_locally_closed",
        quinn::ConnectionError::CidsExhausted => "quic_cids_exhausted",
    }
}

fn to_rsteria_config(config: &Hysteria2NodeConfig) -> Config {
    let certificate_is_pinned = config.certificate_fingerprint.is_some();
    Config {
        server_addr: server_authority(&config.server, config.port),
        server_name: config.server_name.clone(),
        auth: config.authentication.expose().to_owned(),
        // A pinned Hysteria2 certificate is commonly self-signed. rsteria2's
        // `insecure + pin` mode still verifies the exact SHA-256 fingerprint,
        // while avoiding an additional public-CA requirement that would reject
        // a valid pinned certificate.
        insecure: config.allow_insecure || certificate_is_pinned,
        obfs_password: config
            .obfuscation_password
            .as_ref()
            .map(|value| value.expose().to_owned())
            .unwrap_or_default(),
        hop_ports: config.port_hopping.clone().unwrap_or_default(),
        pin_sha256: config.certificate_fingerprint.clone().unwrap_or_default(),
        fast_open: true,
        ..Config::default()
    }
}

fn server_authority(server: &str, port: u16) -> String {
    server
        .parse::<IpAddr>()
        .map(|ip| SocketAddr::new(ip, port).to_string())
        .unwrap_or_else(|_| format!("{server}:{port}"))
}

fn target_authority(target: &ConnectTarget) -> String {
    server_authority(&target.host, target.port)
}

fn connector_error(message: &str) -> LocalProxyError {
    LocalProxyError::Start(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeRegion, ProxyProtocol, SecretNodeUri};

    #[test]
    fn maps_hysteria2_profile_to_embedded_client_config() {
        let node = test_node();
        let parsed = parse_node_connection_config(&node).expect("node config");
        let NodeConnectionConfig::Hysteria2(parsed) = parsed else {
            panic!("expected Hysteria2");
        };
        let config = to_rsteria_config(&parsed);

        assert_eq!(config.server_addr, "example.net:8443");
        assert_eq!(config.server_name, "edge.example.net");
        assert_eq!(config.obfs_password, "secret-obfs");
        assert_eq!(config.hop_ports, "20000-30000");
        assert_eq!(config.pin_sha256, "aabbccdd");
        assert!(config.insecure);
    }

    #[test]
    fn formats_ipv6_targets_with_brackets() {
        assert_eq!(server_authority("2001:db8::1", 443), "[2001:db8::1]:443");
    }

    #[test]
    fn connector_debug_does_not_expose_cached_credentials() {
        let connector = Hysteria2NodeConnector::default();
        assert_eq!(format!("{connector:?}"), "Hysteria2NodeConnector { .. }");
    }

    #[test]
    fn classifies_hysteria_errors_without_rendering_details() {
        let error = rsteria2::Error::Config("sensitive-value".to_owned());
        assert_eq!(hysteria_error_category(&error), "config");
        let timeout = rsteria2::Error::Connection(quinn::ConnectionError::TimedOut);
        assert_eq!(hysteria_error_category(&timeout), "quic_timed_out");
    }

    fn test_node() -> ProxyNode {
        let uri = "hysteria2://secret-auth@example.net:8443?sni=edge.example.net&obfs=salamander&obfs-password=secret-obfs&mport=20000-30000&pinSHA256=aabbccdd";
        ProxyNode {
            index: 1,
            name: "US test".to_owned(),
            protocol: ProxyProtocol::Hysteria2,
            region: NodeRegion::UnitedStates,
            server: "example.net".to_owned(),
            port: 8443,
            uri: SecretNodeUri(uri.to_owned()),
        }
    }
}
