use crate::{
    parse_node_connection_config, BoxedNodeStream, ConnectTarget, Hysteria2NodeConnector,
    LocalProxyError, NodeConnectionConfig, NodeConnector, ProxyNode, ProxyProtocol,
    VlessNodeConnector, VlessSecurity, VlessTransport,
};
use async_trait::async_trait;

#[derive(Debug, Clone, Default)]
pub struct EmbeddedNodeConnector {
    hysteria2: Hysteria2NodeConnector,
    vless: VlessNodeConnector,
}

impl EmbeddedNodeConnector {
    #[must_use]
    pub fn supports(&self, protocol: ProxyProtocol) -> bool {
        matches!(protocol, ProxyProtocol::Hysteria2 | ProxyProtocol::Vless)
    }

    #[must_use]
    pub fn supports_node(&self, node: &ProxyNode) -> bool {
        match parse_node_connection_config(node) {
            Ok(NodeConnectionConfig::Hysteria2(_)) => true,
            Ok(NodeConnectionConfig::Vless(config)) => {
                matches!(config.transport, VlessTransport::Tcp)
                    && config.security != VlessSecurity::Reality
                    && config.flow.as_deref().is_none_or(str::is_empty)
            }
            Err(_) => false,
        }
    }
}

#[async_trait]
impl NodeConnector for EmbeddedNodeConnector {
    async fn connect(
        &self,
        node: &ProxyNode,
        target: &ConnectTarget,
    ) -> Result<BoxedNodeStream, LocalProxyError> {
        match node.protocol {
            ProxyProtocol::Hysteria2 => self.hysteria2.connect(node, target).await,
            ProxyProtocol::Vless => self.vless.connect(node, target).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeRegion, SecretNodeUri};

    #[test]
    fn only_reports_protocols_with_real_embedded_implementations() {
        let connector = EmbeddedNodeConnector::default();

        assert!(connector.supports(ProxyProtocol::Hysteria2));
        assert!(connector.supports(ProxyProtocol::Vless));
    }

    #[test]
    fn rejects_reality_nodes_until_the_real_transport_is_available() {
        let connector = EmbeddedNodeConnector::default();
        let node = ProxyNode {
            index: 1,
            name: "US Reality".to_owned(),
            protocol: ProxyProtocol::Vless,
            region: NodeRegion::UnitedStates,
            server: "example.com".to_owned(),
            port: 443,
            uri: SecretNodeUri(
                "vless://00000000-0000-0000-0000-000000000001@example.com:443?security=reality&type=tcp&sni=www.example.com&pbk=secret#US-Reality".to_owned(),
            ),
        };

        assert!(!connector.supports_node(&node));
    }
}
