use shared_types::{DesktopApp, DesktopProduct};
use std::collections::HashSet;
use std::path::Path;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::process::Command;
#[cfg(target_os = "macos")]
use std::process::Stdio;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("桌面应用检测失败：{0}")]
    Platform(String),
    #[error("当前系统暂不支持桌面应用检测")]
    UnsupportedPlatform,
}

pub trait DesktopDiscovery: Send + Sync {
    fn detect(&self) -> Result<Vec<DesktopApp>, DiscoveryError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemDesktopDiscovery;

impl DesktopDiscovery for SystemDesktopDiscovery {
    fn detect(&self) -> Result<Vec<DesktopApp>, DiscoveryError> {
        detect_installed_apps()
    }
}

pub fn detect_installed_apps() -> Result<Vec<DesktopApp>, DiscoveryError> {
    #[cfg(target_os = "macos")]
    {
        return detect_macos_apps();
    }

    #[cfg(target_os = "windows")]
    {
        return detect_windows_apps();
    }

    #[allow(unreachable_code)]
    Err(DiscoveryError::UnsupportedPlatform)
}

fn product_from_name(value: &str) -> DesktopProduct {
    if value.to_ascii_lowercase().contains("chatgpt") {
        DesktopProduct::ChatGpt
    } else {
        DesktopProduct::Codex
    }
}

fn sort_apps(apps: &mut [DesktopApp]) {
    apps.sort_by(|left, right| {
        let left_order = match left.product {
            DesktopProduct::ChatGpt => 0,
            DesktopProduct::Codex => 1,
        };
        let right_order = match right.product {
            DesktopProduct::ChatGpt => 0,
            DesktopProduct::Codex => 1,
        };
        left_order
            .cmp(&right_order)
            .then_with(|| left.install_path.cmp(&right.install_path))
    });
}

#[cfg(target_os = "macos")]
fn detect_macos_apps() -> Result<Vec<DesktopApp>, DiscoveryError> {
    let mut candidates = macos_known_candidates();
    candidates.extend(macos_spotlight_candidates());
    candidates.extend(macos_running_candidates());

    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for candidate in candidates {
        let Some(bundle_path) = normalize_macos_bundle_path(&candidate) else {
            continue;
        };
        if let Ok(Some(app)) = inspect_macos_bundle(&bundle_path) {
            let identity = macos_app_identity(&app);
            if !seen.insert(identity) {
                continue;
            }
            apps.push(app);
        }
    }

    sort_apps(&mut apps);
    Ok(apps)
}

#[cfg(target_os = "macos")]
fn macos_app_identity(app: &DesktopApp) -> String {
    fs_identity(&app.executable_path).unwrap_or_else(|| app.install_path.to_ascii_lowercase())
}

#[cfg(target_os = "macos")]
fn fs_identity(path: &str) -> Option<String> {
    use std::os::unix::fs::MetadataExt;

    let metadata = std::fs::metadata(path).ok()?;
    Some(format!("{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(target_os = "macos")]
fn macos_known_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("/Applications/ChatGPT.app"),
        PathBuf::from("/Applications/Codex.app"),
        PathBuf::from("/Applications/OpenAI Codex.app"),
        PathBuf::from("/System/Volumes/Data/Applications/ChatGPT.app"),
        PathBuf::from("/System/Volumes/Data/Applications/Codex.app"),
        PathBuf::from("/System/Volumes/Data/Applications/OpenAI Codex.app"),
    ];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("Applications/ChatGPT.app"));
        candidates.push(home.join("Applications/Codex.app"));
        candidates.push(home.join("Applications/OpenAI Codex.app"));
    }
    candidates
}

#[cfg(target_os = "macos")]
fn macos_spotlight_candidates() -> Vec<PathBuf> {
    let queries = [
        r#"kMDItemCFBundleIdentifier == "com.openai.codex""#,
        r#"kMDItemCFBundleIdentifier == "com.openai.chatgpt""#,
        r#"kMDItemFSName == "ChatGPT.app""#,
        r#"kMDItemFSName == "Codex.app""#,
    ];
    let mut candidates = Vec::new();
    for query in queries {
        let Ok(output) = Command::new("mdfind").arg(query).output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        candidates.extend(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(PathBuf::from),
        );
    }
    candidates
}

#[cfg(target_os = "macos")]
fn macos_running_candidates() -> Vec<PathBuf> {
    let Ok(output) = Command::new("ps").args(["-axo", "comm="]).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.contains(".app/Contents/MacOS/")
                && (line.to_ascii_lowercase().contains("chatgpt")
                    || line.to_ascii_lowercase().contains("codex"))
        })
        .map(PathBuf::from)
        .collect()
}

#[cfg(target_os = "macos")]
fn normalize_macos_bundle_path(candidate: &Path) -> Option<PathBuf> {
    let raw = candidate.to_string_lossy();
    let raw = raw.trim().trim_matches('"');
    let lower = raw.to_ascii_lowercase();
    let end = lower.find(".app").map(|index| index + 4)?;
    let path = PathBuf::from(&raw[..end]);
    path.is_dir().then_some(path)
}

#[cfg(target_os = "macos")]
fn inspect_macos_bundle(path: &Path) -> Result<Option<DesktopApp>, DiscoveryError> {
    let info_path = path.join("Contents/Info.plist");
    let value = plist::Value::from_file(&info_path).map_err(|error| {
        DiscoveryError::Platform(format!("无法读取 {}：{error}", info_path.display()))
    })?;
    let Some(dictionary) = value.as_dictionary() else {
        return Ok(None);
    };

    let bundle_identifier = dictionary
        .get("CFBundleIdentifier")
        .and_then(plist::Value::as_string)
        .map(str::to_owned);
    let display_name = dictionary
        .get("CFBundleDisplayName")
        .or_else(|| dictionary.get("CFBundleName"))
        .and_then(plist::Value::as_string)
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("ChatGPT")
        })
        .to_owned();
    let executable_name = dictionary
        .get("CFBundleExecutable")
        .and_then(plist::Value::as_string)
        .unwrap_or(display_name.as_str());

    let supported_identifier = bundle_identifier.as_deref().is_some_and(|identifier| {
        identifier.eq_ignore_ascii_case("com.openai.codex")
            || identifier.eq_ignore_ascii_case("com.openai.chatgpt")
    });
    let supported_name = [display_name.as_str(), executable_name].iter().any(|name| {
        let lower = name.to_ascii_lowercase();
        lower.contains("chatgpt") || lower.contains("codex")
    });
    if !supported_identifier && !supported_name {
        return Ok(None);
    }

    let executable_path = path.join("Contents/MacOS").join(executable_name);
    let version = dictionary
        .get("CFBundleShortVersionString")
        .and_then(plist::Value::as_string)
        .map(str::to_owned);
    let running = Command::new("pgrep")
        .args(["-x", executable_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());

    Ok(Some(DesktopApp {
        product: product_from_name(&display_name),
        display_name,
        install_path: path.to_string_lossy().into_owned(),
        executable_path: executable_path.to_string_lossy().into_owned(),
        bundle_identifier,
        version,
        running,
    }))
}

#[cfg(target_os = "windows")]
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WindowsCandidate {
    path: String,
    version: Option<String>,
}

#[cfg(target_os = "windows")]
fn detect_windows_apps() -> Result<Vec<DesktopApp>, DiscoveryError> {
    let script = r#"
$seen = @{}
function Emit-DesktopApp([string]$path) {
  if ([string]::IsNullOrWhiteSpace($path)) { return }
  $path = $path.Trim('"')
  if ($seen.ContainsKey($path)) { return }
  if ($path -notmatch '\\(Codex|ChatGPT)\.exe$') { return }
  if ($path -match '\\resources\\(Codex|ChatGPT)\.exe$') { return }
  if (-not (Test-Path -LiteralPath $path)) { return }
  $seen[$path] = $true
  $version = $null
  try { $version = (Get-Item -LiteralPath $path).VersionInfo.ProductVersion } catch {}
  [pscustomobject]@{ Path = $path; Version = $version } | ConvertTo-Json -Compress
}

Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -in @('Codex.exe', 'ChatGPT.exe') } |
  ForEach-Object { Emit-DesktopApp $_.ExecutablePath }

$packages = @()
$packages += Get-AppxPackage -ErrorAction SilentlyContinue
$packages += Get-AppxPackage -AllUsers -ErrorAction SilentlyContinue
$packages |
  Where-Object { $_.Name -match 'Codex|ChatGPT' -or $_.PackageFamilyName -match 'Codex|ChatGPT' } |
  ForEach-Object {
    Emit-DesktopApp (Join-Path $_.InstallLocation 'app\Codex.exe')
    Emit-DesktopApp (Join-Path $_.InstallLocation 'app\ChatGPT.exe')
    Emit-DesktopApp (Join-Path $_.InstallLocation 'Codex.exe')
    Emit-DesktopApp (Join-Path $_.InstallLocation 'ChatGPT.exe')
  }

$roots = @($env:LOCALAPPDATA, $env:ProgramFiles, ${env:ProgramFiles(x86)}) | Where-Object { $_ }
foreach ($root in $roots) {
  Emit-DesktopApp (Join-Path $root 'Programs\Codex\Codex.exe')
  Emit-DesktopApp (Join-Path $root 'Codex\Codex.exe')
  Emit-DesktopApp (Join-Path $root 'Programs\ChatGPT\ChatGPT.exe')
  Emit-DesktopApp (Join-Path $root 'ChatGPT\ChatGPT.exe')
}
"#;

    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|error| DiscoveryError::Platform(error.to_string()))?;
    if !output.status.success() {
        return Err(DiscoveryError::Platform(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }

    let running_names = windows_running_processes();
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(candidate) = serde_json::from_str::<WindowsCandidate>(line.trim()) else {
            continue;
        };
        let Some(path) = normalize_windows_candidate(&candidate.path) else {
            continue;
        };
        let identity = path.to_ascii_lowercase();
        if !seen.insert(identity) {
            continue;
        }
        let executable_name = Path::new(&path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("ChatGPT.exe");
        let display_name = executable_name.trim_end_matches(".exe").to_owned();
        let install_path = Path::new(&path)
            .parent()
            .unwrap_or_else(|| Path::new(&path))
            .to_string_lossy()
            .into_owned();
        let version = candidate.version.and_then(|version| {
            let version = version.trim().to_owned();
            (!version.is_empty()).then_some(version)
        });
        apps.push(DesktopApp {
            product: product_from_name(&display_name),
            display_name,
            install_path,
            executable_path: path.clone(),
            bundle_identifier: infer_windows_app_id(&path),
            version,
            running: running_names.contains(&executable_name.to_ascii_lowercase()),
        });
    }
    sort_apps(&mut apps);
    Ok(apps)
}

#[cfg(target_os = "windows")]
fn windows_running_processes() -> HashSet<String> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-Process -Name Codex,ChatGPT -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Path",
        ])
        .output();
    let Ok(output) = output else {
        return HashSet::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            Path::new(line.trim())
                .file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase())
        })
        .collect()
}

#[must_use]
pub fn normalize_windows_candidate(candidate: &str) -> Option<String> {
    let candidate = candidate.trim().trim_matches('"');
    if candidate.is_empty() {
        return None;
    }
    let normalized = candidate.replace('/', "\\");
    let lower = normalized.to_ascii_lowercase();
    if !lower.ends_with("\\codex.exe") && !lower.ends_with("\\chatgpt.exe") {
        return None;
    }
    if lower.contains("\\resources\\codex.exe") || lower.contains("\\resources\\chatgpt.exe") {
        return None;
    }
    Some(normalized)
}

#[must_use]
pub fn infer_windows_app_id(app_path: &str) -> Option<String> {
    let cleaned = app_path.trim().trim_matches('"').replace('/', "\\");
    for part in cleaned.split('\\') {
        let lower = part.to_ascii_lowercase();
        if (!lower.starts_with("openai.codex_") && !lower.starts_with("openai.chatgpt_"))
            || !part.contains("__")
        {
            continue;
        }
        let (left, publisher) = part.split_once("__")?;
        let package_name = left.split('_').next()?;
        if !publisher.is_empty() {
            return Some(format!("{package_name}_{publisher}!App"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_chatgpt_and_codex_windows_paths() {
        for input in [
            r"C:\Program Files\OpenAI\Codex\Codex.exe",
            r"C:\Program Files\OpenAI\ChatGPT\ChatGPT.exe",
        ] {
            assert_eq!(normalize_windows_candidate(input).as_deref(), Some(input));
        }
    }

    #[test]
    fn rejects_windows_resource_executables() {
        for input in [
            r"C:\Apps\Codex\resources\Codex.exe",
            r"C:\Apps\ChatGPT\resources\ChatGPT.exe",
            r"C:\Apps\ChatGPT\helper.exe",
        ] {
            assert!(normalize_windows_candidate(input).is_none());
        }
    }

    #[test]
    fn infers_windows_store_app_ids() {
        let chatgpt = r"C:\Program Files\WindowsApps\OpenAI.ChatGPT_26.707.1.0_x64__8wekyb3d8bbwe\app\ChatGPT.exe";
        let codex = r"C:\Program Files\WindowsApps\OpenAI.Codex_26.707.1.0_x64__8wekyb3d8bbwe\app\Codex.exe";
        assert_eq!(
            infer_windows_app_id(chatgpt).as_deref(),
            Some("OpenAI.ChatGPT_8wekyb3d8bbwe!App")
        );
        assert_eq!(
            infer_windows_app_id(codex).as_deref(),
            Some("OpenAI.Codex_8wekyb3d8bbwe!App")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn inspects_renamed_chatgpt_bundle() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let app = temp.path().join("ChatGPT.app");
        let contents = app.join("Contents");
        std::fs::create_dir_all(contents.join("MacOS")).expect("bundle directories");

        let mut info = plist::Dictionary::new();
        info.insert(
            "CFBundleIdentifier".to_owned(),
            plist::Value::String("com.openai.codex".to_owned()),
        );
        info.insert(
            "CFBundleDisplayName".to_owned(),
            plist::Value::String("ChatGPT".to_owned()),
        );
        info.insert(
            "CFBundleExecutable".to_owned(),
            plist::Value::String("ChatGPT".to_owned()),
        );
        info.insert(
            "CFBundleShortVersionString".to_owned(),
            plist::Value::String("26.707.31428".to_owned()),
        );
        plist::Value::Dictionary(info)
            .to_file_xml(contents.join("Info.plist"))
            .expect("write plist");

        let detected = inspect_macos_bundle(&app)
            .expect("valid bundle")
            .expect("supported bundle");
        assert_eq!(detected.product, DesktopProduct::ChatGpt);
        assert_eq!(detected.display_name, "ChatGPT");
        assert_eq!(detected.version.as_deref(), Some("26.707.31428"));
        assert!(detected.executable_path.ends_with("Contents/MacOS/ChatGPT"));
    }
}
