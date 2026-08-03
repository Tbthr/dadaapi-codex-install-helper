use std::{sync::Arc, time::Duration};

use reqwest::{redirect::Policy, Client};
use serde::Deserialize;
use shared_types::{
    CpuArchitecture, DownloadCatalog, DownloadCompatibility, DownloadPackageKind, OperatingSystem,
    SoftwareArtifactSummary, SoftwareProductId, SoftwareProductSummary,
};
use thiserror::Error;
use url::Url;

const CHATGPT_OFFICIAL_PAGE: &str = "https://chatgpt.com/download/";
const CHATGPT_MACOS_ARM64_URL: &str = "https://persistent.oaistatic.com/codex-app-prod/Codex.dmg";
const CHATGPT_MACOS_X64_URL: &str =
    "https://persistent.oaistatic.com/codex-app-prod/Codex-latest-x64.dmg";
const CLAUDE_OFFICIAL_PAGE: &str = "https://claude.ai/download";
const CLAUDE_MAC_RELEASES_URL: &str =
    "https://downloads.claude.ai/releases/darwin/universal/RELEASES.json";
const CLAUDE_WINDOWS_ARM64_URL: &str =
    "https://claude.ai/api/desktop/win32/arm64/msix/latest/redirect";
const CLAUDE_WINDOWS_X64_URL: &str = "https://claude.ai/api/desktop/win32/x64/msix/latest/redirect";

const CC_SWITCH_OFFICIAL_PAGE: &str = "https://github.com/farion1231/cc-switch";
const CC_SWITCH_RELEASES_URL: &str =
    "https://api.github.com/repos/farion1231/cc-switch/releases/latest";

const NODE_OFFICIAL_PAGE: &str = "https://nodejs.org/en/download";
const NODE_RELEASES_URL: &str = "https://nodejs.org/dist/index.json";

const VSCODE_OFFICIAL_PAGE: &str = "https://code.visualstudio.com/download";
const VSCODE_MACOS_URL: &str =
    "https://update.code.visualstudio.com/latest/darwin-universal/stable";
const VSCODE_WINDOWS_ARM64_URL: &str =
    "https://update.code.visualstudio.com/latest/win32-arm64-user/stable";
const VSCODE_WINDOWS_X64_URL: &str =
    "https://update.code.visualstudio.com/latest/win32-x64-user/stable";

const MAX_REDIRECTS: usize = 5;
const MAX_INSTALLER_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("内置官方下载地址无效")]
    InvalidUrl,
    #[error("没有适用于当前系统与架构的官方安装包")]
    UnsupportedHost,
    #[error("未知的官方软件产品")]
    UnknownProduct,
    #[error("未知或不适用于当前设备的官方安装包")]
    UnknownArtifact,
    #[error("无法创建官方下载客户端")]
    Client(#[source] reqwest::Error),
    #[error("官方版本信息请求失败")]
    Request,
    #[error("官方版本信息格式无效")]
    InvalidMetadata,
    #[error("官方版本中没有找到当前设备的安装包")]
    AssetNotFound,
}

#[derive(Debug, Clone)]
enum ArtifactSource {
    Fixed(Url),
    MicrosoftStore(CpuArchitecture),
    ClaudeMac,
    CcSwitch,
    NodeLts,
}

#[derive(Debug, Clone)]
pub struct TrustedDownloadArtifact {
    pub product_id: SoftwareProductId,
    pub summary: SoftwareArtifactSummary,
    source: ArtifactSource,
    pub official_page_url: Url,
    pub allowed_redirect_hosts: Arc<[String]>,
    pub maximum_size_bytes: u64,
}

impl TrustedDownloadArtifact {
    pub fn download_client(&self) -> Result<Client, CatalogError> {
        let allowed_hosts = self.allowed_redirect_hosts.clone();
        Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30 * 60))
            .redirect(Policy::custom(move |attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    return attempt.error("too many download redirects");
                }
                if trusted_download_url(attempt.url(), &allowed_hosts) {
                    attempt.follow()
                } else {
                    attempt.error("download redirect is not trusted")
                }
            }))
            .build()
            .map_err(CatalogError::Client)
    }

    pub async fn resolve_source_url(&self) -> Result<Url, CatalogError> {
        let url = match &self.source {
            ArtifactSource::Fixed(url) => Ok(url.clone()),
            ArtifactSource::MicrosoftStore(architecture) => {
                super::ms_store::resolve_chatgpt_msix_url(*architecture)
                    .await
                    .map_err(|error| {
                        tracing::warn!(error = %error, "could not resolve ChatGPT MSIX");
                        CatalogError::Request
                    })
            }
            ArtifactSource::ClaudeMac => resolve_claude_mac_url().await,
            ArtifactSource::CcSwitch => {
                resolve_cc_switch_url(
                    self.summary.operating_system,
                    self.summary.native_architecture,
                )
                .await
            }
            ArtifactSource::NodeLts => {
                resolve_node_lts_url(
                    self.summary.operating_system,
                    self.summary.native_architecture,
                )
                .await
            }
        }?;
        if trusted_download_url(&url, &self.allowed_redirect_hosts) {
            Ok(url)
        } else {
            Err(CatalogError::InvalidUrl)
        }
    }
}

pub fn official_download_catalog(
    operating_system: OperatingSystem,
    cpu_architecture: CpuArchitecture,
) -> Result<DownloadCatalog, CatalogError> {
    Ok(DownloadCatalog {
        operating_system,
        cpu_architecture,
        products: vec![
            product_summary(
                SoftwareProductId::ChatGptDesktop,
                "ChatGPT",
                "OpenAI",
                CHATGPT_OFFICIAL_PAGE,
                operating_system,
                cpu_architecture,
            )?,
            product_summary(
                SoftwareProductId::ClaudeDesktop,
                "Claude Desktop",
                "Anthropic",
                CLAUDE_OFFICIAL_PAGE,
                operating_system,
                cpu_architecture,
            )?,
            product_summary(
                SoftwareProductId::CcSwitch,
                "CC Switch",
                "CC Switch",
                CC_SWITCH_OFFICIAL_PAGE,
                operating_system,
                cpu_architecture,
            )?,
            product_summary(
                SoftwareProductId::NodeJsLts,
                "Node.js LTS",
                "OpenJS Foundation",
                NODE_OFFICIAL_PAGE,
                operating_system,
                cpu_architecture,
            )?,
            product_summary(
                SoftwareProductId::VisualStudioCode,
                "Visual Studio Code",
                "Microsoft",
                VSCODE_OFFICIAL_PAGE,
                operating_system,
                cpu_architecture,
            )?,
        ],
    })
}

pub fn resolve_official_artifact(
    product_id: SoftwareProductId,
    artifact_id: Option<&str>,
    operating_system: OperatingSystem,
    cpu_architecture: CpuArchitecture,
) -> Result<TrustedDownloadArtifact, CatalogError> {
    let artifact = match product_id {
        SoftwareProductId::ChatGptDesktop => chatgpt_artifact(operating_system, cpu_architecture)?,
        SoftwareProductId::ClaudeDesktop => claude_artifact(operating_system, cpu_architecture)?,
        SoftwareProductId::CcSwitch => cc_switch_artifact(operating_system, cpu_architecture)?,
        SoftwareProductId::NodeJsLts => node_artifact(operating_system, cpu_architecture)?,
        SoftwareProductId::VisualStudioCode => vscode_artifact(operating_system, cpu_architecture)?,
    };
    if artifact_id.is_some_and(|requested| requested != artifact.summary.id) {
        return Err(CatalogError::UnknownArtifact);
    }
    Ok(artifact)
}

fn product_summary(
    product_id: SoftwareProductId,
    display_name: &str,
    publisher: &str,
    official_page: &str,
    operating_system: OperatingSystem,
    cpu_architecture: CpuArchitecture,
) -> Result<SoftwareProductSummary, CatalogError> {
    let artifact = resolve_official_artifact(product_id, None, operating_system, cpu_architecture)?;
    let official_page_url = parse_official_page(official_page)?;
    Ok(SoftwareProductSummary {
        id: product_id,
        display_name: display_name.to_owned(),
        publisher: publisher.to_owned(),
        official_page_url,
        artifacts: vec![artifact.summary],
    })
}

fn chatgpt_artifact(
    operating_system: OperatingSystem,
    cpu_architecture: CpuArchitecture,
) -> Result<TrustedDownloadArtifact, CatalogError> {
    match (operating_system, cpu_architecture) {
        (OperatingSystem::MacOs, CpuArchitecture::Arm64) => fixed_artifact(
            SoftwareProductId::ChatGptDesktop,
            "chatgpt-macos-arm64",
            OperatingSystem::MacOs,
            Some(CpuArchitecture::Arm64),
            DownloadCompatibility::Native,
            DownloadPackageKind::Dmg,
            "ChatGPT-Apple-Silicon.dmg",
            Some("macOS 12.0"),
            CHATGPT_MACOS_ARM64_URL,
            CHATGPT_OFFICIAL_PAGE,
            &["persistent.oaistatic.com"],
        ),
        (OperatingSystem::MacOs, CpuArchitecture::X64) => fixed_artifact(
            SoftwareProductId::ChatGptDesktop,
            "chatgpt-macos-x64",
            OperatingSystem::MacOs,
            Some(CpuArchitecture::X64),
            DownloadCompatibility::Native,
            DownloadPackageKind::Dmg,
            "ChatGPT-Intel.dmg",
            Some("macOS 12.0"),
            CHATGPT_MACOS_X64_URL,
            CHATGPT_OFFICIAL_PAGE,
            &["persistent.oaistatic.com"],
        ),
        (OperatingSystem::Windows, architecture) => dynamic_artifact(
            SoftwareProductId::ChatGptDesktop,
            match architecture {
                CpuArchitecture::Arm64 => "chatgpt-windows-arm64-msix",
                CpuArchitecture::X64 => "chatgpt-windows-x64-msix",
            },
            OperatingSystem::Windows,
            Some(architecture),
            DownloadCompatibility::Native,
            DownloadPackageKind::Msix,
            "ChatGPT.msix",
            Some("Windows 10 19041"),
            ArtifactSource::MicrosoftStore(architecture),
            CHATGPT_OFFICIAL_PAGE,
            &["dl.delivery.mp.microsoft.com"],
        ),
    }
}

fn claude_artifact(
    operating_system: OperatingSystem,
    cpu_architecture: CpuArchitecture,
) -> Result<TrustedDownloadArtifact, CatalogError> {
    match operating_system {
        OperatingSystem::MacOs => dynamic_artifact(
            SoftwareProductId::ClaudeDesktop,
            "claude-macos-universal",
            OperatingSystem::MacOs,
            None,
            DownloadCompatibility::Native,
            DownloadPackageKind::Dmg,
            "Claude.dmg",
            Some("macOS 11"),
            ArtifactSource::ClaudeMac,
            CLAUDE_OFFICIAL_PAGE,
            &["downloads.claude.ai"],
        ),
        OperatingSystem::Windows => fixed_artifact(
            SoftwareProductId::ClaudeDesktop,
            match cpu_architecture {
                CpuArchitecture::Arm64 => "claude-windows-arm64",
                CpuArchitecture::X64 => "claude-windows-x64",
            },
            OperatingSystem::Windows,
            Some(cpu_architecture),
            DownloadCompatibility::Native,
            DownloadPackageKind::Msix,
            "Claude.msix",
            Some("Windows 10"),
            match cpu_architecture {
                CpuArchitecture::Arm64 => CLAUDE_WINDOWS_ARM64_URL,
                CpuArchitecture::X64 => CLAUDE_WINDOWS_X64_URL,
            },
            CLAUDE_OFFICIAL_PAGE,
            &["claude.ai", "downloads.claude.ai"],
        ),
    }
}

fn cc_switch_artifact(
    operating_system: OperatingSystem,
    cpu_architecture: CpuArchitecture,
) -> Result<TrustedDownloadArtifact, CatalogError> {
    let (id, architecture, kind, file_name) = match (operating_system, cpu_architecture) {
        (OperatingSystem::MacOs, _) => (
            "cc-switch-macos",
            None,
            DownloadPackageKind::Dmg,
            "CC-Switch.dmg",
        ),
        (OperatingSystem::Windows, CpuArchitecture::Arm64) => (
            "cc-switch-windows-arm64",
            Some(CpuArchitecture::Arm64),
            DownloadPackageKind::Msi,
            "CC-Switch-arm64.msi",
        ),
        (OperatingSystem::Windows, CpuArchitecture::X64) => (
            "cc-switch-windows-x64",
            Some(CpuArchitecture::X64),
            DownloadPackageKind::Msi,
            "CC-Switch-x64.msi",
        ),
    };
    dynamic_artifact(
        SoftwareProductId::CcSwitch,
        id,
        operating_system,
        architecture,
        DownloadCompatibility::Native,
        kind,
        file_name,
        None,
        ArtifactSource::CcSwitch,
        CC_SWITCH_OFFICIAL_PAGE,
        &[
            "github.com",
            "release-assets.githubusercontent.com",
            "objects.githubusercontent.com",
        ],
    )
}

fn node_artifact(
    operating_system: OperatingSystem,
    cpu_architecture: CpuArchitecture,
) -> Result<TrustedDownloadArtifact, CatalogError> {
    dynamic_artifact(
        SoftwareProductId::NodeJsLts,
        match (operating_system, cpu_architecture) {
            (OperatingSystem::MacOs, _) => "node-lts-macos",
            (OperatingSystem::Windows, CpuArchitecture::Arm64) => "node-lts-windows-arm64",
            (OperatingSystem::Windows, CpuArchitecture::X64) => "node-lts-windows-x64",
        },
        operating_system,
        match operating_system {
            OperatingSystem::MacOs => None,
            OperatingSystem::Windows => Some(cpu_architecture),
        },
        DownloadCompatibility::Native,
        match operating_system {
            OperatingSystem::MacOs => DownloadPackageKind::Pkg,
            OperatingSystem::Windows => DownloadPackageKind::Msi,
        },
        match operating_system {
            OperatingSystem::MacOs => "Node.js LTS.pkg",
            OperatingSystem::Windows => "Node.js LTS.msi",
        },
        None,
        ArtifactSource::NodeLts,
        NODE_OFFICIAL_PAGE,
        &["nodejs.org"],
    )
}

fn vscode_artifact(
    operating_system: OperatingSystem,
    cpu_architecture: CpuArchitecture,
) -> Result<TrustedDownloadArtifact, CatalogError> {
    match operating_system {
        OperatingSystem::MacOs => fixed_artifact(
            SoftwareProductId::VisualStudioCode,
            "vscode-macos-universal",
            OperatingSystem::MacOs,
            None,
            DownloadCompatibility::Native,
            DownloadPackageKind::Zip,
            "Visual Studio Code.zip",
            Some("macOS 11"),
            VSCODE_MACOS_URL,
            VSCODE_OFFICIAL_PAGE,
            &[
                "update.code.visualstudio.com",
                "vscode.download.prss.microsoft.com",
            ],
        ),
        OperatingSystem::Windows => fixed_artifact(
            SoftwareProductId::VisualStudioCode,
            match cpu_architecture {
                CpuArchitecture::Arm64 => "vscode-windows-arm64-user",
                CpuArchitecture::X64 => "vscode-windows-x64-user",
            },
            OperatingSystem::Windows,
            Some(cpu_architecture),
            DownloadCompatibility::Native,
            DownloadPackageKind::ExeBootstrapper,
            "Visual Studio Code Setup.exe",
            Some("Windows 10"),
            match cpu_architecture {
                CpuArchitecture::Arm64 => VSCODE_WINDOWS_ARM64_URL,
                CpuArchitecture::X64 => VSCODE_WINDOWS_X64_URL,
            },
            VSCODE_OFFICIAL_PAGE,
            &[
                "update.code.visualstudio.com",
                "vscode.download.prss.microsoft.com",
            ],
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn fixed_artifact(
    product_id: SoftwareProductId,
    id: &str,
    operating_system: OperatingSystem,
    native_architecture: Option<CpuArchitecture>,
    compatibility: DownloadCompatibility,
    package_kind: DownloadPackageKind,
    file_name: &str,
    minimum_os: Option<&str>,
    source_url: &str,
    official_page: &str,
    allowed_hosts: &[&str],
) -> Result<TrustedDownloadArtifact, CatalogError> {
    dynamic_artifact(
        product_id,
        id,
        operating_system,
        native_architecture,
        compatibility,
        package_kind,
        file_name,
        minimum_os,
        ArtifactSource::Fixed(parse_trusted_url(source_url, allowed_hosts)?),
        official_page,
        allowed_hosts,
    )
}

#[allow(clippy::too_many_arguments)]
fn dynamic_artifact(
    product_id: SoftwareProductId,
    id: &str,
    operating_system: OperatingSystem,
    native_architecture: Option<CpuArchitecture>,
    compatibility: DownloadCompatibility,
    package_kind: DownloadPackageKind,
    file_name: &str,
    minimum_os: Option<&str>,
    source: ArtifactSource,
    official_page: &str,
    allowed_hosts: &[&str],
) -> Result<TrustedDownloadArtifact, CatalogError> {
    let allowed_redirect_hosts = allowed_hosts
        .iter()
        .map(|host| (*host).to_owned())
        .collect::<Vec<_>>()
        .into();
    let official_host = Url::parse(official_page)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .ok_or(CatalogError::InvalidUrl)?;
    Ok(TrustedDownloadArtifact {
        product_id,
        summary: artifact_summary(
            id,
            operating_system,
            native_architecture,
            compatibility,
            package_kind,
            file_name,
            minimum_os,
        ),
        source,
        official_page_url: parse_trusted_url(official_page, &[official_host.as_str()])?,
        allowed_redirect_hosts,
        maximum_size_bytes: MAX_INSTALLER_BYTES,
    })
}

#[allow(clippy::too_many_arguments)]
fn artifact_summary(
    id: &str,
    operating_system: OperatingSystem,
    native_architecture: Option<CpuArchitecture>,
    compatibility: DownloadCompatibility,
    package_kind: DownloadPackageKind,
    file_name: &str,
    minimum_os: Option<&str>,
) -> SoftwareArtifactSummary {
    SoftwareArtifactSummary {
        id: id.to_owned(),
        operating_system,
        native_architecture,
        compatibility,
        package_kind,
        file_name: file_name.to_owned(),
        minimum_os: minimum_os.map(str::to_owned),
        available: true,
    }
}

async fn resolve_claude_mac_url() -> Result<Url, CatalogError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Releases {
        current_release: String,
        releases: Vec<Release>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Release {
        version: String,
        update_to: UpdateTo,
    }
    #[derive(Deserialize)]
    struct UpdateTo {
        version: String,
        url: String,
    }

    let releases: Releases = fetch_json(CLAUDE_MAC_RELEASES_URL, &["downloads.claude.ai"]).await?;
    let selected = releases
        .releases
        .iter()
        .find(|release| {
            release.version == releases.current_release
                || release.update_to.version == releases.current_release
        })
        .or_else(|| releases.releases.first())
        .ok_or(CatalogError::AssetNotFound)?;
    let mut url = parse_trusted_url(&selected.update_to.url, &["downloads.claude.ai"])?;
    if !url.path().to_ascii_lowercase().ends_with(".zip") {
        return Err(CatalogError::InvalidMetadata);
    }
    let path = url.path().trim_end_matches(".zip").to_owned() + ".dmg";
    url.set_path(&path);
    Ok(url)
}

async fn resolve_cc_switch_url(
    operating_system: OperatingSystem,
    native_architecture: Option<CpuArchitecture>,
) -> Result<Url, CatalogError> {
    #[derive(Deserialize)]
    struct Release {
        assets: Vec<Asset>,
    }
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }

    let release: Release = fetch_json(CC_SWITCH_RELEASES_URL, &["api.github.com"]).await?;
    let expected_suffix = match (operating_system, native_architecture) {
        (OperatingSystem::MacOs, _) => "-macOS.dmg",
        (OperatingSystem::Windows, Some(CpuArchitecture::Arm64)) => "-Windows-arm64.msi",
        (OperatingSystem::Windows, _) => "-Windows.msi",
    };
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name.ends_with(expected_suffix))
        .ok_or(CatalogError::AssetNotFound)?;
    parse_trusted_url(&asset.browser_download_url, &["github.com"])
}

async fn resolve_node_lts_url(
    operating_system: OperatingSystem,
    native_architecture: Option<CpuArchitecture>,
) -> Result<Url, CatalogError> {
    #[derive(Deserialize)]
    struct Release {
        version: String,
        lts: serde_json::Value,
    }
    let releases: Vec<Release> = fetch_json(NODE_RELEASES_URL, &["nodejs.org"]).await?;
    let release = releases
        .into_iter()
        .find(|release| match &release.lts {
            serde_json::Value::String(value) => !value.trim().is_empty(),
            serde_json::Value::Bool(value) => *value,
            _ => false,
        })
        .ok_or(CatalogError::AssetNotFound)?;
    let file_name = match operating_system {
        OperatingSystem::MacOs => format!("node-{}.pkg", release.version),
        OperatingSystem::Windows => {
            let architecture = native_architecture.ok_or(CatalogError::UnsupportedHost)?;
            format!(
                "node-{}-{}.msi",
                release.version,
                match architecture {
                    CpuArchitecture::Arm64 => "arm64",
                    CpuArchitecture::X64 => "x64",
                }
            )
        }
    };
    parse_trusted_url(
        &format!("https://nodejs.org/dist/{}/{}", release.version, file_name),
        &["nodejs.org"],
    )
}

async fn fetch_json<T>(url: &str, allowed_hosts: &[&str]) -> Result<T, CatalogError>
where
    T: for<'de> Deserialize<'de>,
{
    let url = parse_trusted_url(url, allowed_hosts)?;
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .redirect(Policy::none())
        .build()
        .map_err(CatalogError::Client)?;
    let response = client
        .get(url)
        .header("Accept", "application/json")
        .header("User-Agent", "dada-assistant/1.0")
        .send()
        .await
        .map_err(|_| CatalogError::Request)?;
    if !response.status().is_success() {
        return Err(CatalogError::Request);
    }
    let bytes = response.bytes().await.map_err(|_| CatalogError::Request)?;
    if bytes.len() > MAX_METADATA_BYTES {
        return Err(CatalogError::InvalidMetadata);
    }
    serde_json::from_slice(&bytes).map_err(|_| CatalogError::InvalidMetadata)
}

fn parse_trusted_url(value: &str, allowed_hosts: &[&str]) -> Result<Url, CatalogError> {
    let url = Url::parse(value).map_err(|_| CatalogError::InvalidUrl)?;
    let allowed_hosts = allowed_hosts
        .iter()
        .map(|host| (*host).to_owned())
        .collect::<Vec<_>>();
    if trusted_download_url(&url, &allowed_hosts) {
        Ok(url)
    } else {
        Err(CatalogError::InvalidUrl)
    }
}

fn parse_official_page(value: &str) -> Result<Url, CatalogError> {
    let url = Url::parse(value).map_err(|_| CatalogError::InvalidUrl)?;
    let host = url.host_str().ok_or(CatalogError::InvalidUrl)?.to_owned();
    if trusted_download_url(&url, &[host]) {
        Ok(url)
    } else {
        Err(CatalogError::InvalidUrl)
    }
}

fn trusted_download_url(url: &Url, allowed_hosts: &[String]) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host_allowed = allowed_hosts.iter().any(|allowed| {
        host == allowed
            || host.strip_suffix(allowed).is_some_and(|prefix| {
                prefix.ends_with('.') && !prefix[..prefix.len() - 1].is_empty()
            })
    });
    let scheme_allowed = url.scheme() == "https"
        || (url.scheme() == "http" && host.ends_with("dl.delivery.mp.microsoft.com"));
    scheme_allowed
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && host_allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_all_desktop_installers() {
        let catalog = official_download_catalog(OperatingSystem::MacOs, CpuArchitecture::Arm64)
            .expect("catalog");
        let ids = catalog
            .products
            .iter()
            .map(|product| product.id)
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                SoftwareProductId::ChatGptDesktop,
                SoftwareProductId::ClaudeDesktop,
                SoftwareProductId::CcSwitch,
                SoftwareProductId::NodeJsLts,
                SoftwareProductId::VisualStudioCode,
            ]
        );
        assert!(catalog
            .products
            .iter()
            .all(|product| product.artifacts.len() == 1 && product.artifacts[0].available));
    }

    #[test]
    fn selects_architecture_specific_artifacts() {
        let cc_arm = resolve_official_artifact(
            SoftwareProductId::CcSwitch,
            None,
            OperatingSystem::Windows,
            CpuArchitecture::Arm64,
        )
        .expect("cc switch arm");
        let node_x64 = resolve_official_artifact(
            SoftwareProductId::NodeJsLts,
            None,
            OperatingSystem::Windows,
            CpuArchitecture::X64,
        )
        .expect("node x64");
        let vscode_arm = resolve_official_artifact(
            SoftwareProductId::VisualStudioCode,
            None,
            OperatingSystem::Windows,
            CpuArchitecture::Arm64,
        )
        .expect("vscode arm");
        let chatgpt_arm = resolve_official_artifact(
            SoftwareProductId::ChatGptDesktop,
            None,
            OperatingSystem::Windows,
            CpuArchitecture::Arm64,
        )
        .expect("chatgpt arm");
        assert_eq!(chatgpt_arm.summary.id, "chatgpt-windows-arm64-msix");
        assert_eq!(chatgpt_arm.summary.package_kind, DownloadPackageKind::Msix);
        assert_eq!(cc_arm.summary.id, "cc-switch-windows-arm64");
        assert_eq!(node_x64.summary.id, "node-lts-windows-x64");
        assert_eq!(vscode_arm.summary.id, "vscode-windows-arm64-user");
    }

    #[tokio::test]
    #[ignore = "live vendor metadata integration"]
    async fn resolves_live_official_metadata() {
        for product in [
            SoftwareProductId::ClaudeDesktop,
            SoftwareProductId::CcSwitch,
            SoftwareProductId::NodeJsLts,
            SoftwareProductId::VisualStudioCode,
        ] {
            let artifact = resolve_official_artifact(
                product,
                None,
                OperatingSystem::MacOs,
                CpuArchitecture::Arm64,
            )
            .expect("artifact");
            let url = artifact.resolve_source_url().await.expect("official url");
            assert!(trusted_download_url(&url, &artifact.allowed_redirect_hosts));
        }
    }

    #[tokio::test]
    #[ignore = "live vendor metadata integration"]
    async fn resolves_windows_arm64_official_metadata() {
        for product in [
            SoftwareProductId::ClaudeDesktop,
            SoftwareProductId::CcSwitch,
            SoftwareProductId::NodeJsLts,
            SoftwareProductId::VisualStudioCode,
        ] {
            let artifact = resolve_official_artifact(
                product,
                None,
                OperatingSystem::Windows,
                CpuArchitecture::Arm64,
            )
            .expect("artifact");
            let url = artifact.resolve_source_url().await.expect("official url");
            assert!(trusted_download_url(&url, &artifact.allowed_redirect_hosts));
        }
    }

    #[tokio::test]
    #[ignore = "live Microsoft Store SOAP integration"]
    async fn resolves_windows_arm64_chatgpt_msix() {
        let artifact = resolve_official_artifact(
            SoftwareProductId::ChatGptDesktop,
            None,
            OperatingSystem::Windows,
            CpuArchitecture::Arm64,
        )
        .expect("artifact");
        let url = artifact.resolve_source_url().await.expect("official url");
        assert!(trusted_download_url(&url, &artifact.allowed_redirect_hosts));
    }
}
