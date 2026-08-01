use async_trait::async_trait;
#[cfg(any(target_os = "macos", target_os = "windows", test))]
use serde::{Deserialize, Serialize};
use shared_types::DesktopApp;
use std::fmt;
use std::net::SocketAddr;
#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use tokio::time::sleep;

#[derive(Clone, PartialEq, Eq)]
pub struct NetworkState {
    serialized: String,
}

impl NetworkState {
    #[must_use]
    pub fn from_serialized(serialized: String) -> Self {
        Self { serialized }
    }

    #[must_use]
    pub fn expose_serialized(&self) -> &str {
        &self.serialized
    }
}

impl fmt::Debug for NetworkState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NetworkState([REDACTED])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalProxySettings {
    pub address: SocketAddr,
    pub bypass_domains: Vec<String>,
}

impl LocalProxySettings {
    pub fn new(address: SocketAddr, bypass_domains: Vec<String>) -> Result<Self, PlatformError> {
        if !address.ip().is_loopback() || address.port() == 0 {
            return Err(PlatformError::InvalidProxySettings);
        }
        Ok(Self {
            address,
            bypass_domains,
        })
    }
}

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("系统操作失败：{0}")]
    Operation(String),
    #[error("当前阶段尚未实现该系统操作：{0}")]
    Unsupported(String),
    #[error("本地代理必须使用有效的回环地址和端口")]
    InvalidProxySettings,
    #[error("系统代理状态格式无效")]
    InvalidNetworkState,
    #[error("网络服务 {0} 使用了无法安全恢复的认证代理")]
    AuthenticatedProxyUnsupported(String),
    #[error("没有找到可配置的网络服务")]
    NoNetworkServices,
}

#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    async fn stop_desktop_app(&self, app: &DesktopApp) -> Result<(), PlatformError>;
    async fn launch_desktop_app(&self, app: &DesktopApp) -> Result<(), PlatformError>;
    async fn desktop_app_uses_locale(
        &self,
        app: &DesktopApp,
        locale: &str,
    ) -> Result<bool, PlatformError>;
    async fn save_network_state(&self) -> Result<NetworkState, PlatformError>;
    async fn apply_local_proxy(&self, settings: &LocalProxySettings) -> Result<(), PlatformError>;
    async fn restore_network_state(&self, state: &NetworkState) -> Result<(), PlatformError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NativePlatformAdapter;

#[async_trait]
impl PlatformAdapter for NativePlatformAdapter {
    async fn stop_desktop_app(&self, app: &DesktopApp) -> Result<(), PlatformError> {
        stop_desktop_app(app).await
    }

    async fn launch_desktop_app(&self, app: &DesktopApp) -> Result<(), PlatformError> {
        launch_desktop_app(app).await
    }

    async fn desktop_app_uses_locale(
        &self,
        app: &DesktopApp,
        locale: &str,
    ) -> Result<bool, PlatformError> {
        desktop_app_uses_locale(app, locale).await
    }

    async fn save_network_state(&self) -> Result<NetworkState, PlatformError> {
        save_network_state().await
    }

    async fn apply_local_proxy(&self, settings: &LocalProxySettings) -> Result<(), PlatformError> {
        apply_local_proxy(settings).await
    }

    async fn restore_network_state(&self, state: &NetworkState) -> Result<(), PlatformError> {
        restore_network_state(state).await
    }
}

pub async fn stop_desktop_app(app: &DesktopApp) -> Result<(), PlatformError> {
    #[cfg(target_os = "macos")]
    {
        return stop_macos_app(app).await;
    }

    #[cfg(target_os = "windows")]
    {
        return stop_windows_app(app).await;
    }

    #[allow(unreachable_code)]
    Err(PlatformError::Unsupported("停止桌面应用".to_owned()))
}

pub async fn launch_desktop_app(app: &DesktopApp) -> Result<(), PlatformError> {
    #[cfg(target_os = "macos")]
    {
        return launch_macos_app(app).await;
    }

    #[cfg(target_os = "windows")]
    {
        return launch_windows_app(app).await;
    }

    #[allow(unreachable_code)]
    Err(PlatformError::Unsupported("启动桌面应用".to_owned()))
}

pub async fn desktop_app_uses_locale(
    app: &DesktopApp,
    locale: &str,
) -> Result<bool, PlatformError> {
    if locale.trim().is_empty() {
        return Ok(false);
    }

    #[cfg(target_os = "macos")]
    {
        return macos_desktop_app_uses_locale(app, locale).await;
    }

    #[cfg(target_os = "windows")]
    {
        return windows_desktop_app_uses_locale(app, locale).await;
    }

    #[allow(unreachable_code)]
    Err(PlatformError::Unsupported(
        "验证桌面应用运行语言".to_owned(),
    ))
}

pub async fn save_network_state() -> Result<NetworkState, PlatformError> {
    #[cfg(target_os = "macos")]
    {
        return save_macos_network_state().await;
    }

    #[cfg(target_os = "windows")]
    {
        return save_windows_network_state().await;
    }

    #[allow(unreachable_code)]
    Err(PlatformError::Unsupported("保存系统代理状态".to_owned()))
}

pub async fn apply_local_proxy(settings: &LocalProxySettings) -> Result<(), PlatformError> {
    if !settings.address.ip().is_loopback() || settings.address.port() == 0 {
        return Err(PlatformError::InvalidProxySettings);
    }

    #[cfg(target_os = "macos")]
    {
        return apply_macos_local_proxy(settings).await;
    }

    #[cfg(target_os = "windows")]
    {
        return apply_windows_local_proxy(settings).await;
    }

    #[allow(unreachable_code)]
    Err(PlatformError::Unsupported("应用本地系统代理".to_owned()))
}

pub async fn restore_network_state(state: &NetworkState) -> Result<(), PlatformError> {
    #[cfg(target_os = "macos")]
    {
        return restore_macos_network_state(state).await;
    }

    #[cfg(target_os = "windows")]
    {
        return restore_windows_network_state(state).await;
    }

    #[allow(unreachable_code)]
    Err(PlatformError::Unsupported("恢复系统代理状态".to_owned()))
}

#[cfg(target_os = "macos")]
async fn stop_macos_app(app: &DesktopApp) -> Result<(), PlatformError> {
    let executable_name = Path::new(&app.executable_path)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| PlatformError::Operation("无法识别应用进程名".to_owned()))?;

    let status = Command::new("pkill")
        .args(["-TERM", "-x", executable_name])
        .status()
        .await
        .map_err(|error| PlatformError::Operation(error.to_string()))?;
    if !status.success() && status.code() != Some(1) {
        return Err(PlatformError::Operation(format!(
            "无法停止 {}",
            app.display_name
        )));
    }

    for _ in 0..10 {
        if !macos_process_running(executable_name).await? {
            return Ok(());
        }
        sleep(Duration::from_millis(250)).await;
    }

    let status = Command::new("pkill")
        .args(["-KILL", "-x", executable_name])
        .status()
        .await
        .map_err(|error| PlatformError::Operation(error.to_string()))?;
    if status.success() || status.code() == Some(1) {
        Ok(())
    } else {
        Err(PlatformError::Operation(format!(
            "{} 未能完全退出",
            app.display_name
        )))
    }
}

#[cfg(target_os = "macos")]
async fn macos_process_running(executable_name: &str) -> Result<bool, PlatformError> {
    let status = Command::new("pgrep")
        .args(["-x", executable_name])
        .status()
        .await
        .map_err(|error| PlatformError::Operation(error.to_string()))?;
    Ok(status.success())
}

#[cfg(target_os = "macos")]
async fn launch_macos_app(app: &DesktopApp) -> Result<(), PlatformError> {
    let status = Command::new("open")
        .args([
            "-na",
            app.install_path.as_str(),
            "--args",
            "--lang=zh-CN",
            "--locale=zh-CN",
        ])
        .status()
        .await
        .map_err(|error| PlatformError::Operation(error.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(PlatformError::Operation(format!(
            "无法重新打开 {}",
            app.display_name
        )))
    }
}

#[cfg(target_os = "macos")]
async fn macos_desktop_app_uses_locale(
    app: &DesktopApp,
    locale: &str,
) -> Result<bool, PlatformError> {
    let output = Command::new("ps")
        .args(["-axo", "command="])
        .output()
        .await
        .map_err(|error| PlatformError::Operation(error.to_string()))?;
    if !output.status.success() {
        return Err(PlatformError::Operation(
            "无法读取 ChatGPT/Codex 运行状态".to_owned(),
        ));
    }
    let commands = String::from_utf8(output.stdout)
        .map_err(|_| PlatformError::Operation("桌面应用运行状态格式无效".to_owned()))?;
    Ok(process_commands_use_locale(
        &commands,
        &app.install_path,
        locale,
    ))
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacNetworkState {
    services: Vec<MacServiceState>,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacServiceState {
    name: String,
    web_proxy: MacProxyState,
    secure_web_proxy: MacProxyState,
    socks_proxy: MacProxyState,
    auto_proxy_url: MacAutoProxyState,
    auto_discovery_enabled: bool,
    bypass_domains: Vec<String>,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacProxyState {
    enabled: bool,
    server: String,
    port: u16,
    authenticated: bool,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MacAutoProxyState {
    enabled: bool,
    url: String,
}

#[cfg(target_os = "macos")]
async fn save_macos_network_state() -> Result<NetworkState, PlatformError> {
    let services = macos_network_services().await?;
    let mut states = Vec::with_capacity(services.len());
    for service in services {
        let web_proxy = read_macos_proxy("-getwebproxy", &service).await?;
        let secure_web_proxy = read_macos_proxy("-getsecurewebproxy", &service).await?;
        let socks_proxy = read_macos_proxy("-getsocksfirewallproxy", &service).await?;
        if web_proxy.authenticated || secure_web_proxy.authenticated || socks_proxy.authenticated {
            return Err(PlatformError::AuthenticatedProxyUnsupported(service));
        }
        states.push(MacServiceState {
            auto_proxy_url: read_macos_auto_proxy(&service).await?,
            auto_discovery_enabled: read_macos_auto_discovery(&service).await?,
            bypass_domains: read_macos_bypass_domains(&service).await?,
            name: service,
            web_proxy,
            secure_web_proxy,
            socks_proxy,
        });
    }
    let serialized = serde_json::to_string(&MacNetworkState { services: states })
        .map_err(|_| PlatformError::InvalidNetworkState)?;
    Ok(NetworkState::from_serialized(serialized))
}

#[cfg(target_os = "macos")]
async fn apply_macos_local_proxy(settings: &LocalProxySettings) -> Result<(), PlatformError> {
    let host = settings.address.ip().to_string();
    let port = settings.address.port().to_string();
    let mut bypass_domains = settings.bypass_domains.clone();
    for domain in ["localhost", "127.0.0.1", "::1"] {
        if !bypass_domains.iter().any(|value| value == domain) {
            bypass_domains.push(domain.to_owned());
        }
    }

    for service in macos_network_services().await? {
        networksetup_status(&["-setwebproxy", &service, &host, &port]).await?;
        networksetup_status(&["-setwebproxystate", &service, "on"]).await?;
        networksetup_status(&["-setsecurewebproxy", &service, &host, &port]).await?;
        networksetup_status(&["-setsecurewebproxystate", &service, "on"]).await?;
        networksetup_status(&["-setsocksfirewallproxystate", &service, "off"]).await?;
        networksetup_status(&["-setautoproxystate", &service, "off"]).await?;
        networksetup_status(&["-setproxyautodiscovery", &service, "off"]).await?;
        let mut arguments = vec!["-setproxybypassdomains", service.as_str()];
        arguments.extend(bypass_domains.iter().map(String::as_str));
        networksetup_status(&arguments).await?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn restore_macos_network_state(state: &NetworkState) -> Result<(), PlatformError> {
    let state: MacNetworkState = serde_json::from_str(state.expose_serialized())
        .map_err(|_| PlatformError::InvalidNetworkState)?;
    if state.services.is_empty() {
        return Err(PlatformError::NoNetworkServices);
    }

    let mut first_error = None;
    for service in &state.services {
        if let Err(error) = restore_macos_service(service).await {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

#[cfg(target_os = "macos")]
async fn restore_macos_service(service: &MacServiceState) -> Result<(), PlatformError> {
    restore_macos_proxy(
        "-setwebproxy",
        "-setwebproxystate",
        &service.name,
        &service.web_proxy,
    )
    .await?;
    restore_macos_proxy(
        "-setsecurewebproxy",
        "-setsecurewebproxystate",
        &service.name,
        &service.secure_web_proxy,
    )
    .await?;
    restore_macos_proxy(
        "-setsocksfirewallproxy",
        "-setsocksfirewallproxystate",
        &service.name,
        &service.socks_proxy,
    )
    .await?;

    if !service.auto_proxy_url.url.is_empty() {
        networksetup_status(&[
            "-setautoproxyurl",
            &service.name,
            &service.auto_proxy_url.url,
        ])
        .await?;
    }
    networksetup_status(&[
        "-setautoproxystate",
        &service.name,
        on_off(service.auto_proxy_url.enabled),
    ])
    .await?;
    networksetup_status(&[
        "-setproxyautodiscovery",
        &service.name,
        on_off(service.auto_discovery_enabled),
    ])
    .await?;

    if service.bypass_domains.is_empty() {
        networksetup_status(&["-setproxybypassdomains", &service.name, "Empty"]).await?;
    } else {
        let mut arguments = vec!["-setproxybypassdomains", service.name.as_str()];
        arguments.extend(service.bypass_domains.iter().map(String::as_str));
        networksetup_status(&arguments).await?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn restore_macos_proxy(
    set_command: &str,
    state_command: &str,
    service: &str,
    proxy: &MacProxyState,
) -> Result<(), PlatformError> {
    if !proxy.server.is_empty() && proxy.port > 0 {
        let port = proxy.port.to_string();
        networksetup_status(&[set_command, service, &proxy.server, &port]).await?;
    }
    networksetup_status(&[state_command, service, on_off(proxy.enabled)]).await
}

#[cfg(target_os = "macos")]
async fn macos_network_services() -> Result<Vec<String>, PlatformError> {
    let output = networksetup_output(&["-listallnetworkservices"]).await?;
    let services = parse_macos_network_services(&output);
    if services.is_empty() {
        return Err(PlatformError::NoNetworkServices);
    }
    Ok(services)
}

#[cfg(target_os = "macos")]
async fn read_macos_proxy(command: &str, service: &str) -> Result<MacProxyState, PlatformError> {
    let output = networksetup_output(&[command, service]).await?;
    parse_macos_proxy_state(&output)
}

#[cfg(target_os = "macos")]
async fn read_macos_auto_proxy(service: &str) -> Result<MacAutoProxyState, PlatformError> {
    let output = networksetup_output(&["-getautoproxyurl", service]).await?;
    Ok(MacAutoProxyState {
        enabled: parse_networksetup_bool(field_value(&output, "Enabled")?)?,
        url: field_value(&output, "URL").unwrap_or_default().to_owned(),
    })
}

#[cfg(target_os = "macos")]
async fn read_macos_auto_discovery(service: &str) -> Result<bool, PlatformError> {
    let output = networksetup_output(&["-getproxyautodiscovery", service]).await?;
    parse_networksetup_bool(field_value(&output, "Auto Proxy Discovery")?)
}

#[cfg(target_os = "macos")]
async fn read_macos_bypass_domains(service: &str) -> Result<Vec<String>, PlatformError> {
    let output = networksetup_output(&["-getproxybypassdomains", service]).await?;
    Ok(parse_macos_bypass_domains(&output))
}

#[cfg(target_os = "macos")]
async fn networksetup_output(arguments: &[&str]) -> Result<String, PlatformError> {
    let output = Command::new("/usr/sbin/networksetup")
        .args(arguments)
        .output()
        .await
        .map_err(|error| PlatformError::Operation(error.to_string()))?;
    if !output.status.success() {
        return Err(PlatformError::Operation(format!(
            "networksetup {} 执行失败",
            arguments.first().copied().unwrap_or("命令")
        )));
    }
    String::from_utf8(output.stdout).map_err(|_| PlatformError::InvalidNetworkState)
}

#[cfg(target_os = "macos")]
async fn networksetup_status(arguments: &[&str]) -> Result<(), PlatformError> {
    let status = Command::new("/usr/sbin/networksetup")
        .args(arguments)
        .status()
        .await
        .map_err(|error| PlatformError::Operation(error.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(PlatformError::Operation(format!(
            "networksetup {} 执行失败",
            arguments.first().copied().unwrap_or("命令")
        )))
    }
}

#[cfg(target_os = "macos")]
fn parse_macos_network_services(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty() && !line.starts_with("An asterisk") && !line.starts_with('*')
        })
        .map(str::to_owned)
        .collect()
}

#[cfg(target_os = "macos")]
fn parse_macos_proxy_state(output: &str) -> Result<MacProxyState, PlatformError> {
    Ok(MacProxyState {
        enabled: parse_networksetup_bool(field_value(output, "Enabled")?)?,
        server: field_value(output, "Server").unwrap_or_default().to_owned(),
        port: field_value(output, "Port")?
            .parse()
            .map_err(|_| PlatformError::InvalidNetworkState)?,
        authenticated: parse_networksetup_bool(field_value(
            output,
            "Authenticated Proxy Enabled",
        )?)?,
    })
}

#[cfg(target_os = "macos")]
fn parse_macos_bypass_domains(output: &str) -> Vec<String> {
    if output.contains("aren't any bypass domains") {
        return Vec::new();
    }
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(target_os = "macos")]
fn field_value<'a>(output: &'a str, key: &str) -> Result<&'a str, PlatformError> {
    output
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find_map(|(candidate, value)| (candidate.trim() == key).then_some(value.trim()))
        .ok_or(PlatformError::InvalidNetworkState)
}

#[cfg(target_os = "macos")]
fn parse_networksetup_bool(value: &str) -> Result<bool, PlatformError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "yes" | "on" | "1" => Ok(true),
        "no" | "off" | "0" => Ok(false),
        _ => Err(PlatformError::InvalidNetworkState),
    }
}

#[cfg(target_os = "macos")]
fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

#[cfg(target_os = "windows")]
async fn stop_windows_app(_app: &DesktopApp) -> Result<(), PlatformError> {
    let mut command = hidden_windows_command("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        "Get-Process -Name Codex,ChatGPT -ErrorAction SilentlyContinue | Stop-Process -Force",
    ]);
    let status = command
        .status()
        .await
        .map_err(|error| PlatformError::Operation(error.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(PlatformError::Operation(
            "无法停止 ChatGPT/Codex".to_owned(),
        ))
    }
}

#[cfg(target_os = "windows")]
async fn launch_windows_app(app: &DesktopApp) -> Result<(), PlatformError> {
    let normalized = app.executable_path.replace('/', "\\").to_ascii_lowercase();
    if normalized.contains("\\windowsapps\\") {
        let app_id = app.bundle_identifier.as_deref().ok_or_else(|| {
            PlatformError::Operation("无法识别 Microsoft Store 应用 ID".to_owned())
        })?;
        hidden_windows_command("explorer.exe")
            .arg(format!(r"shell:AppsFolder\{app_id}"))
            .spawn()
            .map_err(|error| PlatformError::Operation(error.to_string()))?;
        return Ok(());
    }

    hidden_windows_command(&app.executable_path)
        .args(["--lang=zh-CN", "--locale=zh-CN"])
        .spawn()
        .map_err(|error| PlatformError::Operation(error.to_string()))?;
    Ok(())
}

#[cfg(target_os = "windows")]
async fn windows_desktop_app_uses_locale(
    app: &DesktopApp,
    locale: &str,
) -> Result<bool, PlatformError> {
    let output = windows_powershell_output(
        WINDOWS_VERIFY_APP_LOCALE_SCRIPT,
        &[
            ("DADA_ASSISTANT_APP_INSTALL_PATH", app.install_path.as_str()),
            ("DADA_ASSISTANT_APP_LOCALE", locale),
        ],
        "Windows 应用语言检测命令执行失败",
    )
    .await?;
    match output.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(PlatformError::Operation(
            "桌面应用运行语言检查返回无效结果".to_owned(),
        )),
    }
}

#[cfg(any(target_os = "macos", test))]
fn process_commands_use_locale(commands: &str, install_path: &str, locale: &str) -> bool {
    if install_path.trim().is_empty() || locale.trim().is_empty() {
        return false;
    }
    let locale_flag = format!("--lang={locale}");
    commands.lines().any(|command| {
        command.contains(install_path)
            && command.contains("--type=renderer")
            && command.split_whitespace().any(|part| part == locale_flag)
    })
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowsNetworkState {
    proxy_enable: RegistryDwordState,
    proxy_server: RegistryStringState,
    proxy_override: RegistryStringState,
    auto_config_url: RegistryStringState,
    auto_detect: RegistryDwordState,
    #[serde(default)]
    per_connection_flags: RegistryDwordState,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryDwordState {
    exists: bool,
    value: u32,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryStringState {
    exists: bool,
    value: String,
}

#[cfg(target_os = "windows")]
async fn save_windows_network_state() -> Result<NetworkState, PlatformError> {
    let script = windows_proxy_script(WINDOWS_SAVE_PROXY_SCRIPT);
    let output = windows_powershell_output(&script, &[], "Windows 系统代理状态读取失败").await?;
    let state: WindowsNetworkState =
        serde_json::from_str(output.trim()).map_err(|_| PlatformError::InvalidNetworkState)?;
    let serialized =
        serde_json::to_string(&state).map_err(|_| PlatformError::InvalidNetworkState)?;
    Ok(NetworkState::from_serialized(serialized))
}

#[cfg(target_os = "windows")]
async fn apply_windows_local_proxy(settings: &LocalProxySettings) -> Result<(), PlatformError> {
    let proxy_server = settings.address.to_string();
    let mut bypass_domains = settings.bypass_domains.clone();
    for domain in ["<local>", "localhost", "127.0.0.1", "::1"] {
        if !bypass_domains.iter().any(|value| value == domain) {
            bypass_domains.push(domain.to_owned());
        }
    }
    let proxy_override = bypass_domains.join(";");
    let environment = [
        ("DADA_ASSISTANT_PROXY_SERVER", proxy_server.as_str()),
        ("DADA_ASSISTANT_PROXY_OVERRIDE", proxy_override.as_str()),
    ];
    let script = windows_proxy_script(WINDOWS_APPLY_PROXY_SCRIPT);
    let mut attempt = 1_u8;
    loop {
        let result =
            windows_powershell_status(&script, &environment, "Windows 系统代理写入失败").await;
        match result {
            Ok(()) => return Ok(()),
            Err(error) if attempt < 3 => {
                tracing::warn!(attempt, error = %error, "retrying Windows proxy apply");
                sleep(Duration::from_millis(u64::from(attempt) * 350)).await;
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(target_os = "windows")]
async fn restore_windows_network_state(state: &NetworkState) -> Result<(), PlatformError> {
    let parsed: WindowsNetworkState = serde_json::from_str(state.expose_serialized())
        .map_err(|_| PlatformError::InvalidNetworkState)?;
    let serialized =
        serde_json::to_string(&parsed).map_err(|_| PlatformError::InvalidNetworkState)?;
    let script = windows_proxy_script(WINDOWS_RESTORE_PROXY_SCRIPT);
    windows_powershell_status(
        &script,
        &[("DADA_ASSISTANT_NETWORK_STATE", serialized.as_str())],
        "Windows 系统代理恢复失败",
    )
    .await
}

#[cfg(target_os = "windows")]
async fn windows_powershell_output(
    script: &str,
    environment: &[(&str, &str)],
    failure_message: &str,
) -> Result<String, PlatformError> {
    let mut command = hidden_windows_command("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        script,
    ]);
    command.envs(environment.iter().copied());
    let output = command
        .output()
        .await
        .map_err(|error| PlatformError::Operation(error.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(
            exit_code = output.status.code(),
            stderr = %stderr.trim(),
            "Windows PowerShell operation failed"
        );
        return Err(PlatformError::Operation(windows_failure_message(
            failure_message,
            &stderr,
        )));
    }
    String::from_utf8(output.stdout).map_err(|_| PlatformError::InvalidNetworkState)
}

#[cfg(target_os = "windows")]
fn hidden_windows_command(program: &str) -> Command {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(target_os = "windows")]
async fn windows_powershell_status(
    script: &str,
    environment: &[(&str, &str)],
    failure_message: &str,
) -> Result<(), PlatformError> {
    windows_powershell_output(script, environment, failure_message)
        .await
        .map(|_| ())
}

#[cfg(target_os = "windows")]
fn windows_failure_message(fallback: &str, stderr: &str) -> String {
    let value = stderr.to_ascii_lowercase();
    let detail = if value.contains("proxy_write_verification_failed") {
        "写入后校验失败，可能被其他代理软件或系统策略立即覆盖"
    } else if value.contains("proxy_restore_verification_failed") {
        "恢复后校验失败，可能被其他代理软件或系统策略立即覆盖"
    } else if value.contains("per_connection_flags_read_failed") {
        "Windows 有效代理状态读取失败"
    } else if value.contains("per_connection_flags_write_failed") {
        "Windows 有效代理状态更新失败"
    } else if value.contains("wininet_notify_failed") {
        "代理值已写入，但 Windows 网络设置刷新失败"
    } else if value.contains("proxy_settings_path_missing") {
        "Windows 系统代理注册表路径不可用"
    } else if value.contains("access is denied")
        || value.contains("requested registry access is not allowed")
        || value.contains("unauthorizedaccess")
        || stderr.contains("拒绝访问")
    {
        "注册表访问被系统策略或安全软件拒绝"
    } else if value.contains("marked for deletion") {
        "系统代理注册表暂时不可用"
    } else {
        return fallback.to_owned();
    };
    format!("{fallback}：{detail}")
}

#[cfg(target_os = "windows")]
fn windows_proxy_script(body: &str) -> String {
    format!("{WINDOWS_WININET_PROXY_HELPER}\n{body}")
}

#[cfg(target_os = "windows")]
const WINDOWS_WININET_PROXY_HELPER: &str = r#"
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;

public static class DadaAssistantWinInetProxy
{
  private const uint INTERNET_OPTION_REFRESH = 37;
  private const uint INTERNET_OPTION_SETTINGS_CHANGED = 39;
  private const uint INTERNET_OPTION_PER_CONNECTION_OPTION = 75;
  private const uint INTERNET_PER_CONN_FLAGS = 1;
  private const uint INTERNET_PER_CONN_FLAGS_UI = 10;

  [StructLayout(LayoutKind.Explicit)]
  private struct INTERNET_PER_CONN_OPTION_VALUE
  {
    [FieldOffset(0)] public uint dwValue;
    [FieldOffset(0)] public IntPtr pszValue;
    [FieldOffset(0)] public System.Runtime.InteropServices.ComTypes.FILETIME ftValue;
  }

  [StructLayout(LayoutKind.Sequential)]
  private struct INTERNET_PER_CONN_OPTION
  {
    public uint dwOption;
    public INTERNET_PER_CONN_OPTION_VALUE Value;
  }

  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
  private struct INTERNET_PER_CONN_OPTION_LIST
  {
    public uint dwSize;
    public IntPtr pszConnection;
    public uint dwOptionCount;
    public uint dwOptionError;
    public IntPtr pOptions;
  }

  [DllImport("wininet.dll", EntryPoint = "InternetQueryOptionW", SetLastError = true)]
  [return: MarshalAs(UnmanagedType.Bool)]
  private static extern bool InternetQueryOption(
    IntPtr hInternet,
    uint dwOption,
    IntPtr lpBuffer,
    ref uint lpdwBufferLength);

  [DllImport("wininet.dll", EntryPoint = "InternetSetOptionW", SetLastError = true)]
  [return: MarshalAs(UnmanagedType.Bool)]
  private static extern bool InternetSetOption(
    IntPtr hInternet,
    uint dwOption,
    IntPtr lpBuffer,
    uint dwBufferLength);

  private static readonly int OptionSize = Marshal.SizeOf(typeof(INTERNET_PER_CONN_OPTION));
  private static readonly int ListSize = Marshal.SizeOf(typeof(INTERNET_PER_CONN_OPTION_LIST));

  private static void CheckLayout()
  {
    if (IntPtr.Size == 8 &&
        (OptionSize != 16 ||
         Marshal.OffsetOf(typeof(INTERNET_PER_CONN_OPTION), "Value").ToInt64() != 8 ||
         ListSize != 32 ||
         Marshal.OffsetOf(typeof(INTERNET_PER_CONN_OPTION_LIST), "pOptions").ToInt64() != 24))
    {
      throw new InvalidOperationException("unexpected WinINet x64 layout");
    }
  }

  private static bool TryQueryConnectionFlags(uint option, out uint flags, out int error)
  {
    CheckLayout();
    IntPtr options = IntPtr.Zero;
    IntPtr listPointer = IntPtr.Zero;
    try
    {
      options = Marshal.AllocHGlobal(OptionSize);
      Marshal.StructureToPtr(new INTERNET_PER_CONN_OPTION { dwOption = option }, options, false);
      listPointer = Marshal.AllocHGlobal(ListSize);
      Marshal.StructureToPtr(new INTERNET_PER_CONN_OPTION_LIST {
        dwSize = (uint)ListSize,
        pszConnection = IntPtr.Zero,
        dwOptionCount = 1,
        pOptions = options
      }, listPointer, false);

      uint bufferLength = (uint)ListSize;
      bool succeeded = InternetQueryOption(
        IntPtr.Zero,
        INTERNET_OPTION_PER_CONNECTION_OPTION,
        listPointer,
        ref bufferLength);
      error = succeeded ? 0 : Marshal.GetLastWin32Error();
      flags = succeeded
        ? ((INTERNET_PER_CONN_OPTION)Marshal.PtrToStructure(
            options,
            typeof(INTERNET_PER_CONN_OPTION))).Value.dwValue
        : 0;
      return succeeded;
    }
    finally
    {
      if (listPointer != IntPtr.Zero) Marshal.FreeHGlobal(listPointer);
      if (options != IntPtr.Zero) Marshal.FreeHGlobal(options);
    }
  }

  public static uint QueryConnectionFlags()
  {
    int error;
    uint flags;
    if (TryQueryConnectionFlags(INTERNET_PER_CONN_FLAGS_UI, out flags, out error) ||
        TryQueryConnectionFlags(INTERNET_PER_CONN_FLAGS, out flags, out error))
    {
      return flags;
    }
    throw new Win32Exception(error, "InternetQueryOptionW flags failed");
  }

  private static bool TrySetConnectionFlags(uint option, uint flags, out int error)
  {
    CheckLayout();
    IntPtr options = IntPtr.Zero;
    IntPtr listPointer = IntPtr.Zero;
    try
    {
      options = Marshal.AllocHGlobal(OptionSize);
      Marshal.StructureToPtr(new INTERNET_PER_CONN_OPTION {
        dwOption = option,
        Value = new INTERNET_PER_CONN_OPTION_VALUE { dwValue = flags }
      }, options, false);
      listPointer = Marshal.AllocHGlobal(ListSize);
      Marshal.StructureToPtr(new INTERNET_PER_CONN_OPTION_LIST {
        dwSize = (uint)ListSize,
        pszConnection = IntPtr.Zero,
        dwOptionCount = 1,
        pOptions = options
      }, listPointer, false);

      bool succeeded = InternetSetOption(
        IntPtr.Zero,
        INTERNET_OPTION_PER_CONNECTION_OPTION,
        listPointer,
        (uint)ListSize);
      error = succeeded ? 0 : Marshal.GetLastWin32Error();
      return succeeded;
    }
    finally
    {
      if (listPointer != IntPtr.Zero) Marshal.FreeHGlobal(listPointer);
      if (options != IntPtr.Zero) Marshal.FreeHGlobal(options);
    }
  }

  private static void NotifySettingsChanged()
  {
    if (!InternetSetOption(IntPtr.Zero, INTERNET_OPTION_SETTINGS_CHANGED, IntPtr.Zero, 0))
    {
      throw new Win32Exception(Marshal.GetLastWin32Error());
    }
    if (!InternetSetOption(IntPtr.Zero, INTERNET_OPTION_REFRESH, IntPtr.Zero, 0))
    {
      throw new Win32Exception(Marshal.GetLastWin32Error());
    }
  }

  public static void SetConnectionFlags(uint flags)
  {
    int error;
    if (!TrySetConnectionFlags(INTERNET_PER_CONN_FLAGS, flags, out error))
    {
      throw new Win32Exception(error, "InternetSetOptionW flags failed");
    }
    NotifySettingsChanged();
  }

  public static void RestoreConnectionFlags(uint flags)
  {
    int error;
    if (!TrySetConnectionFlags(INTERNET_PER_CONN_FLAGS, flags, out error))
    {
      throw new Win32Exception(error, "InternetSetOptionW restore flags failed");
    }
    NotifySettingsChanged();
  }

  public const uint ProxyTypeDirect = 0x00000001;
  public const uint ProxyTypeProxy = 0x00000002;
  public const uint ProxyTypeAutoProxyUrl = 0x00000004;
  public const uint ProxyTypeAutoDetect = 0x00000008;
}
'@

function Get-WinInetProxyFlags {
  return [DadaAssistantWinInetProxy]::QueryConnectionFlags()
}

function Set-WinInetProxyFlags([uint32]$flags) {
  [DadaAssistantWinInetProxy]::SetConnectionFlags($flags)
}

function Restore-WinInetProxyFlags([uint32]$flags) {
  [DadaAssistantWinInetProxy]::RestoreConnectionFlags($flags)
}
"#;

#[cfg(target_os = "windows")]
const WINDOWS_SAVE_PROXY_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$path = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'
function Read-Dword([string]$name) {
  try {
    [pscustomobject]@{ exists = $true; value = [uint32](Get-ItemPropertyValue -Path $path -Name $name -ErrorAction Stop) }
  } catch {
    [pscustomobject]@{ exists = $false; value = [uint32]0 }
  }
}
function Read-String([string]$name) {
  try {
    [pscustomobject]@{ exists = $true; value = [string](Get-ItemPropertyValue -Path $path -Name $name -ErrorAction Stop) }
  } catch {
    [pscustomobject]@{ exists = $false; value = '' }
  }
}
function Read-PerConnectionFlags {
  try {
    return [pscustomobject]@{ exists = $true; value = [uint32](Get-WinInetProxyFlags) }
  } catch {
    throw 'per_connection_flags_read_failed'
  }
}
[pscustomobject]@{
  proxyEnable = Read-Dword 'ProxyEnable'
  proxyServer = Read-String 'ProxyServer'
  proxyOverride = Read-String 'ProxyOverride'
  autoConfigUrl = Read-String 'AutoConfigURL'
  autoDetect = Read-Dword 'AutoDetect'
  perConnectionFlags = Read-PerConnectionFlags
} | ConvertTo-Json -Compress -Depth 4
"#;

#[cfg(target_os = "windows")]
const WINDOWS_APPLY_PROXY_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$path = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'
if ([string]::IsNullOrWhiteSpace($env:DADA_ASSISTANT_PROXY_SERVER)) { throw 'missing proxy server' }
if (-not (Test-Path -LiteralPath $path)) { throw 'proxy_settings_path_missing' }
New-ItemProperty -Path $path -Name 'ProxyEnable' -PropertyType DWord -Value 1 -Force | Out-Null
New-ItemProperty -Path $path -Name 'ProxyServer' -PropertyType String -Value $env:DADA_ASSISTANT_PROXY_SERVER -Force | Out-Null
New-ItemProperty -Path $path -Name 'ProxyOverride' -PropertyType String -Value $env:DADA_ASSISTANT_PROXY_OVERRIDE -Force | Out-Null
Remove-ItemProperty -Path $path -Name 'AutoConfigURL' -ErrorAction SilentlyContinue
try {
  Set-WinInetProxyFlags ([DadaAssistantWinInetProxy]::ProxyTypeProxy)
} catch {
  throw 'per_connection_flags_write_failed'
}
$actualEnable = [uint32](Get-ItemPropertyValue -Path $path -Name 'ProxyEnable' -ErrorAction Stop)
$actualServer = [string](Get-ItemPropertyValue -Path $path -Name 'ProxyServer' -ErrorAction Stop)
$actualOverride = [string](Get-ItemPropertyValue -Path $path -Name 'ProxyOverride' -ErrorAction Stop)
$actualAutoConfig = Get-ItemProperty -Path $path -Name 'AutoConfigURL' -ErrorAction SilentlyContinue
try {
  $actualFlags = [uint32](Get-WinInetProxyFlags)
} catch {
  throw 'per_connection_flags_read_failed'
}
$autoProxyFlags = [DadaAssistantWinInetProxy]::ProxyTypeAutoProxyUrl -bor [DadaAssistantWinInetProxy]::ProxyTypeAutoDetect
if ($actualEnable -ne 1 -or
    $actualServer -ne $env:DADA_ASSISTANT_PROXY_SERVER -or
    $actualOverride -ne $env:DADA_ASSISTANT_PROXY_OVERRIDE -or
    $null -ne $actualAutoConfig -or
    ($actualFlags -band [DadaAssistantWinInetProxy]::ProxyTypeProxy) -eq 0 -or
    ($actualFlags -band $autoProxyFlags) -ne 0) {
  throw 'proxy_write_verification_failed'
}
"#;

#[cfg(target_os = "windows")]
const WINDOWS_RESTORE_PROXY_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$path = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'
$state = $env:DADA_ASSISTANT_NETWORK_STATE | ConvertFrom-Json
if (-not (Test-Path -LiteralPath $path)) { throw 'proxy_settings_path_missing' }
function Restore-Dword([string]$name, $entry) {
  if ($entry.exists) {
    New-ItemProperty -Path $path -Name $name -PropertyType DWord -Value ([uint32]$entry.value) -Force | Out-Null
  } else {
    Remove-ItemProperty -Path $path -Name $name -ErrorAction SilentlyContinue
  }
}
function Restore-String([string]$name, $entry) {
  if ($entry.exists) {
    New-ItemProperty -Path $path -Name $name -PropertyType String -Value ([string]$entry.value) -Force | Out-Null
  } else {
    Remove-ItemProperty -Path $path -Name $name -ErrorAction SilentlyContinue
  }
}
Restore-Dword 'ProxyEnable' $state.proxyEnable
Restore-String 'ProxyServer' $state.proxyServer
Restore-String 'ProxyOverride' $state.proxyOverride
Restore-String 'AutoConfigURL' $state.autoConfigUrl
Restore-Dword 'AutoDetect' $state.autoDetect
if ($null -ne $state.perConnectionFlags -and [bool]$state.perConnectionFlags.exists) {
  $expectedFlags = [uint32]$state.perConnectionFlags.value
  try {
    Restore-WinInetProxyFlags $expectedFlags
  } catch {
    throw 'per_connection_flags_write_failed'
  }
} else {
  $legacyFlags = [uint32]0
  if ([bool]$state.proxyEnable.exists -and [uint32]$state.proxyEnable.value -ne 0) {
    $legacyFlags = $legacyFlags -bor [DadaAssistantWinInetProxy]::ProxyTypeProxy
  }
  if ([bool]$state.autoConfigUrl.exists -and -not [string]::IsNullOrWhiteSpace([string]$state.autoConfigUrl.value)) {
    $legacyFlags = $legacyFlags -bor [DadaAssistantWinInetProxy]::ProxyTypeAutoProxyUrl
  }
  if ([bool]$state.autoDetect.exists -and [uint32]$state.autoDetect.value -ne 0) {
    $legacyFlags = $legacyFlags -bor [DadaAssistantWinInetProxy]::ProxyTypeAutoDetect
  }
  if ($legacyFlags -eq 0) {
    $legacyFlags = [DadaAssistantWinInetProxy]::ProxyTypeDirect
  }
  $expectedFlags = $legacyFlags
  try {
    Restore-WinInetProxyFlags $expectedFlags
  } catch {
    throw 'per_connection_flags_write_failed'
  }
}
function Assert-Dword([string]$name, $expected) {
  try {
    $value = [uint32](Get-ItemPropertyValue -Path $path -Name $name -ErrorAction Stop)
    if (-not $expected.exists -or $value -ne [uint32]$expected.value) {
      throw 'proxy_restore_verification_failed'
    }
  } catch {
    if ($expected.exists -or $_.Exception.Message -eq 'proxy_restore_verification_failed') {
      throw 'proxy_restore_verification_failed'
    }
  }
}
function Assert-String([string]$name, $expected) {
  try {
    $value = [string](Get-ItemPropertyValue -Path $path -Name $name -ErrorAction Stop)
    if (-not $expected.exists -or $value -ne [string]$expected.value) {
      throw 'proxy_restore_verification_failed'
    }
  } catch {
    if ($expected.exists -or $_.Exception.Message -eq 'proxy_restore_verification_failed') {
      throw 'proxy_restore_verification_failed'
    }
  }
}
Assert-Dword 'ProxyEnable' $state.proxyEnable
Assert-String 'ProxyServer' $state.proxyServer
Assert-String 'ProxyOverride' $state.proxyOverride
Assert-String 'AutoConfigURL' $state.autoConfigUrl
try {
  $actualFlags = [uint32](Get-WinInetProxyFlags)
} catch {
  throw 'per_connection_flags_read_failed'
}
if ($actualFlags -ne $expectedFlags) {
  throw 'proxy_restore_verification_failed'
}
"#;

#[cfg(target_os = "windows")]
const WINDOWS_VERIFY_APP_LOCALE_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$installPath = $env:DADA_ASSISTANT_APP_INSTALL_PATH
$localeFlag = '--lang=' + $env:DADA_ASSISTANT_APP_LOCALE
if ([string]::IsNullOrWhiteSpace($installPath) -or [string]::IsNullOrWhiteSpace($env:DADA_ASSISTANT_APP_LOCALE)) {
  Write-Output 'false'
  exit 0
}
$matched = Get-CimInstance Win32_Process | Where-Object {
  $_.CommandLine -and
  $_.CommandLine.Contains($installPath) -and
  $_.CommandLine.Contains('--type=renderer') -and
  $_.CommandLine.Contains($localeFlag)
} | Select-Object -First 1
if ($null -eq $matched) {
  Write-Output 'false'
} else {
  Write-Output 'true'
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn local_proxy_settings_only_accept_loopback_addresses() {
        let valid = LocalProxySettings::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 17_892),
            Vec::new(),
        );
        let remote = LocalProxySettings::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10)), 17_892),
            Vec::new(),
        );
        let zero_port = LocalProxySettings::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            Vec::new(),
        );

        assert!(valid.is_ok());
        assert!(matches!(remote, Err(PlatformError::InvalidProxySettings)));
        assert!(matches!(
            zero_port,
            Err(PlatformError::InvalidProxySettings)
        ));
    }

    #[test]
    fn network_state_debug_output_is_redacted() {
        let state =
            NetworkState::from_serialized(r#"{"proxyServer":"sensitive.example:443"}"#.to_owned());
        let output = format!("{state:?}");

        assert_eq!(output, "NetworkState([REDACTED])");
        assert!(!output.contains("sensitive.example"));
    }

    #[test]
    fn legacy_windows_network_state_deserializes_without_connection_flags() {
        let state: WindowsNetworkState = serde_json::from_str(
            r#"{
              "proxyEnable":{"exists":true,"value":1},
              "proxyServer":{"exists":true,"value":"127.0.0.1:17892"},
              "proxyOverride":{"exists":false,"value":""},
              "autoConfigUrl":{"exists":false,"value":""},
              "autoDetect":{"exists":false,"value":0}
            }"#,
        )
        .expect("legacy state remains readable");

        assert!(!state.per_connection_flags.exists);
        assert_eq!(state.per_connection_flags.value, 0);
    }

    #[test]
    fn detects_only_target_renderer_with_requested_locale() {
        let commands = r#"
/Applications/ChatGPT.app/Contents/MacOS/ChatGPT --lang=en-US
/Applications/Other.app/Contents/Frameworks/Other Renderer --type=renderer --lang=zh-CN
/Applications/ChatGPT.app/Contents/Frameworks/Codex Renderer --type=renderer --lang=zh-CN
"#;

        assert!(process_commands_use_locale(
            commands,
            "/Applications/ChatGPT.app",
            "zh-CN"
        ));
        assert!(!process_commands_use_locale(
            commands,
            "/Applications/ChatGPT.app",
            "ja-JP"
        ));
        assert!(!process_commands_use_locale(
            commands,
            "/Applications/Codex.app",
            "zh-CN"
        ));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parses_only_enabled_macos_network_services() {
        let output = "An asterisk (*) denotes that a network service is disabled.\nWi-Fi\n*Thunderbolt Bridge\nUSB 10/100/1000 LAN\n";

        assert_eq!(
            parse_macos_network_services(output),
            vec!["Wi-Fi", "USB 10/100/1000 LAN"]
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parses_macos_proxy_and_bypass_state() {
        let proxy = parse_macos_proxy_state(
            "Enabled: Yes\nServer: 127.0.0.1\nPort: 17892\nAuthenticated Proxy Enabled: 0\n",
        )
        .expect("valid proxy state");
        let bypass = parse_macos_bypass_domains("localhost\n127.0.0.1\n*.example.com\n");
        let empty = parse_macos_bypass_domains("There aren't any bypass domains set on Wi-Fi.\n");

        assert!(proxy.enabled);
        assert_eq!(proxy.server, "127.0.0.1");
        assert_eq!(proxy.port, 17_892);
        assert!(!proxy.authenticated);
        assert_eq!(bypass, vec!["localhost", "127.0.0.1", "*.example.com"]);
        assert!(empty.is_empty());
    }
}
