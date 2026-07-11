use std::{env, ffi::OsString, io, path::PathBuf, process::Stdio, time::Duration};

use shared_types::{CliToolId, CliToolStatus, CliToolsOverview};
use thiserror::Error;
use tokio::{process::Command, time::timeout};

const NPM_REGISTRY: &str = "https://registry.npmjs.org";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Error)]
pub enum CliToolError {
    #[error("请先安装 Node.js LTS")]
    NodeRequired,
    #[error("未找到 npm，请重新安装 Node.js LTS")]
    NpmRequired,
    #[error("无法启动 CLI 安装程序")]
    Spawn(#[source] io::Error),
    #[error("CLI 安装超时，请检查本地网络后重试")]
    TimedOut,
    #[error("CLI 安装失败，请检查网络或 npm 全局目录权限")]
    InstallFailed,
    #[error("npm 已完成安装，但暂时无法找到 CLI，请重新打开终端或应用")]
    InstalledNotDetected,
}

#[derive(Debug, Clone, Copy)]
struct ToolDefinition {
    id: CliToolId,
    display_name: &'static str,
    executable_name: &'static str,
    npm_package: &'static str,
    include_optional: bool,
}

const TOOLS: [ToolDefinition; 2] = [
    ToolDefinition {
        id: CliToolId::CodexCli,
        display_name: "Codex CLI",
        executable_name: "codex",
        npm_package: "@openai/codex",
        include_optional: false,
    },
    ToolDefinition {
        id: CliToolId::ClaudeCodeCli,
        display_name: "Claude Code CLI",
        executable_name: "claude",
        npm_package: "@anthropic-ai/claude-code",
        include_optional: true,
    },
];

pub async fn inspect_cli_tools() -> CliToolsOverview {
    let node_version = command_version("node").await;
    let npm_version = command_version(npm_command_name()).await;
    let mut tools = Vec::with_capacity(TOOLS.len());
    for definition in TOOLS {
        let version = command_version(definition.executable_name).await;
        tools.push(CliToolStatus {
            id: definition.id,
            display_name: definition.display_name.to_owned(),
            installed: version.is_some(),
            version,
        });
    }
    CliToolsOverview {
        node_version,
        npm_version,
        tools,
    }
}

pub async fn install_cli_tool(tool_id: CliToolId) -> Result<CliToolStatus, CliToolError> {
    if command_version("node").await.is_none() {
        return Err(CliToolError::NodeRequired);
    }
    let npm = npm_command_name();
    if command_version(npm).await.is_none() {
        return Err(CliToolError::NpmRequired);
    }
    let definition = tool_definition(tool_id);
    let mut command = prepared_command(npm);
    command
        .arg("install")
        .arg("-g")
        .arg(definition.npm_package)
        .arg(format!("--registry={NPM_REGISTRY}"));
    if definition.include_optional {
        command.arg("--include=optional");
    }
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let status = timeout(INSTALL_TIMEOUT, command.status())
        .await
        .map_err(|_| CliToolError::TimedOut)?
        .map_err(CliToolError::Spawn)?;
    if !status.success() {
        return Err(CliToolError::InstallFailed);
    }
    let version = command_version(definition.executable_name).await;
    if version.is_none() {
        return Err(CliToolError::InstalledNotDetected);
    }
    Ok(CliToolStatus {
        id: definition.id,
        display_name: definition.display_name.to_owned(),
        installed: true,
        version,
    })
}

fn tool_definition(tool_id: CliToolId) -> ToolDefinition {
    match tool_id {
        CliToolId::CodexCli => TOOLS[0],
        CliToolId::ClaudeCodeCli => TOOLS[1],
    }
}

async fn command_version(program: &str) -> Option<String> {
    let mut command = prepared_command(program);
    command.arg("--version").stderr(Stdio::null());
    let output = timeout(COMMAND_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!version.is_empty()).then_some(version)
}

fn prepared_command(program: &str) -> Command {
    let resolved = resolve_executable(program).unwrap_or_else(|| PathBuf::from(program));
    let mut command = Command::new(resolved);
    command.env("PATH", augmented_path());
    hide_windows_console(&mut command);
    command
}

fn resolve_executable(program: &str) -> Option<PathBuf> {
    let directories = env::var_os("PATH")
        .into_iter()
        .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .chain(standard_binary_directories());
    let names = executable_file_names(program);
    directories
        .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
        .find(|candidate| candidate.is_file())
}

fn standard_binary_directories() -> impl Iterator<Item = PathBuf> {
    let mut directories = vec![
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/bin"),
    ];
    if let Some(home) = dirs::home_dir() {
        directories.push(home.join(".npm-global/bin"));
        directories.push(home.join(".local/bin"));
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(program_files) = env::var_os("ProgramFiles") {
            directories.push(PathBuf::from(program_files).join("nodejs"));
        }
        if let Some(app_data) = env::var_os("APPDATA") {
            directories.push(PathBuf::from(app_data).join("npm"));
        }
    }
    directories.into_iter()
}

fn augmented_path() -> OsString {
    let mut paths = env::var_os("PATH")
        .into_iter()
        .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    for directory in standard_binary_directories() {
        if !paths.contains(&directory) {
            paths.push(directory);
        }
    }
    env::join_paths(paths).unwrap_or_default()
}

fn executable_file_names(program: &str) -> Vec<OsString> {
    #[cfg(target_os = "windows")]
    {
        if program.contains('.') {
            return vec![OsString::from(program)];
        }
        vec![
            OsString::from(format!("{program}.exe")),
            OsString::from(format!("{program}.cmd")),
        ]
    }
    #[cfg(not(target_os = "windows"))]
    {
        vec![std::ffi::OsStr::new(program).to_owned()]
    }
}

fn npm_command_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "npm.cmd"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "npm"
    }
}

#[cfg(target_os = "windows")]
fn hide_windows_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.as_std_mut().creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn hide_windows_console(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_distinct_package() {
        assert_ne!(TOOLS[0].id, TOOLS[1].id);
        assert_ne!(TOOLS[0].npm_package, TOOLS[1].npm_package);
        assert!(TOOLS.iter().all(|tool| tool.npm_package.starts_with('@')));
    }

    #[test]
    fn registry_is_the_official_npm_registry() {
        assert_eq!(NPM_REGISTRY, "https://registry.npmjs.org");
    }
}
