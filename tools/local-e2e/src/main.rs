use anyhow::{bail, Context};
use local_e2e::LocalE2eStack;
use route_catalog::SecretSubscriptionUrl;
use std::env;
use url::Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let select_node = env::args().any(|argument| argument == "--select-node");
    let quick = env::args().any(|argument| argument == "--quick");
    let candidate_limit = env::args()
        .any(|argument| argument == "--single")
        .then_some(1);
    let raw_url = env::var("SUBSCRIPTION_URL")
        .context("missing required environment variable SUBSCRIPTION_URL")?;
    if raw_url.trim().is_empty() {
        bail!("SUBSCRIPTION_URL must not be empty");
    }
    let upstream = Url::parse(&raw_url).context("invalid SUBSCRIPTION_URL")?;
    drop(raw_url);
    let stack = LocalE2eStack::start_from_upstream(SecretSubscriptionUrl::new(upstream)?).await?;
    let summary = stack.summary();
    println!("本地联调链路已通过自检");
    println!("解析节点：{}", summary.parsed_node_count);
    println!("排除条目：{}", summary.rejected_node_count);
    println!("当前协议实现可测试：{}", summary.supported_node_count);
    println!("暂不支持的节点：{}", summary.unsupported_node_count);
    println!("Hysteria2：{}", summary.hysteria2_node_count);
    println!("VLESS：{}", summary.vless_node_count);
    println!(
        "Hysteria2 混淆：{}，端口跳跃：{}，跳过证书校验：{}",
        summary.hysteria2_obfuscated_count,
        summary.hysteria2_port_hopping_count,
        summary.hysteria2_insecure_count
    );
    println!("WOCAO_HUB_CONFIG_URL={}", summary.config_url);
    println!(
        "WOCAO_HUB_CONFIG_PUBLIC_KEY_PEM={}",
        summary.public_key_pem.replace('\n', "\\n")
    );
    if select_node {
        println!("正在通过 ChatGPT/OpenAI 真实请求筛选节点...");
        let cache_directory = tempfile::tempdir()?;
        let selected = stack
            .select_best_node(cache_directory.path(), quick, candidate_limit)
            .await?;
        println!("最优节点：{}", terminal_safe_label(&selected.name));
        println!("协议：{}", selected.protocol);
        println!("出口：{}", selected.country_code);
        println!(
            "目标覆盖：{}/{}",
            selected.successful_targets, selected.target_count
        );
        println!("综合延迟：{} ms", selected.median_delay_ms);
        for target in &selected.targets {
            println!(
                "目标 {}：{}/{}",
                terminal_safe_label(&target.name),
                target.success_count,
                target.attempt_count
            );
        }
    }
    println!("按 Ctrl+C 停止本地联调服务");
    tokio::signal::ctrl_c().await?;
    Ok(())
}

fn terminal_safe_label(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .filter(|character| !character.is_control())
        .take(120)
        .collect();
    if sanitized.trim().is_empty() {
        "未命名节点".to_owned()
    } else {
        sanitized
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=warn".into()),
        )
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_terminal_control_characters_from_node_labels() {
        assert_eq!(terminal_safe_label("US\u{1b}[31m-fast\n"), "US[31m-fast");
        assert_eq!(terminal_safe_label("\n\r\t"), "未命名节点");
    }
}
