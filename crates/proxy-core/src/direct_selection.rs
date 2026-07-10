use crate::{
    parse_node_connection_config, verify_selected_node, ActivationProbeTarget,
    ActivationVerificationOptions, EmbeddedNodeConnector, HttpConnectProxyEngine,
    LocalProxySupervisor, NodeBenchmark, NodeConnectionConfig, NodeSelectionReport, ProxyNode,
    ProxyProtocol, VerifiedActivationNode,
};
use async_trait::async_trait;
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::time::Duration;
use thiserror::Error;
use tokio::time::{timeout_at, Instant};
use url::Url;

#[derive(Debug, Clone)]
pub struct DirectSelectionOptions {
    pub exit_target_url: Url,
    pub probe_targets: Vec<ActivationProbeTarget>,
    pub minimum_target_coverage: usize,
    pub attempts: usize,
    pub timeout: Duration,
    pub preflight_timeout: Duration,
    pub selection_timeout: Duration,
    pub candidate_limit: usize,
}

#[derive(Debug, Clone)]
pub struct DirectVerifiedNode {
    pub node: ProxyNode,
    pub verification: VerifiedActivationNode,
}

#[derive(Debug, Clone)]
pub struct DirectNodeSelectionReport {
    pub selected: DirectVerifiedNode,
    pub verified: Vec<DirectVerifiedNode>,
}

impl DirectNodeSelectionReport {
    #[must_use]
    pub fn metrics_report(&self) -> NodeSelectionReport {
        NodeSelectionReport {
            selected: self.selected.verification.clone(),
            verified: self
                .verified
                .iter()
                .map(|candidate| candidate.verification.clone())
                .collect(),
        }
    }
}

#[derive(Debug, Error)]
pub enum DirectSelectionError {
    #[error("至少需要一个 ChatGPT/OpenAI 检测目标")]
    EmptyProbeTargets,
    #[error("订阅中没有当前客户端支持的海外节点")]
    NoSupportedNodes,
    #[error(
        "没有在限定时间内通过 ChatGPT/OpenAI 稳定性验证的节点：候选={candidate_count}，代理启动失败={start_failures}，目标验证失败={verification_failures}，超时={timed_out}"
    )]
    NoUsableNodes {
        candidate_count: usize,
        start_failures: usize,
        verification_failures: usize,
        timed_out: usize,
    },
    #[error("节点测速结束后本地代理关闭失败")]
    ProxyShutdown,
    #[error("本地代理地址无效")]
    InvalidLocalProxy,
}

#[async_trait]
pub trait ActivationNodeProbe: Send + Sync {
    async fn probe(
        &self,
        node: &ProxyNode,
        local_proxy_url: &Url,
        options: &DirectSelectionOptions,
    ) -> Option<VerifiedActivationNode>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HttpActivationNodeProbe;

#[async_trait]
impl ActivationNodeProbe for HttpActivationNodeProbe {
    async fn probe(
        &self,
        node: &ProxyNode,
        local_proxy_url: &Url,
        options: &DirectSelectionOptions,
    ) -> Option<VerifiedActivationNode> {
        let benchmark = NodeBenchmark {
            name: node.name.clone(),
            protocol: protocol_name(node.protocol).to_owned(),
            region: node.region,
            success_count: 1,
            attempt_count: 1,
            median_delay_ms: None,
            jitter_ms: None,
            score: 0,
        };
        verify_selected_node(
            &benchmark,
            local_proxy_url,
            &ActivationVerificationOptions {
                exit_target_url: &options.exit_target_url,
                probe_targets: &options.probe_targets,
                minimum_target_coverage: options
                    .minimum_target_coverage
                    .clamp(1, options.probe_targets.len()),
                attempts: options.attempts.max(1),
                timeout: options.timeout,
                preflight_timeout: options.preflight_timeout,
            },
        )
        .await
    }
}

pub struct DirectNodeSelector<P = HttpActivationNodeProbe> {
    connector: EmbeddedNodeConnector,
    probe: P,
    readiness_timeout: Duration,
}

impl Default for DirectNodeSelector<HttpActivationNodeProbe> {
    fn default() -> Self {
        Self::new(
            EmbeddedNodeConnector::default(),
            HttpActivationNodeProbe,
            Duration::from_secs(3),
        )
    }
}

impl<P> DirectNodeSelector<P>
where
    P: ActivationNodeProbe,
{
    #[must_use]
    pub fn new(connector: EmbeddedNodeConnector, probe: P, readiness_timeout: Duration) -> Self {
        Self {
            connector,
            probe,
            readiness_timeout,
        }
    }

    pub async fn select(
        &self,
        nodes: &[ProxyNode],
        options: &DirectSelectionOptions,
    ) -> Result<DirectNodeSelectionReport, DirectSelectionError> {
        if options.probe_targets.is_empty() {
            return Err(DirectSelectionError::EmptyProbeTargets);
        }
        let mut candidates: Vec<_> = nodes
            .iter()
            .filter(|node| node.activation_candidate() && self.connector.supports_node(node))
            .cloned()
            .collect();
        candidates.sort_by_key(|node| (connection_complexity(node), node.index));
        candidates.truncate(options.candidate_limit.max(1));
        if candidates.is_empty() {
            return Err(DirectSelectionError::NoSupportedNodes);
        }

        let supervisor = LocalProxySupervisor::new(
            HttpConnectProxyEngine::new(self.connector.clone()).with_timeouts(
                probe_handshake_timeout(options.timeout),
                probe_connect_timeout(options.timeout),
                Duration::from_secs(3),
            ),
            self.readiness_timeout,
        );
        let candidate_count = candidates.len();
        let mut start_failures = 0;
        let mut verification_failures = 0;
        let mut verified = Vec::new();
        let mut tasks = FuturesUnordered::new();
        for (candidate_index, node) in candidates.into_iter().enumerate() {
            let supervisor = &supervisor;
            let probe = &self.probe;
            tasks.push(async move {
                tracing::info!(
                    candidate = candidate_index + 1,
                    total = candidate_count,
                    "testing activation node in parallel"
                );
                let session = match supervisor.start(&node).await {
                    Ok(session) => session,
                    Err(_) => return CandidateOutcome::StartFailed,
                };
                let local_proxy_url = match Url::parse(&format!("http://{}", session.endpoint())) {
                    Ok(url) => url,
                    Err(_) => return CandidateOutcome::InvalidLocalProxy,
                };
                let verification = probe.probe(&node, &local_proxy_url, options).await;
                if session.shutdown().await.is_err() {
                    return CandidateOutcome::ShutdownFailed;
                }
                match verification {
                    Some(verification) => {
                        CandidateOutcome::Verified(Box::new(DirectVerifiedNode {
                            node,
                            verification,
                        }))
                    }
                    None => CandidateOutcome::VerificationFailed,
                }
            });
        }

        let deadline = Instant::now() + options.selection_timeout.max(Duration::from_millis(1));
        while !tasks.is_empty() {
            let outcome = match timeout_at(deadline, tasks.next()).await {
                Ok(Some(outcome)) => outcome,
                Ok(None) => break,
                Err(_) => break,
            };
            match outcome {
                CandidateOutcome::Verified(candidate) => verified.push(*candidate),
                CandidateOutcome::StartFailed => start_failures += 1,
                CandidateOutcome::VerificationFailed => verification_failures += 1,
                CandidateOutcome::ShutdownFailed => {
                    return Err(DirectSelectionError::ProxyShutdown)
                }
                CandidateOutcome::InvalidLocalProxy => {
                    return Err(DirectSelectionError::InvalidLocalProxy)
                }
            }
        }
        let timed_out = tasks.len();
        drop(tasks);
        if timed_out > 0 {
            tracing::info!(
                timed_out,
                candidate_count,
                "activation node selection deadline reached"
            );
        }
        verified.sort_by(|left, right| {
            left.verification
                .score
                .cmp(&right.verification.score)
                .then(left.node.index.cmp(&right.node.index))
        });
        let selected = verified
            .first()
            .cloned()
            .ok_or(DirectSelectionError::NoUsableNodes {
                candidate_count,
                start_failures,
                verification_failures,
                timed_out,
            })?;
        Ok(DirectNodeSelectionReport { selected, verified })
    }
}

enum CandidateOutcome {
    Verified(Box<DirectVerifiedNode>),
    StartFailed,
    VerificationFailed,
    ShutdownFailed,
    InvalidLocalProxy,
}

fn probe_handshake_timeout(probe_timeout: Duration) -> Duration {
    probe_timeout.min(Duration::from_secs(2))
}

fn probe_connect_timeout(probe_timeout: Duration) -> Duration {
    let margin = Duration::from_millis(500).min(probe_timeout / 4);
    probe_timeout.saturating_sub(margin)
}

fn connection_complexity(node: &ProxyNode) -> u8 {
    match parse_node_connection_config(node) {
        Ok(NodeConnectionConfig::Hysteria2(config)) => {
            u8::from(config.port_hopping.is_some()) + u8::from(config.obfuscation.is_some())
        }
        Ok(NodeConnectionConfig::Vless(_)) => 0,
        Err(_) => u8::MAX,
    }
}

fn protocol_name(protocol: ProxyProtocol) -> &'static str {
    match protocol {
        ProxyProtocol::Vless => "vless",
        ProxyProtocol::Hysteria2 => "hysteria2",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeRegion, SecretNodeUri, TargetBenchmark};

    #[test]
    fn proxy_connect_timeout_finishes_before_the_outer_probe_timeout() {
        let probe_timeout = Duration::from_secs(5);

        assert_eq!(
            probe_handshake_timeout(probe_timeout),
            Duration::from_secs(2)
        );
        assert_eq!(
            probe_connect_timeout(probe_timeout),
            Duration::from_millis(4500)
        );
        assert!(probe_connect_timeout(probe_timeout) < probe_timeout);
    }

    #[test]
    fn fixed_port_nodes_are_considered_before_port_hopping_nodes() {
        let fixed = node(
            2,
            "fixed",
            ProxyProtocol::Hysteria2,
            "hysteria2://secret@example.net:443?sni=example.net",
        );
        let hopping = node(
            1,
            "hopping",
            ProxyProtocol::Hysteria2,
            "hysteria2://secret@example.net:443?sni=example.net&mport=20000-30000",
        );

        assert!(connection_complexity(&fixed) < connection_complexity(&hopping));
    }

    #[derive(Debug, Clone, Copy)]
    struct FakeProbe;

    #[async_trait]
    impl ActivationNodeProbe for FakeProbe {
        async fn probe(
            &self,
            node: &ProxyNode,
            local_proxy_url: &Url,
            _options: &DirectSelectionOptions,
        ) -> Option<VerifiedActivationNode> {
            assert_eq!(local_proxy_url.host_str(), Some("127.0.0.1"));
            let score = if node.name.contains("fast") { 10 } else { 20 };
            Some(VerifiedActivationNode {
                name: node.name.clone(),
                protocol: "vless".to_owned(),
                region: node.region,
                country_code: "US".to_owned(),
                exit_success_count: 2,
                exit_attempt_count: 2,
                successful_targets: 1,
                target_count: 1,
                success_count: 2,
                attempt_count: 2,
                median_delay_ms: score,
                jitter_ms: 0,
                score,
                targets: vec![TargetBenchmark {
                    name: "chatgpt".to_owned(),
                    success_count: 2,
                    attempt_count: 2,
                    median_delay_ms: Some(score),
                    jitter_ms: Some(0),
                }],
            })
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct DelayedProbe {
        delay: Duration,
        succeeds: bool,
    }

    #[async_trait]
    impl ActivationNodeProbe for DelayedProbe {
        async fn probe(
            &self,
            node: &ProxyNode,
            local_proxy_url: &Url,
            options: &DirectSelectionOptions,
        ) -> Option<VerifiedActivationNode> {
            tokio::time::sleep(self.delay).await;
            if !self.succeeds {
                return None;
            }
            FakeProbe.probe(node, local_proxy_url, options).await
        }
    }

    #[tokio::test]
    async fn selects_best_supported_overseas_node_with_local_proxy_lifecycle() {
        let selector = DirectNodeSelector::new(
            EmbeddedNodeConnector::default(),
            FakeProbe,
            Duration::from_secs(1),
        );
        let options = options();
        let report = selector
            .select(
                &[
                    basic_vless_node(1, "US slow", NodeRegion::UnitedStates),
                    basic_vless_node(2, "US fast", NodeRegion::UnitedStates),
                    basic_vless_node(3, "HK fast", NodeRegion::HongKong),
                ],
                &options,
            )
            .await
            .expect("direct selection");

        assert_eq!(report.selected.node.name, "US fast");
        assert_eq!(report.verified.len(), 2);
        assert_eq!(report.metrics_report().selected.score, 10);
    }

    #[tokio::test]
    async fn rejects_subscription_with_only_reality_nodes() {
        let selector = DirectNodeSelector::new(
            EmbeddedNodeConnector::default(),
            FakeProbe,
            Duration::from_secs(1),
        );
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

        assert!(matches!(
            selector.select(&[node], &options()).await,
            Err(DirectSelectionError::NoSupportedNodes)
        ));
    }

    #[tokio::test]
    async fn probes_all_candidates_concurrently() {
        let selector = DirectNodeSelector::new(
            EmbeddedNodeConnector::default(),
            DelayedProbe {
                delay: Duration::from_millis(250),
                succeeds: true,
            },
            Duration::from_secs(1),
        );
        let mut options = options();
        options.selection_timeout = Duration::from_secs(2);
        let nodes: Vec<_> = (1..=4)
            .map(|index| {
                basic_vless_node(index, &format!("US node {index}"), NodeRegion::UnitedStates)
            })
            .collect();
        let started = Instant::now();

        let report = selector
            .select(&nodes, &options)
            .await
            .expect("parallel selection");

        assert_eq!(report.verified.len(), 4);
        assert!(started.elapsed() < Duration::from_millis(700));
    }

    #[tokio::test]
    async fn enforces_the_overall_selection_deadline() {
        let selector = DirectNodeSelector::new(
            EmbeddedNodeConnector::default(),
            DelayedProbe {
                delay: Duration::from_secs(5),
                succeeds: true,
            },
            Duration::from_secs(1),
        );
        let mut options = options();
        options.selection_timeout = Duration::from_millis(100);
        let nodes = [
            basic_vless_node(1, "US node 1", NodeRegion::UnitedStates),
            basic_vless_node(2, "US node 2", NodeRegion::UnitedStates),
        ];
        let started = Instant::now();

        let error = selector
            .select(&nodes, &options)
            .await
            .expect_err("selection deadline");

        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(matches!(
            error,
            DirectSelectionError::NoUsableNodes { timed_out: 2, .. }
        ));
    }

    fn basic_vless_node(index: usize, name: &str, region: NodeRegion) -> ProxyNode {
        ProxyNode {
            index,
            name: name.to_owned(),
            protocol: ProxyProtocol::Vless,
            region,
            server: "127.0.0.1".to_owned(),
            port: 443,
            uri: SecretNodeUri(format!(
                "vless://00000000-0000-0000-0000-000000000001@127.0.0.1:443#{}",
                name.replace(' ', "-")
            )),
        }
    }

    fn node(index: usize, name: &str, protocol: ProxyProtocol, uri: &str) -> ProxyNode {
        let parsed = Url::parse(uri).expect("node URL");
        ProxyNode {
            index,
            name: name.to_owned(),
            protocol,
            region: NodeRegion::UnitedStates,
            server: parsed.host_str().expect("server host").to_owned(),
            port: parsed.port().expect("server port"),
            uri: SecretNodeUri(uri.to_owned()),
        }
    }

    fn options() -> DirectSelectionOptions {
        DirectSelectionOptions {
            exit_target_url: Url::parse("https://chatgpt.com/cdn-cgi/trace").expect("exit URL"),
            probe_targets: vec![ActivationProbeTarget::new(
                "chatgpt",
                Url::parse("https://chatgpt.com/").expect("probe URL"),
            )],
            minimum_target_coverage: 1,
            attempts: 2,
            timeout: Duration::from_secs(1),
            preflight_timeout: Duration::from_millis(500),
            selection_timeout: Duration::from_secs(2),
            candidate_limit: 8,
        }
    }
}
