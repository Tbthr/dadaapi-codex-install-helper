<script setup lang="ts">
import { PhCheck, PhDownloadSimple } from "@phosphor-icons/vue";
import { onMounted, ref } from "vue";
import BrandIcon from "../components/BrandIcon.vue";
import { getCliToolsOverview } from "../services/cli-tools";
import { getSoftwareInstallationStatuses } from "../services/software-status";
import type { CliToolStatus } from "../types/cli-tools";
import type { InstalledSoftwareId } from "../types/software-status";

type ToolTab = "desktop" | "cli";
const activeTab = ref<ToolTab>("desktop");
const installationStatuses = ref<Partial<Record<InstalledSoftwareId, boolean>>>({});
const cliStatuses = ref<CliToolStatus[]>([]);

const desktopTools = [
  {
    name: "ChatGPT",
    id: "chatGpt" as const,
    publisher: "OpenAI",
    note: "",
    brand: "openai" as const,
  },
  {
    name: "Claude Desktop",
    id: "claudeDesktop" as const,
    publisher: "Anthropic",
    note: "需要本机自备可访问 Claude 服务的外网环境",
    brand: "claude" as const,
  },
  {
    name: "CC Switch",
    id: "ccSwitch" as const,
    publisher: "CC Switch",
    note: "",
    brand: "ccSwitch" as const,
  },
  {
    name: "Node.js LTS",
    id: "nodeJsLts" as const,
    publisher: "OpenJS Foundation",
    note: "",
    brand: "node" as const,
  },
  {
    name: "Visual Studio Code",
    id: "visualStudioCode" as const,
    publisher: "Microsoft",
    note: "",
    brand: "vscode" as const,
  },
];

const cliTools = [
  {
    id: "codexCli" as const,
    name: "Codex CLI",
    publisher: "OpenAI",
    brand: "openai" as const,
  },
  {
    id: "claudeCodeCli" as const,
    name: "Claude Code CLI",
    publisher: "Anthropic",
    brand: "claudeCode" as const,
  },
];

onMounted(async () => {
  const [software, cli] = await Promise.allSettled([
    getSoftwareInstallationStatuses(),
    getCliToolsOverview(),
  ]);
  if (software.status === "fulfilled") {
    installationStatuses.value = Object.fromEntries(
      software.value.map((item) => [item.id, item.installed]),
    );
  }
  if (cli.status === "fulfilled") {
    cliStatuses.value = cli.value.tools;
  }
});

function installedState(id: InstalledSoftwareId): boolean | undefined {
  return installationStatuses.value[id];
}

function statusLabel(id: InstalledSoftwareId): string {
  const installed = installedState(id);
  return installed === undefined ? "检测中" : installed ? "已安装" : "可安装";
}

function cliStatus(id: "codexCli" | "claudeCodeCli"): CliToolStatus | undefined {
  return cliStatuses.value.find((item) => item.id === id);
}
</script>

<template>
  <div class="page software-page">
    <header class="page-header with-tabs">
      <div>
        <span class="eyebrow">官方来源</span>
        <h1>软件工具</h1>
        <p>通过本地网络从对应软件的官方地址下载。</p>
      </div>
      <div class="segmented-control" aria-label="工具类型">
        <button
          type="button"
          :class="{ active: activeTab === 'desktop' }"
          @click="activeTab = 'desktop'"
        >
          桌面应用
        </button>
        <button type="button" :class="{ active: activeTab === 'cli' }" @click="activeTab = 'cli'">
          命令行工具
        </button>
      </div>
    </header>

    <section class="software-grid">
      <template v-if="activeTab === 'desktop'">
        <article v-for="tool in desktopTools" :key="tool.name" class="software-card">
          <div class="software-card-top">
            <span :class="['software-logo', `brand-${tool.brand}`]">
              <BrandIcon :brand="tool.brand" :size="30" />
            </span>
            <span :class="['software-state', { installed: installedState(tool.id) }]">
              <PhCheck v-if="installedState(tool.id)" :size="13" weight="bold" />
              {{ statusLabel(tool.id) }}
            </span>
          </div>
          <div class="software-copy">
            <strong>{{ tool.name }}</strong>
            <span>{{ tool.publisher }}</span>
            <small v-if="tool.note">{{ tool.note }}</small>
          </div>
          <button type="button" class="software-action">
            <PhDownloadSimple :size="16" />{{ installedState(tool.id) ? "重新下载" : "下载" }}
          </button>
        </article>
      </template>

      <template v-else>
        <article v-for="tool in cliTools" :key="tool.name" class="software-card cli-card">
          <div class="software-card-top">
            <span :class="['software-logo', `brand-${tool.brand}`]">
              <BrandIcon :brand="tool.brand" :size="30" />
            </span>
            <span :class="['software-state', { installed: cliStatus(tool.id)?.installed }]">
              <PhCheck v-if="cliStatus(tool.id)?.installed" :size="13" weight="bold" />
              {{ cliStatus(tool.id)?.installed ? "已安装" : cliStatus(tool.id) ? "可安装" : "检测中" }}
            </span>
          </div>
          <div class="software-copy">
            <strong>{{ tool.name }}</strong>
            <span>
              {{ tool.publisher }}<template v-if="cliStatus(tool.id)?.version">
                · {{ cliStatus(tool.id)?.version }}</template>
            </span>
          </div>
          <button type="button" class="software-action">
            {{ cliStatus(tool.id)?.installed ? "重新安装" : "安装" }}
          </button>
        </article>
      </template>
    </section>
  </div>
</template>
