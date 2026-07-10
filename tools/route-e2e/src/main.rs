use activation_core::{
    default_activation_selection_options, ProxyPreparationService,
    StaticRouteProxyPreparationService,
};
use anyhow::{bail, Context};
use proxy_core::EmbeddedNodeConnector;
use route_bundle::RouteBundleClient;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use url::Url;
use zeroize::Zeroizing;

const DEFAULT_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/ray7086/wocao-hub-routes/main/public/manifest.json";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let quick = env::args().any(|argument| argument == "--quick");
    let single = env::args().any(|argument| argument == "--single");
    let config_directory = default_route_config_directory()?;
    let manifest_url = env::var("WOCAO_HUB_ROUTE_MANIFEST_URL")
        .unwrap_or_else(|_| DEFAULT_MANIFEST_URL.to_owned());
    let public_key_path = environment_path(
        "WOCAO_HUB_ROUTE_PUBLIC_KEY_FILE",
        config_directory.join("route-signing-public.pem"),
    );
    let encryption_key_path = environment_path(
        "WOCAO_HUB_ROUTE_KEY_FILE",
        config_directory.join("route-encryption-key.bin"),
    );
    let key_id = env::var("WOCAO_HUB_ROUTE_KEY_ID").unwrap_or_else(|_| "v1".to_owned());

    let public_key = fs::read_to_string(&public_key_path)
        .context("cannot read the local route verification public key")?;
    let raw_key = Zeroizing::new(
        fs::read(&encryption_key_path).context("cannot read the local route decryption key")?,
    );
    let encryption_key: [u8; 32] = raw_key
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("the local route decryption key must contain 32 bytes"))?;
    let manifest_url = Url::parse(&manifest_url).context("invalid route manifest URL")?;

    let cache_directory = tempfile::tempdir().context("cannot create temporary route cache")?;
    let route_client = RouteBundleClient::new(
        manifest_url,
        &public_key,
        Zeroizing::new(encryption_key),
        key_id,
        cache_directory.path().join("bundles"),
    )?;
    let mut options =
        default_activation_selection_options().context("invalid built-in probe target")?;
    if quick {
        options.minimum_target_coverage = 2;
        options.attempts = 2;
        options.timeout = Duration::from_secs(5);
        options.candidate_limit = 4;
    }
    if single {
        options.candidate_limit = 1;
    }
    let preparation = StaticRouteProxyPreparationService::new(
        route_client,
        options,
        cache_directory.path().join("proxy-cache.json"),
    );

    let payload = preparation.fetch_proxy_config().await?;
    let parsed = proxy_core::parse_subscription(payload.as_bytes())
        .context("decrypted route subscription cannot be parsed")?;
    let nodes = preparation.load_proxy_nodes(&payload).await?;
    if nodes.is_empty() {
        bail!("no overseas route candidates remain after filtering");
    }
    let connector = EmbeddedNodeConnector::default();
    let supported = nodes
        .iter()
        .filter(|node| connector.supports_node(node))
        .count();

    println!("GitHub 静态路由下载、验签、校验和解密已通过");
    println!("解析后的海外候选节点：{}", nodes.len());
    println!("已排除香港、国内或无效条目：{}", parsed.rejected.len());
    println!("当前内置协议可检测节点：{}", supported);
    println!("正在通过 ChatGPT/OpenAI 真实请求筛选节点...");

    let report = preparation.select_proxy_node(&nodes).await?;
    let selected = &report.selected.verification;
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
    Ok(())
}

fn environment_path(name: &str, default: PathBuf) -> PathBuf {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or(default)
}

fn default_route_config_directory() -> anyhow::Result<PathBuf> {
    let home_directory =
        dirs::home_dir().context("cannot locate the current user home directory")?;
    let dot_config = home_directory
        .join(".config")
        .join("wocao-hub")
        .join("routes");
    if dot_config.join("route-signing-public.pem").is_file()
        || dot_config.join("route-encryption-key.bin").is_file()
    {
        return Ok(dot_config);
    }
    Ok(dirs::config_dir()
        .context("cannot locate the current user configuration directory")?
        .join("wocao-hub")
        .join("routes"))
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
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
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
