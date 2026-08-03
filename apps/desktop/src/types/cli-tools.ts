export type CliToolId = "codexCli" | "claudeCodeCli";

export interface CliToolStatus {
  id: CliToolId;
  displayName: string;
  installed: boolean;
  version: string | null;
}

export interface CliToolsOverview {
  nodeVersion: string | null;
  npmVersion: string | null;
  tools: CliToolStatus[];
}
