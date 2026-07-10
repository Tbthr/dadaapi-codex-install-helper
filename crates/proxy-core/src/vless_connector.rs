use crate::{
    parse_node_connection_config, BoxedNodeStream, ConnectTarget, LocalProxyError,
    NodeConnectionConfig, NodeConnector, ProxyNode, VlessNodeConfig, VlessSecurity, VlessTransport,
};
use async_trait::async_trait;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use std::fmt;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

#[derive(Debug, Clone, Copy, Default)]
pub struct VlessNodeConnector;

#[async_trait]
impl NodeConnector for VlessNodeConnector {
    async fn connect(
        &self,
        node: &ProxyNode,
        target: &ConnectTarget,
    ) -> Result<BoxedNodeStream, LocalProxyError> {
        let parsed = parse_node_connection_config(node)
            .map_err(|_| connector_error("VLESS 节点配置无效"))?;
        let NodeConnectionConfig::Vless(config) = parsed else {
            return Err(connector_error("节点协议不是 VLESS"));
        };
        validate_supported_profile(&config)?;
        let mut stream = connect_transport(&config).await?;
        let header = encode_vless_request(&config, target)?;
        stream
            .write_all(&header)
            .await
            .map_err(|_| connector_error("VLESS 握手写入失败"))?;
        stream
            .flush()
            .await
            .map_err(|_| connector_error("VLESS 握手发送失败"))?;
        Ok(Box::new(VlessClientStream::new(stream)))
    }
}

fn validate_supported_profile(config: &VlessNodeConfig) -> Result<(), LocalProxyError> {
    if !matches!(config.transport, VlessTransport::Tcp) {
        return Err(connector_error("当前版本暂不支持该 VLESS 传输方式"));
    }
    if config.security == VlessSecurity::Reality {
        return Err(connector_error("当前版本尚未启用 VLESS Reality"));
    }
    if config.flow.as_deref().is_some_and(|flow| !flow.is_empty()) {
        return Err(connector_error("当前版本尚未启用 VLESS Vision"));
    }
    Ok(())
}

async fn connect_transport(config: &VlessNodeConfig) -> Result<BoxedNodeStream, LocalProxyError> {
    let tcp = TcpStream::connect(server_authority(&config.server, config.port))
        .await
        .map_err(|_| connector_error("无法连接 VLESS 节点"))?;
    if config.security == VlessSecurity::None {
        return Ok(Box::new(tcp));
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client_config = tls_client_config(config.allow_insecure);
    let server_name = config
        .server_name
        .as_deref()
        .unwrap_or(config.server.as_str())
        .to_owned();
    let server_name = ServerName::try_from(server_name)
        .map_err(|_| connector_error("VLESS TLS 服务器名称无效"))?;
    let stream = TlsConnector::from(Arc::new(client_config))
        .connect(server_name, tcp)
        .await
        .map_err(|_| connector_error("VLESS TLS 握手失败"))?;
    Ok(Box::new(stream))
}

fn tls_client_config(allow_insecure: bool) -> ClientConfig {
    if allow_insecure {
        return ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoCertificateVerification))
            .with_no_client_auth();
    }
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

fn encode_vless_request(
    config: &VlessNodeConfig,
    target: &ConnectTarget,
) -> Result<Vec<u8>, LocalProxyError> {
    let mut header = Vec::with_capacity(64 + target.host.len());
    header.push(0);
    header.extend_from_slice(config.user_id.as_bytes());
    header.push(0);
    header.push(1);
    header.extend_from_slice(&target.port.to_be_bytes());
    match target.host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            header.push(1);
            header.extend_from_slice(&ip.octets());
        }
        Ok(IpAddr::V6(ip)) => {
            header.push(3);
            header.extend_from_slice(&ip.octets());
        }
        Err(_) => {
            let domain = target.host.as_bytes();
            let domain_length =
                u8::try_from(domain.len()).map_err(|_| connector_error("VLESS 目标域名过长"))?;
            if domain_length == 0 {
                return Err(connector_error("VLESS 目标域名无效"));
            }
            header.push(2);
            header.push(domain_length);
            header.extend_from_slice(domain);
        }
    }
    Ok(header)
}

fn server_authority(server: &str, port: u16) -> String {
    server
        .parse::<IpAddr>()
        .map(|ip| SocketAddr::new(ip, port).to_string())
        .unwrap_or_else(|_| format!("{server}:{port}"))
}

fn connector_error(message: &str) -> LocalProxyError {
    LocalProxyError::Start(message.to_owned())
}

#[derive(Debug)]
struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _certificate: &CertificateDer<'_>,
        _signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _certificate: &CertificateDer<'_>,
        _signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        use SignatureScheme::{
            ECDSA_NISTP256_SHA256, ECDSA_NISTP384_SHA384, ECDSA_NISTP521_SHA512, ED25519, ED448,
            RSA_PKCS1_SHA256, RSA_PKCS1_SHA384, RSA_PKCS1_SHA512, RSA_PSS_SHA256, RSA_PSS_SHA384,
            RSA_PSS_SHA512,
        };
        vec![
            RSA_PKCS1_SHA256,
            ECDSA_NISTP256_SHA256,
            RSA_PKCS1_SHA384,
            ECDSA_NISTP384_SHA384,
            RSA_PKCS1_SHA512,
            ECDSA_NISTP521_SHA512,
            RSA_PSS_SHA256,
            RSA_PSS_SHA384,
            RSA_PSS_SHA512,
            ED25519,
            ED448,
        ]
    }
}

enum ResponseState {
    Header { bytes: [u8; 2], read: usize },
    Addon { remaining: usize },
    Ready,
}

struct VlessClientStream {
    inner: BoxedNodeStream,
    response: ResponseState,
}

impl VlessClientStream {
    fn new(inner: BoxedNodeStream) -> Self {
        Self {
            inner,
            response: ResponseState::Header {
                bytes: [0; 2],
                read: 0,
            },
        }
    }

    fn poll_response_header(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        loop {
            match &mut self.response {
                ResponseState::Header { bytes, read } => {
                    let mut buffer = ReadBuf::new(&mut bytes[*read..]);
                    match Pin::new(&mut self.inner).poll_read(cx, &mut buffer) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                        Poll::Ready(Ok(())) => {
                            let amount = buffer.filled().len();
                            if amount == 0 {
                                return Poll::Ready(Err(io::Error::new(
                                    io::ErrorKind::UnexpectedEof,
                                    "VLESS 服务器在响应前关闭连接",
                                )));
                            }
                            *read += amount;
                            if *read < bytes.len() {
                                continue;
                            }
                            if bytes[0] != 0 {
                                return Poll::Ready(Err(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    "VLESS 响应版本无效",
                                )));
                            }
                            self.response = if bytes[1] == 0 {
                                ResponseState::Ready
                            } else {
                                ResponseState::Addon {
                                    remaining: usize::from(bytes[1]),
                                }
                            };
                        }
                    }
                }
                ResponseState::Addon { remaining } => {
                    let mut discard = [0_u8; 256];
                    let amount = (*remaining).min(discard.len());
                    let mut buffer = ReadBuf::new(&mut discard[..amount]);
                    match Pin::new(&mut self.inner).poll_read(cx, &mut buffer) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                        Poll::Ready(Ok(())) => {
                            let read = buffer.filled().len();
                            if read == 0 {
                                return Poll::Ready(Err(io::Error::new(
                                    io::ErrorKind::UnexpectedEof,
                                    "VLESS 响应附加数据不完整",
                                )));
                            }
                            *remaining -= read;
                            if *remaining == 0 {
                                self.response = ResponseState::Ready;
                            }
                        }
                    }
                }
                ResponseState::Ready => return Poll::Ready(Ok(())),
            }
        }
    }
}

impl fmt::Debug for VlessClientStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VlessClientStream([REDACTED])")
    }
}

impl AsyncRead for VlessClientStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.poll_response_header(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => Pin::new(&mut self.inner).poll_read(cx, buffer),
        }
    }
}

impl AsyncWrite for VlessClientStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeRegion, ProxyProtocol, SecretNodeUri};
    use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;

    const USER_ID: &str = "00000000-0000-0000-0000-000000000001";

    #[test]
    fn encodes_vless_domain_header_exactly() {
        let config = vless_config(VlessSecurity::None, false);
        let header = encode_vless_request(
            &config,
            &ConnectTarget {
                host: "chatgpt.com".to_owned(),
                port: 443,
            },
        )
        .expect("VLESS header");

        assert_eq!(header[0], 0);
        assert_eq!(&header[1..17], config.user_id.as_bytes());
        assert_eq!(header[17], 0);
        assert_eq!(header[18], 1);
        assert_eq!(&header[19..21], &443_u16.to_be_bytes());
        assert_eq!(header[21], 2);
        assert_eq!(header[22], 11);
        assert_eq!(&header[23..], b"chatgpt.com");
    }

    #[tokio::test]
    async fn plain_vless_connector_forwards_bidirectional_stream() {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("VLESS listener");
        let address = listener.local_addr().expect("VLESS address");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("VLESS connection");
            let mut fixed = [0_u8; 23];
            stream
                .read_exact(&mut fixed)
                .await
                .expect("VLESS fixed header");
            let domain_length = usize::from(fixed[22]);
            let mut domain = vec![0_u8; domain_length];
            stream.read_exact(&mut domain).await.expect("VLESS domain");
            assert_eq!(&domain, b"chatgpt.com");
            stream.write_all(&[0, 0]).await.expect("VLESS response");
            let mut payload = [0_u8; 9];
            stream
                .read_exact(&mut payload)
                .await
                .expect("VLESS payload");
            stream.write_all(&payload).await.expect("VLESS echo");
        });
        let node = test_node(address, "");
        let target = ConnectTarget {
            host: "chatgpt.com".to_owned(),
            port: 443,
        };
        let mut stream = VlessNodeConnector
            .connect(&node, &target)
            .await
            .expect("VLESS stream");

        stream.write_all(b"wocao-hub").await.expect("payload write");
        let mut echoed = [0_u8; 9];
        stream.read_exact(&mut echoed).await.expect("payload read");
        assert_eq!(&echoed, b"wocao-hub");
        server.await.expect("VLESS server");
    }

    #[tokio::test]
    async fn tls_vless_connector_supports_explicit_insecure_nodes() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
            .expect("self-signed certificate");
        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![certified.cert.der().clone()],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der())),
            )
            .expect("TLS server config");
        let acceptor = TlsAcceptor::from(Arc::new(server_config));
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("TLS VLESS listener");
        let address = listener.local_addr().expect("TLS VLESS address");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("TLS VLESS connection");
            let mut stream = acceptor.accept(stream).await.expect("TLS accept");
            let mut fixed = [0_u8; 23];
            stream
                .read_exact(&mut fixed)
                .await
                .expect("VLESS fixed header");
            let mut domain = vec![0_u8; usize::from(fixed[22])];
            stream.read_exact(&mut domain).await.expect("VLESS domain");
            stream.write_all(&[0, 0]).await.expect("VLESS response");
            let mut payload = [0_u8; 9];
            stream
                .read_exact(&mut payload)
                .await
                .expect("VLESS payload");
            stream.write_all(&payload).await.expect("VLESS echo");
        });
        let node = test_node(address, "?security=tls&sni=localhost&insecure=1");
        let target = ConnectTarget {
            host: "chatgpt.com".to_owned(),
            port: 443,
        };
        let mut stream = VlessNodeConnector
            .connect(&node, &target)
            .await
            .expect("TLS VLESS stream");

        stream.write_all(b"wocao-hub").await.expect("payload write");
        let mut echoed = [0_u8; 9];
        stream.read_exact(&mut echoed).await.expect("payload read");
        assert_eq!(&echoed, b"wocao-hub");
        server.await.expect("TLS VLESS server");
    }

    #[test]
    fn reality_and_vision_are_not_reported_as_basic_vless() {
        let mut reality = vless_config(VlessSecurity::Tls, false);
        reality.security = VlessSecurity::Reality;
        reality.server_name = Some("www.example.com".to_owned());
        assert!(validate_supported_profile(&reality).is_err());

        let mut vision = vless_config(VlessSecurity::Tls, false);
        vision.flow = Some("xtls-rprx-vision".to_owned());
        assert!(validate_supported_profile(&vision).is_err());
    }

    fn vless_config(security: VlessSecurity, allow_insecure: bool) -> VlessNodeConfig {
        let node = test_node(
            SocketAddr::from(([127, 0, 0, 1], 443)),
            &format!("?security={}", security_name(security)),
        );
        let NodeConnectionConfig::Vless(mut config) =
            parse_node_connection_config(&node).expect("VLESS config")
        else {
            panic!("expected VLESS");
        };
        config.allow_insecure = allow_insecure;
        config
    }

    fn security_name(security: VlessSecurity) -> &'static str {
        match security {
            VlessSecurity::None => "none",
            VlessSecurity::Tls => "tls",
            VlessSecurity::Reality => "reality",
        }
    }

    fn test_node(address: SocketAddr, query: &str) -> ProxyNode {
        let uri = format!(
            "vless://{USER_ID}@{}:{}{query}#US-test",
            address.ip(),
            address.port()
        );
        ProxyNode {
            index: 1,
            name: "US test".to_owned(),
            protocol: ProxyProtocol::Vless,
            region: NodeRegion::UnitedStates,
            server: address.ip().to_string(),
            port: address.port(),
            uri: SecretNodeUri(uri),
        }
    }
}
