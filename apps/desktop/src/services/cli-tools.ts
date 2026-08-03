import { invoke } from "@tauri-apps/api/core";
import type { CliToolId, CliToolStatus, CliToolsOverview } from "../types/cli-tools";

export function getCliToolsOverview(): Promise<CliToolsOverview> {
  return invoke<CliToolsOverview>("get_cli_tools_overview");
}

export function installCliTool(toolId: CliToolId): Promise<CliToolStatus> {
  return invoke<CliToolStatus>("install_cli_tool", { toolId });
}
