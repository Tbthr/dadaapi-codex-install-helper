use crate::{ProxyNode, ProxyProtocol};
use percent_encoding::percent_decode_str;
use std::fmt;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

#[derive(Clone)]
pub struct SecretValue(String);

impl SecretValue {
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

#[derive(Clone)]
pub struct SecretUuid([u8; 16]);

impl SecretUuid {
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl fmt::Debug for SecretUuid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretUuid([REDACTED])")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VlessSecurity {
    None,
    Tls,
    Reality,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VlessTransport {
    Tcp,
    WebSocket { path: String, host: Option<String> },
    Grpc { service_name: String },
}

#[derive(Debug, Clone)]
pub struct VlessNodeConfig {
    pub server: String,
    pub port: u16,
    pub user_id: SecretUuid,
    pub security: VlessSecurity,
    pub transport: VlessTransport,
    pub server_name: Option<String>,
    pub flow: Option<String>,
    pub client_fingerprint: Option<String>,
    pub reality_public_key: Option<SecretValue>,
    pub reality_short_id: Option<SecretValue>,
    pub reality_spider_x: Option<String>,
    pub allow_insecure: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hysteria2Obfuscation {
    Salamander,
}

#[derive(Debug, Clone)]
pub struct Hysteria2NodeConfig {
    pub server: String,
    pub port: u16,
    pub authentication: SecretValue,
    pub server_name: String,
    pub allow_insecure: bool,
    pub obfuscation: Option<Hysteria2Obfuscation>,
    pub obfuscation_password: Option<SecretValue>,
    pub port_hopping: Option<String>,
    pub certificate_fingerprint: Option<String>,
}

#[derive(Debug, Clone)]
pub enum NodeConnectionConfig {
    Vless(VlessNodeConfig),
    Hysteria2(Hysteria2NodeConfig),
}

#[derive(Debug, Error)]
pub enum NodeConfigError {
    #[error("节点连接配置无效")]
    Invalid,
    #[error("节点连接方式暂不支持")]
    UnsupportedTransport,
    #[error("节点安全配置暂不支持")]
    UnsupportedSecurity,
    #[error("Reality 节点缺少必要参数")]
    IncompleteReality,
    #[error("Hysteria2 混淆配置无效")]
    InvalidHysteria2Obfuscation,
}

pub fn parse_node_connection_config(
    node: &ProxyNode,
) -> Result<NodeConnectionConfig, NodeConfigError> {
    let url = Url::parse(node.uri.expose()).map_err(|_| NodeConfigError::Invalid)?;
    if url.host_str() != Some(node.server.as_str()) || url.port() != Some(node.port) {
        return Err(NodeConfigError::Invalid);
    }
    match node.protocol {
        ProxyProtocol::Vless if url.scheme().eq_ignore_ascii_case("vless") => {
            parse_vless_config(&url).map(NodeConnectionConfig::Vless)
        }
        ProxyProtocol::Hysteria2
            if matches!(
                url.scheme().to_ascii_lowercase().as_str(),
                "hysteria2" | "hy2"
            ) =>
        {
            parse_hysteria2_config(&url).map(NodeConnectionConfig::Hysteria2)
        }
        _ => Err(NodeConfigError::Invalid),
    }
}

fn parse_vless_config(url: &Url) -> Result<VlessNodeConfig, NodeConfigError> {
    let server = url.host_str().ok_or(NodeConfigError::Invalid)?.to_owned();
    let port = url.port().ok_or(NodeConfigError::Invalid)?;
    let user_id = Uuid::parse_str(url.username()).map_err(|_| NodeConfigError::Invalid)?;
    let security = match query_value(url, "security")
        .unwrap_or_else(|| "none".to_owned())
        .to_ascii_lowercase()
        .as_str()
    {
        "none" | "" => VlessSecurity::None,
        "tls" => VlessSecurity::Tls,
        "reality" => VlessSecurity::Reality,
        _ => return Err(NodeConfigError::UnsupportedSecurity),
    };
    let transport_name = query_value(url, "type")
        .unwrap_or_else(|| "tcp".to_owned())
        .to_ascii_lowercase();
    let transport = match transport_name.as_str() {
        "tcp" | "raw" => VlessTransport::Tcp,
        "ws" | "websocket" => VlessTransport::WebSocket {
            path: query_value(url, "path").unwrap_or_else(|| "/".to_owned()),
            host: query_value(url, "host"),
        },
        "grpc" => VlessTransport::Grpc {
            service_name: query_value(url, "serviceName")
                .or_else(|| query_value(url, "service_name"))
                .unwrap_or_default(),
        },
        _ => return Err(NodeConfigError::UnsupportedTransport),
    };
    let server_name = query_value(url, "sni").or_else(|| query_value(url, "serverName"));
    let reality_public_key = query_value(url, "pbk").map(SecretValue);
    if security == VlessSecurity::Reality
        && (reality_public_key.is_none() || server_name.as_deref().unwrap_or_default().is_empty())
    {
        return Err(NodeConfigError::IncompleteReality);
    }
    Ok(VlessNodeConfig {
        server,
        port,
        user_id: SecretUuid(*user_id.as_bytes()),
        security,
        transport,
        server_name,
        flow: query_value(url, "flow").filter(|value| !value.is_empty()),
        client_fingerprint: query_value(url, "fp").filter(|value| !value.is_empty()),
        reality_public_key,
        reality_short_id: query_value(url, "sid")
            .filter(|value| !value.is_empty())
            .map(SecretValue),
        reality_spider_x: query_value(url, "spx").filter(|value| !value.is_empty()),
        allow_insecure: insecure_enabled(url),
    })
}

fn parse_hysteria2_config(url: &Url) -> Result<Hysteria2NodeConfig, NodeConfigError> {
    let server = url.host_str().ok_or(NodeConfigError::Invalid)?.to_owned();
    let port = url.port().ok_or(NodeConfigError::Invalid)?;
    let mut authentication = decode_component(url.username())?;
    if let Some(password) = url.password() {
        authentication.push(':');
        authentication.push_str(&decode_component(password)?);
    }
    if authentication.is_empty() {
        return Err(NodeConfigError::Invalid);
    }
    let obfuscation = match query_value(url, "obfs")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "none" => None,
        "salamander" => Some(Hysteria2Obfuscation::Salamander),
        _ => return Err(NodeConfigError::InvalidHysteria2Obfuscation),
    };
    let obfuscation_password = query_value(url, "obfs-password")
        .or_else(|| query_value(url, "obfsPassword"))
        .filter(|value| !value.is_empty())
        .map(SecretValue);
    if obfuscation.is_some() && obfuscation_password.is_none() {
        return Err(NodeConfigError::InvalidHysteria2Obfuscation);
    }
    Ok(Hysteria2NodeConfig {
        server: server.clone(),
        port,
        authentication: SecretValue(authentication),
        server_name: query_value(url, "sni").unwrap_or(server),
        allow_insecure: insecure_enabled(url),
        obfuscation,
        obfuscation_password,
        port_hopping: query_value(url, "mport")
            .or_else(|| query_value(url, "ports"))
            .filter(|value| !value.is_empty()),
        certificate_fingerprint: query_value(url, "pinSHA256")
            .or_else(|| query_value(url, "pin_sha256"))
            .filter(|value| !value.is_empty()),
    })
}

fn query_value(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.into_owned())
}

fn insecure_enabled(url: &Url) -> bool {
    ["insecure", "allowInsecure"]
        .into_iter()
        .filter_map(|key| query_value(url, key))
        .any(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

fn decode_component(value: &str) -> Result<String, NodeConfigError> {
    percent_decode_str(value)
        .decode_utf8()
        .map(|value| value.into_owned())
        .map_err(|_| NodeConfigError::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeRegion, SecretNodeUri};

    #[test]
    fn parses_reality_vless_without_exposing_credentials() {
        let node = node(
            ProxyProtocol::Vless,
            "vless://00000000-0000-0000-0000-000000000001@example.com:443?security=reality&type=tcp&sni=www.example.com&fp=chrome&pbk=secret-public-key&sid=0123456789abcdef&flow=xtls-rprx-vision",
        );
        let config = parse_node_connection_config(&node).expect("VLESS config");
        let NodeConnectionConfig::Vless(config) = config else {
            panic!("expected VLESS config");
        };

        assert_eq!(config.security, VlessSecurity::Reality);
        assert_eq!(config.transport, VlessTransport::Tcp);
        assert_eq!(config.server_name.as_deref(), Some("www.example.com"));
        let debug = format!("{config:?}");
        assert!(!debug.contains("00000000-0000-0000-0000-000000000001"));
        assert!(!debug.contains("secret-public-key"));
    }

    #[test]
    fn parses_hysteria2_salamander_and_redacts_authentication() {
        let node = node(
            ProxyProtocol::Hysteria2,
            "hysteria2://secret-password@example.net:8443?sni=edge.example.net&obfs=salamander&obfs-password=secret-obfs&mport=20000-30000",
        );
        let config = parse_node_connection_config(&node).expect("Hysteria2 config");
        let NodeConnectionConfig::Hysteria2(config) = config else {
            panic!("expected Hysteria2 config");
        };

        assert_eq!(config.obfuscation, Some(Hysteria2Obfuscation::Salamander));
        assert_eq!(config.port_hopping.as_deref(), Some("20000-30000"));
        let debug = format!("{config:?}");
        assert!(!debug.contains("secret-password"));
        assert!(!debug.contains("secret-obfs"));
    }

    #[test]
    fn rejects_unknown_vless_transport() {
        let node = node(
            ProxyProtocol::Vless,
            "vless://00000000-0000-0000-0000-000000000001@example.com:443?type=kcp",
        );

        assert!(matches!(
            parse_node_connection_config(&node),
            Err(NodeConfigError::UnsupportedTransport)
        ));
    }

    fn node(protocol: ProxyProtocol, uri: &str) -> ProxyNode {
        let parsed = Url::parse(uri).expect("node URL");
        ProxyNode {
            index: 1,
            name: "US test".to_owned(),
            protocol,
            region: NodeRegion::UnitedStates,
            server: parsed.host_str().expect("host").to_owned(),
            port: parsed.port().expect("port"),
            uri: SecretNodeUri(uri.to_owned()),
        }
    }
}
