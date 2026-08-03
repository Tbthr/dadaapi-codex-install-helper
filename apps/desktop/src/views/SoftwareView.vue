<script setup lang="ts">
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { PhCheck, PhDownloadSimple, PhSpinnerGap, PhTranslate } from "@phosphor-icons/vue";
import { onMounted, onUnmounted, ref } from "vue";
import { storeToRefs } from "pinia";
import BrandIcon from "../components/BrandIcon.vue";
import { useActivationStore } from "../stores/activation";
import { getCliToolsOverview, installCliTool } from "../services/cli-tools";
import {
  DOWNLOAD_TASK_UPDATED_EVENT,
  cancelDownload,
  getDownloadCatalog,
  launchInstaller,
  listDownloadTasks,
  retryDownload,
  startDownload,
} from "../services/downloads";
import { getSoftwareInstallationStatuses } from "../services/software-status";
import type { CliToolId, CliToolStatus, CliToolsOverview } from "../types/cli-tools";
import type {
  DownloadCatalog,
  DownloadTaskSnapshot,
  SoftwareArtifactSummary,
  SoftwareProductId,
  SoftwareProductSummary,
} from "../types/download";
import { isCommandError } from "../types/locale";
import type { InstalledSoftwareId, SoftwareInstallationStatus } from "../types/software-status";

type Brand = "openai" | "claude" | "claudeCode" | "ccSwitch" | "node" | "vscode";

interface DesktopTool {
  name: string;
  id: InstalledSoftwareId;
  productId: SoftwareProductId;
  publisher: string;
  brand: Brand;
}

const emit = defineEmits<{ openLocale: [trigger: globalThis.HTMLElement] }>();
const activation = useActivationStore();
const { networkPending, running, recoveryRunning } = storeToRefs(activation);
const installationStatuses = ref<Partial<Record<InstalledSoftwareId, SoftwareInstallationStatus>>>(
  {},
);
const cliOverview = ref<CliToolsOverview | null>(null);
const catalog = ref<DownloadCatalog | null>(null);
const tasks = ref<DownloadTaskSnapshot[]>([]);
const downloadBusy = ref<Partial<Record<SoftwareProductId, boolean>>>({});
const downloadErrors = ref<Partial<Record<SoftwareProductId, string>>>({});
const cliBusy = ref<Partial<Record<CliToolId, boolean>>>({});
const cliErrors = ref<Partial<Record<CliToolId, string>>>({});
const pageError = ref("");
let stopDownloadListener: UnlistenFn | null = null;

function openLocale(event: globalThis.MouseEvent): void {
  if (event.currentTarget instanceof globalThis.HTMLElement) {
    emit("openLocale", event.currentTarget);
  }
}

const desktopTools: DesktopTool[] = [
  {
    name: "ChatGPT",
    id: "chatGpt",
    productId: "chatGptDesktop",
    publisher: "OpenAI",
    brand: "openai",
  },
  {
    name: "Claude Desktop",
    id: "claudeDesktop",
    productId: "claudeDesktop",
    publisher: "Anthropic",
    brand: "claude",
  },
  {
    name: "CC Switch",
    id: "ccSwitch",
    productId: "ccSwitch",
    publisher: "CC Switch",
    brand: "ccSwitch",
  },
  {
    name: "Node.js LTS",
    id: "nodeJsLts",
    productId: "nodeJsLts",
    publisher: "OpenJS Foundation",
    brand: "node",
  },
  {
    name: "Visual Studio Code",
    id: "visualStudioCode",
    productId: "visualStudioCode",
    publisher: "Microsoft",
    brand: "vscode",
  },
];

const cliTools = [
  { id: "codexCli" as const, name: "Codex CLI", publisher: "OpenAI", brand: "openai" as const },
  {
    id: "claudeCodeCli" as const,
    name: "Claude Code CLI",
    publisher: "Anthropic",
    brand: "claudeCode" as const,
  },
];

onMounted(async () => {
  try {
    stopDownloadListener = await listen<DownloadTaskSnapshot>(
      DOWNLOAD_TASK_UPDATED_EVENT,
      (event) => updateTask(event.payload),
    );
  } catch (error) {
    pageError.value = errorMessage(error, "无法监听下载进度");
  }
  globalThis.addEventListener("focus", refreshInstallationStatuses);
  await refreshPage();
});

onUnmounted(() => {
  stopDownloadListener?.();
  stopDownloadListener = null;
  globalThis.removeEventListener("focus", refreshInstallationStatuses);
});

async function refreshPage(): Promise<void> {
  const [software, cli, nextCatalog, nextTasks] = await Promise.allSettled([
    getSoftwareInstallationStatuses(),
    getCliToolsOverview(),
    getDownloadCatalog(),
    listDownloadTasks(),
  ]);
  if (software.status === "fulfilled") applyInstallationStatuses(software.value);
  if (cli.status === "fulfilled") cliOverview.value = cli.value;
  if (nextCatalog.status === "fulfilled") {
    catalog.value = nextCatalog.value;
  } else {
    pageError.value = errorMessage(nextCatalog.reason, "无法读取官方下载目录");
  }
  if (nextTasks.status === "fulfilled") {
    tasks.value = nextTasks.value;
  } else {
    pageError.value = errorMessage(nextTasks.reason, "无法读取下载任务");
  }
}

async function refreshInstallationStatuses(): Promise<void> {
  try {
    const [software, cli, nextTasks] = await Promise.all([
      getSoftwareInstallationStatuses(),
      getCliToolsOverview(),
      listDownloadTasks(),
    ]);
    applyInstallationStatuses(software);
    cliOverview.value = cli;
    tasks.value = nextTasks;
  } catch {
    // 保留上一次真实检测结果，避免窗口切回时闪回“可安装”。
  }
}

function applyInstallationStatuses(statuses: SoftwareInstallationStatus[]): void {
  installationStatuses.value = Object.fromEntries(statuses.map((item) => [item.id, item]));
}

function statusFor(id: InstalledSoftwareId): SoftwareInstallationStatus | undefined {
  return installationStatuses.value[id];
}

function installedState(id: InstalledSoftwareId): boolean | undefined {
  return statusFor(id)?.installed;
}

function productFor(productId: SoftwareProductId): SoftwareProductSummary | null {
  return catalog.value?.products.find((product) => product.id === productId) ?? null;
}

function artifactFor(productId: SoftwareProductId): SoftwareArtifactSummary | null {
  return productFor(productId)?.artifacts.find((artifact) => artifact.available) ?? null;
}

function taskFor(productId: SoftwareProductId): DownloadTaskSnapshot | null {
  const artifact = artifactFor(productId);
  return artifact
    ? (tasks.value.find(
        (task) => task.productId === productId && task.artifactId === artifact.id,
      ) ?? null)
    : null;
}

function updateTask(next: DownloadTaskSnapshot): void {
  const existing = tasks.value.some((task) => task.id === next.id);
  tasks.value = existing
    ? tasks.value.map((task) => (task.id === next.id ? next : task))
    : [...tasks.value, next];
}

function progressFor(task: DownloadTaskSnapshot | null): number {
  if (!task?.totalBytes || task.totalBytes <= 0) return 0;
  return Math.min(100, Math.round((task.downloadedBytes / task.totalBytes) * 100));
}

function desktopStatusLabel(tool: DesktopTool): string {
  const task = taskFor(tool.productId);
  if (task?.state === "downloading") return `${progressFor(task)}%`;
  if (task?.state === "resolving" || task?.state === "queued") return "准备中";
  if (task?.state === "failed") return "下载失败";
  if (task?.state === "ready" || task?.state === "launched") {
    return installedState(tool.id) ? "已安装" : "已下载";
  }
  const installed = installedState(tool.id);
  return installed === undefined ? "检测中" : installed ? "已安装" : "可安装";
}

function desktopActionLabel(tool: DesktopTool): string {
  switch (taskFor(tool.productId)?.state) {
    case "queued":
    case "resolving":
    case "downloading":
      return "取消";
    case "cancelled":
    case "failed":
      return "重试";
    case "ready":
    case "launched":
      return "打开安装包";
    case "launching":
      return "正在打开";
    default:
      return installedState(tool.id) ? "重新下载" : "下载";
  }
}

function desktopMessage(tool: DesktopTool): string {
  const error = downloadErrors.value[tool.productId];
  if (error) return error;
  const task = taskFor(tool.productId);
  if (task?.state === "failed") return task.error?.message ?? "下载失败";
  if (task?.state === "downloading") {
    return `${formatBytes(task.downloadedBytes)}${task.totalBytes ? ` / ${formatBytes(task.totalBytes)}` : ""}`;
  }
  if (task?.state === "ready") return "安装包已下载完成";
  if (task?.state === "launched") return "安装包已打开，安装完成后返回此页面会自动重新检测";
  return "";
}

function desktopActionBusy(tool: DesktopTool): boolean {
  return (
    Boolean(downloadBusy.value[tool.productId]) || taskFor(tool.productId)?.state === "launching"
  );
}

async function handleDesktop(tool: DesktopTool): Promise<void> {
  const product = productFor(tool.productId);
  const artifact = artifactFor(tool.productId);
  if (!product || !artifact || downloadBusy.value[tool.productId]) {
    if (!product || !artifact) {
      downloadErrors.value = {
        ...downloadErrors.value,
        [tool.productId]: "当前设备没有可用的官方下载包",
      };
    }
    return;
  }
  downloadBusy.value = { ...downloadBusy.value, [tool.productId]: true };
  downloadErrors.value = { ...downloadErrors.value, [tool.productId]: "" };
  try {
    tasks.value = await listDownloadTasks();
    const current = taskFor(tool.productId);
    let next: DownloadTaskSnapshot;
    if (!current) next = await startDownload(product.id, artifact.id);
    else if (["queued", "resolving", "downloading"].includes(current.state))
      next = await cancelDownload(current.id);
    else if (["cancelled", "failed"].includes(current.state))
      next = await retryDownload(current.id);
    else if (["ready", "launched"].includes(current.state)) {
      next = await launchInstaller(current.id);
      globalThis.setTimeout(() => void refreshInstallationStatuses(), 1500);
    } else return;
    updateTask(next);
  } catch (error) {
    try {
      tasks.value = await listDownloadTasks();
    } catch {
      // 保留当前任务状态，错误信息仍以本次操作为准。
    }
    downloadErrors.value = {
      ...downloadErrors.value,
      [tool.productId]: errorMessage(error, "下载操作失败"),
    };
  } finally {
    downloadBusy.value = { ...downloadBusy.value, [tool.productId]: false };
  }
}

function cliStatus(id: CliToolId): CliToolStatus | undefined {
  return cliOverview.value?.tools.find((item) => item.id === id);
}

function cliStatusLabel(id: CliToolId): string {
  if (cliErrors.value[id]) return "安装失败";
  const status = cliStatus(id);
  if (status?.installed) return "已安装";
  if (!cliOverview.value) return "检测中";
  return "可安装";
}

function cliMessage(id: CliToolId, publisher: string): string {
  const error = cliErrors.value[id];
  if (error) return error;
  const status = cliStatus(id);
  if (status?.version) return `${publisher} · 版本 ${status.version}`;
  if (cliOverview.value && (!cliOverview.value.nodeVersion || !cliOverview.value.npmVersion)) {
    return "需要先安装 Node.js LTS";
  }
  return publisher;
}

async function handleCli(toolId: CliToolId): Promise<void> {
  if (cliBusy.value[toolId]) return;
  if (!cliOverview.value?.nodeVersion || !cliOverview.value?.npmVersion) {
    const nodeTool = desktopTools.find((tool) => tool.productId === "nodeJsLts");
    if (nodeTool) await handleDesktop(nodeTool);
    return;
  }
  cliBusy.value = { ...cliBusy.value, [toolId]: true };
  cliErrors.value = { ...cliErrors.value, [toolId]: "" };
  try {
    const installed = await installCliTool(toolId);
    if (cliOverview.value) {
      cliOverview.value = {
        ...cliOverview.value,
        tools: cliOverview.value.tools.map((tool) => (tool.id === installed.id ? installed : tool)),
      };
    }
  } catch (error) {
    cliErrors.value = { ...cliErrors.value, [toolId]: errorMessage(error, "CLI 安装失败") };
  } finally {
    cliBusy.value = { ...cliBusy.value, [toolId]: false };
  }
}

function formatBytes(bytes: number): string {
  return bytes < 1024 * 1024
    ? `${Math.max(0, Math.round(bytes / 1024))} KB`
    : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function errorMessage(error: unknown, fallback: string): string {
  return isCommandError(error) ? error.message : fallback;
}
</script>

<template>
  <section class="software-page">
    <p v-if="pageError" class="software-page-error" role="alert">{{ pageError }}</p>

    <section class="software-group" aria-labelledby="desktop-tools-title">
      <div class="section-heading">
        <h2 id="desktop-tools-title">桌面应用</h2>
        <span>5 个软件</span>
      </div>
      <div class="software-grid">
        <article v-for="tool in desktopTools" :key="tool.name" class="software-card">
          <div class="software-card-top">
            <span :class="['software-logo', `brand-${tool.brand}`]">
              <BrandIcon :brand="tool.brand" :size="30" />
            </span>
            <span :class="['software-state', { installed: installedState(tool.id) }]">
              <PhCheck v-if="installedState(tool.id)" :size="13" weight="bold" />
              {{ desktopStatusLabel(tool) }}
            </span>
          </div>
          <div class="software-copy">
            <strong>{{ tool.name }}</strong>
            <span>{{ tool.publisher }}</span>
            <span v-if="statusFor(tool.id)?.installed">
              版本 {{ statusFor(tool.id)?.version ?? "未知" }}
            </span>
            <small
              v-if="desktopMessage(tool)"
              :class="{
                error: Boolean(downloadErrors[tool.productId] || taskFor(tool.productId)?.error),
              }"
            >
              {{ desktopMessage(tool) }}
            </small>
            <div
              v-if="taskFor(tool.productId)?.state === 'downloading'"
              class="software-progress"
              aria-hidden="true"
            >
              <i :style="{ width: `${progressFor(taskFor(tool.productId))}%` }" />
            </div>
          </div>
          <div class="software-actions">
            <button
              type="button"
              class="software-action"
              :disabled="desktopActionBusy(tool)"
              @click="handleDesktop(tool)"
            >
              <PhSpinnerGap v-if="desktopActionBusy(tool)" class="spinning" :size="16" />
              <PhDownloadSimple v-else :size="16" />
              {{ desktopActionLabel(tool) }}
            </button>
            <button
              v-if="tool.id === 'chatGpt'"
              type="button"
              class="software-action locale-action"
              data-testid="configure-chinese"
              :disabled="!installedState(tool.id)"
              @click="openLocale"
            >
              <PhSpinnerGap v-if="running || recoveryRunning" class="spinning" :size="16" />
              <PhTranslate v-else :size="16" />
              {{ networkPending ? "恢复原网络" : running ? "查看进度" : "配置中文" }}
            </button>
          </div>
        </article>
      </div>
    </section>

    <section class="software-group" aria-labelledby="cli-tools-title">
      <div class="section-heading">
        <h2 id="cli-tools-title">命令行工具</h2>
        <span>需要 Node.js LTS</span>
      </div>
      <div class="software-grid cli-grid">
        <article v-for="tool in cliTools" :key="tool.name" class="software-card cli-card">
          <div class="software-card-top">
            <span :class="['software-logo', `brand-${tool.brand}`]">
              <BrandIcon :brand="tool.brand" :size="30" />
            </span>
            <span :class="['software-state', { installed: cliStatus(tool.id)?.installed }]">
              <PhCheck v-if="cliStatus(tool.id)?.installed" :size="13" weight="bold" />
              {{ cliStatusLabel(tool.id) }}
            </span>
          </div>
          <div class="software-copy">
            <strong>{{ tool.name }}</strong>
            <span>{{ cliMessage(tool.id, tool.publisher) }}</span>
          </div>
          <button
            type="button"
            class="software-action"
            :disabled="cliBusy[tool.id]"
            @click="handleCli(tool.id)"
          >
            <PhSpinnerGap v-if="cliBusy[tool.id]" class="spinning" :size="16" />
            {{
              cliStatus(tool.id)?.installed
                ? "重新安装"
                : cliOverview?.nodeVersion && cliOverview?.npmVersion
                  ? "安装"
                  : "安装 Node"
            }}
          </button>
        </article>
      </div>
    </section>
  </section>
</template>
