use cli_tools::{inspect_cli_tools, install_cli_tool as install_tool, CliToolError};
use shared_types::{CliToolId, CliToolStatus, CliToolsOverview, CommandError};

#[tauri::command]
pub async fn get_cli_tools_overview() -> CliToolsOverview {
    inspect_cli_tools().await
}

#[tauri::command]
pub async fn install_cli_tool(tool_id: CliToolId) -> Result<CliToolStatus, CommandError> {
    install_tool(tool_id).await.map_err(command_error)
}

fn command_error(error: CliToolError) -> CommandError {
    let code = match error {
        CliToolError::NodeRequired => "node_required",
        CliToolError::NpmRequired => "npm_required",
        CliToolError::Spawn(_) => "cli_install_spawn_failed",
        CliToolError::TimedOut => "cli_install_timeout",
        CliToolError::InstallFailed => "cli_install_failed",
        CliToolError::InstalledNotDetected => "cli_install_not_detected",
    };
    CommandError {
        code: code.to_owned(),
        message: error.to_string(),
    }
}
