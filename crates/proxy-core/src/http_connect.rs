use crate::{LocalProxyEngine, LocalProxyError, LocalProxySession, ProxyNode};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{copy_bidirectional, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::timeout;

const DEFAULT_MAX_HEADER_BYTES: usize = 16 * 1024;
const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

pub trait NodeStream: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T> NodeStream for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

pub type BoxedNodeStream = Box<dyn NodeStream>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectTarget {
    pub host: String,
    pub port: u16,
}

#[async_trait]
pub trait NodeConnector: Send + Sync + 'static {
    async fn connect(
        &self,
        node: &ProxyNode,
        target: &ConnectTarget,
    ) -> Result<BoxedNodeStream, LocalProxyError>;
}

#[derive(Debug, Clone)]
pub struct HttpConnectProxyEngine<C> {
    connector: Arc<C>,
    max_header_bytes: usize,
    handshake_timeout: Duration,
    connect_timeout: Duration,
    shutdown_timeout: Duration,
}

impl<C> HttpConnectProxyEngine<C>
where
    C: NodeConnector,
{
    #[must_use]
    pub fn new(connector: C) -> Self {
        Self {
            connector: Arc::new(connector),
            max_header_bytes: DEFAULT_MAX_HEADER_BYTES,
            handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            shutdown_timeout: DEFAULT_SHUTDOWN_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with_timeouts(
        mut self,
        handshake_timeout: Duration,
        connect_timeout: Duration,
        shutdown_timeout: Duration,
    ) -> Self {
        self.handshake_timeout = handshake_timeout;
        self.connect_timeout = connect_timeout;
        self.shutdown_timeout = shutdown_timeout;
        self
    }
}

#[async_trait]
impl<C> LocalProxyEngine for HttpConnectProxyEngine<C>
where
    C: NodeConnector,
{
    async fn start(&self, node: &ProxyNode) -> Result<Box<dyn LocalProxySession>, LocalProxyError> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|error| LocalProxyError::Start(error.to_string()))?;
        let endpoint = listener
            .local_addr()
            .map_err(|error| LocalProxyError::Start(error.to_string()))?;
        let (ready_tx, ready_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let connector = self.connector.clone();
        let node = Arc::new(node.clone());
        let max_header_bytes = self.max_header_bytes;
        let handshake_timeout = self.handshake_timeout;
        let connect_timeout = self.connect_timeout;
        let task = tokio::spawn(async move {
            run_accept_loop(
                listener,
                connector,
                node,
                max_header_bytes,
                handshake_timeout,
                connect_timeout,
                ready_tx,
                shutdown_rx,
            )
            .await
        });

        Ok(Box::new(HttpConnectSession {
            endpoint,
            ready: Some(ready_rx),
            shutdown: Some(shutdown_tx),
            task: Some(task),
            shutdown_timeout: self.shutdown_timeout,
        }))
    }
}

struct HttpConnectSession {
    endpoint: std::net::SocketAddr,
    ready: Option<oneshot::Receiver<()>>,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<Result<(), LocalProxyError>>>,
    shutdown_timeout: Duration,
}

#[async_trait]
impl LocalProxySession for HttpConnectSession {
    fn endpoint(&self) -> std::net::SocketAddr {
        self.endpoint
    }

    async fn wait_until_ready(
        &mut self,
        timeout_duration: Duration,
    ) -> Result<(), LocalProxyError> {
        let Some(ready) = self.ready.take() else {
            return Ok(());
        };
        match timeout(timeout_duration, ready).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(LocalProxyError::Readiness(
                "本地监听任务在就绪前退出".to_owned(),
            )),
            Err(_) => Err(LocalProxyError::Readiness("本地监听就绪超时".to_owned())),
        }
    }

    async fn shutdown(&mut self) -> Result<(), LocalProxyError> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let Some(mut task) = self.task.take() else {
            return Ok(());
        };
        match timeout(self.shutdown_timeout, &mut task).await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => Err(LocalProxyError::Shutdown(error.to_string())),
            Err(_) => {
                task.abort();
                Err(LocalProxyError::Shutdown("本地监听关闭超时".to_owned()))
            }
        }
    }

    fn abort(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl Drop for HttpConnectSession {
    fn drop(&mut self) {
        self.abort();
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_accept_loop<C>(
    listener: TcpListener,
    connector: Arc<C>,
    node: Arc<ProxyNode>,
    max_header_bytes: usize,
    handshake_timeout: Duration,
    connect_timeout: Duration,
    ready: oneshot::Sender<()>,
    mut shutdown: oneshot::Receiver<()>,
) -> Result<(), LocalProxyError>
where
    C: NodeConnector,
{
    let _ = ready.send(());
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
            accepted = listener.accept() => {
                let (stream, _) = accepted
                    .map_err(|error| LocalProxyError::Start(error.to_string()))?;
                let connector = connector.clone();
                let node = node.clone();
                connections.spawn(async move {
                    let _ = handle_client(
                        stream,
                        connector,
                        node,
                        max_header_bytes,
                        handshake_timeout,
                        connect_timeout,
                    )
                    .await;
                });
            }
        }
    }
    connections.shutdown().await;
    Ok(())
}

async fn handle_client<C>(
    mut inbound: TcpStream,
    connector: Arc<C>,
    node: Arc<ProxyNode>,
    max_header_bytes: usize,
    handshake_timeout: Duration,
    connect_timeout: Duration,
) -> Result<(), LocalProxyError>
where
    C: NodeConnector,
{
    let request = match timeout(
        handshake_timeout,
        read_connect_request(&mut inbound, max_header_bytes),
    )
    .await
    {
        Ok(Ok(request)) => request,
        Ok(Err(error)) => {
            send_request_error(&mut inbound, error).await;
            return Ok(());
        }
        Err(_) => {
            let _ = inbound
                .write_all(b"HTTP/1.1 408 Request Timeout\r\nConnection: close\r\n\r\n")
                .await;
            return Ok(());
        }
    };

    let mut outbound = match timeout(
        connect_timeout,
        connector.connect(node.as_ref(), &request.target),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        Ok(Err(_)) => {
            let _ = inbound
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\n\r\n")
                .await;
            return Ok(());
        }
        Err(_) => {
            tracing::debug!(
                category = "connector_timed_out",
                "embedded node connection failed"
            );
            let _ = inbound
                .write_all(b"HTTP/1.1 504 Gateway Timeout\r\nConnection: close\r\n\r\n")
                .await;
            return Ok(());
        }
    };

    inbound
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
        .map_err(|error| LocalProxyError::Start(error.to_string()))?;
    if !request.trailing_bytes.is_empty() {
        outbound
            .write_all(&request.trailing_bytes)
            .await
            .map_err(|error| LocalProxyError::Start(error.to_string()))?;
    }
    copy_bidirectional(&mut inbound, &mut outbound)
        .await
        .map_err(|error| LocalProxyError::Start(error.to_string()))?;
    let _ = outbound.shutdown().await;
    Ok(())
}

struct ConnectRequest {
    target: ConnectTarget,
    trailing_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestError {
    Invalid,
    UnsupportedMethod,
    HeaderTooLarge,
}

async fn read_connect_request(
    stream: &mut TcpStream,
    max_header_bytes: usize,
) -> Result<ConnectRequest, RequestError> {
    let mut buffer = Vec::with_capacity(1024);
    loop {
        if let Some(header_end) = find_header_end(&buffer) {
            return parse_connect_request(&buffer, header_end);
        }
        if buffer.len() >= max_header_bytes {
            return Err(RequestError::HeaderTooLarge);
        }
        let remaining = max_header_bytes.saturating_sub(buffer.len()).min(1024);
        let mut chunk = vec![0; remaining];
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|_| RequestError::Invalid)?;
        if read == 0 {
            return Err(RequestError::Invalid);
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

fn parse_connect_request(buffer: &[u8], header_end: usize) -> Result<ConnectRequest, RequestError> {
    let header = std::str::from_utf8(&buffer[..header_end]).map_err(|_| RequestError::Invalid)?;
    let request_line = header.lines().next().ok_or(RequestError::Invalid)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or(RequestError::Invalid)?;
    let authority = parts.next().ok_or(RequestError::Invalid)?;
    let version = parts.next().ok_or(RequestError::Invalid)?;
    if parts.next().is_some() || !version.starts_with("HTTP/1.") {
        return Err(RequestError::Invalid);
    }
    if method != "CONNECT" {
        return Err(RequestError::UnsupportedMethod);
    }
    let target = parse_authority(authority)?;
    Ok(ConnectRequest {
        target,
        trailing_bytes: buffer[header_end..].to_vec(),
    })
}

fn parse_authority(authority: &str) -> Result<ConnectTarget, RequestError> {
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let (host, port) = rest.split_once("]:").ok_or(RequestError::Invalid)?;
        (host, port)
    } else {
        authority.rsplit_once(':').ok_or(RequestError::Invalid)?
    };
    if host.is_empty()
        || host
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(RequestError::Invalid);
    }
    let port = port.parse::<u16>().map_err(|_| RequestError::Invalid)?;
    if port == 0 {
        return Err(RequestError::Invalid);
    }
    Ok(ConnectTarget {
        host: host.to_owned(),
        port,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

async fn send_request_error(stream: &mut TcpStream, error: RequestError) {
    let response: &[u8] = match error {
        RequestError::Invalid => b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n",
        RequestError::UnsupportedMethod => {
            b"HTTP/1.1 405 Method Not Allowed\r\nConnection: close\r\n\r\n"
        }
        RequestError::HeaderTooLarge => {
            b"HTTP/1.1 431 Request Header Fields Too Large\r\nConnection: close\r\n\r\n"
        }
    };
    let _ = stream.write_all(response).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LocalProxySupervisor, NodeRegion, ProxyProtocol, SecretNodeUri};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[derive(Debug, Clone, Copy)]
    struct TestDirectConnector;

    #[async_trait]
    impl NodeConnector for TestDirectConnector {
        async fn connect(
            &self,
            _node: &ProxyNode,
            target: &ConnectTarget,
        ) -> Result<BoxedNodeStream, LocalProxyError> {
            let stream = TcpStream::connect((target.host.as_str(), target.port))
                .await
                .map_err(|error| LocalProxyError::Start(error.to_string()))?;
            Ok(Box::new(stream))
        }
    }

    #[tokio::test]
    async fn forwards_connect_tunnel_end_to_end() {
        let echo_listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("echo listener");
        let echo_address = echo_listener.local_addr().expect("echo address");
        let echo_task = tokio::spawn(async move {
            let (mut stream, _) = echo_listener.accept().await.expect("echo connection");
            let (mut reader, mut writer) = stream.split();
            tokio::io::copy(&mut reader, &mut writer)
                .await
                .expect("echo traffic");
        });
        let supervisor = LocalProxySupervisor::new(
            HttpConnectProxyEngine::new(TestDirectConnector),
            Duration::from_secs(2),
        );
        let session = supervisor
            .start(&test_node())
            .await
            .expect("HTTP CONNECT proxy");
        let mut client = TcpStream::connect(session.endpoint())
            .await
            .expect("proxy connection");

        client
            .write_all(
                format!(
                    "CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n",
                    echo_address, echo_address
                )
                .as_bytes(),
            )
            .await
            .expect("CONNECT request");
        let response = read_response_header(&mut client).await;
        assert!(response.starts_with("HTTP/1.1 200"));

        client.write_all(b"wocao-hub").await.expect("tunnel write");
        let mut echoed = [0_u8; 9];
        client.read_exact(&mut echoed).await.expect("tunnel read");
        assert_eq!(&echoed, b"wocao-hub");

        drop(client);
        session.shutdown().await.expect("proxy shutdown");
        echo_task.await.expect("echo task");
    }

    #[tokio::test]
    async fn rejects_non_connect_http_requests() {
        let supervisor = LocalProxySupervisor::new(
            HttpConnectProxyEngine::new(TestDirectConnector),
            Duration::from_secs(2),
        );
        let session = supervisor
            .start(&test_node())
            .await
            .expect("HTTP CONNECT proxy");
        let mut client = TcpStream::connect(session.endpoint())
            .await
            .expect("proxy connection");

        client
            .write_all(b"GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\n\r\n")
            .await
            .expect("HTTP request");
        let response = read_response_header(&mut client).await;

        assert!(response.starts_with("HTTP/1.1 405"));
        session.shutdown().await.expect("proxy shutdown");
    }

    #[test]
    fn parses_domain_and_ipv6_connect_authorities() {
        assert_eq!(
            parse_authority("chatgpt.com:443").expect("domain authority"),
            ConnectTarget {
                host: "chatgpt.com".to_owned(),
                port: 443,
            }
        );
        assert_eq!(
            parse_authority("[::1]:8443").expect("IPv6 authority"),
            ConnectTarget {
                host: "::1".to_owned(),
                port: 8443,
            }
        );
    }

    async fn read_response_header(stream: &mut TcpStream) -> String {
        let mut buffer = Vec::new();
        let mut byte = [0_u8; 1];
        while !buffer.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).await.expect("response byte");
            buffer.push(byte[0]);
        }
        String::from_utf8(buffer).expect("UTF-8 response")
    }

    fn test_node() -> ProxyNode {
        ProxyNode {
            index: 1,
            name: "US test".to_owned(),
            protocol: ProxyProtocol::Vless,
            region: NodeRegion::UnitedStates,
            server: "example.com".to_owned(),
            port: 443,
            uri: SecretNodeUri(
                "vless://00000000-0000-0000-0000-000000000001@example.com:443".to_owned(),
            ),
        }
    }
}
