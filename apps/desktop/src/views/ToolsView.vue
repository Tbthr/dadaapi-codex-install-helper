<script setup lang="ts">
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Check,
  CircleAlert,
  Download,
  LoaderCircle,
  Play,
  RotateCcw,
  Terminal,
  X,
} from "lucide-vue-next";
import { computed, onMounted, onUnmounted, ref } from "vue";
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
import { getRepairOverview, restoreLocaleConfiguration } from "../services/locale";
import type { CliToolId, CliToolsOverview } from "../types/cli-tools";
import type {
  DownloadCatalog,
  DownloadTaskSnapshot,
  SoftwareArtifactSummary,
  SoftwareProductId,
  SoftwareProductSummary,
} from "../types/download";
import { isCommandError, type RepairOverview } from "../types/locale";

const catalog = ref<DownloadCatalog | null>(null);
const tasks = ref<DownloadTaskSnapshot[]>([]);
const cliOverview = ref<CliToolsOverview | null>(null);
const repair = ref<RepairOverview | null>(null);
const loading = ref(true);
const downloadBusy = ref<Partial<Record<SoftwareProductId, boolean>>>({});
const downloadErrors = ref<Partial<Record<SoftwareProductId, string>>>({});
const cliBusy = ref<Partial<Record<CliToolId, boolean>>>({});
const cliErrors = ref<Partial<Record<CliToolId, string>>>({});
const localeBusy = ref(false);
const pageError = ref("");
const localeMessage = ref("");
let stopDownloadListener: UnlistenFn | null = null;

const products = computed(() => catalog.value?.products ?? []);
const nodeProduct = computed(
  () => products.value.find((product) => product.id === "nodeJsLts") ?? null,
);

onMounted(async () => {
  stopDownloadListener = await listen<DownloadTaskSnapshot>(DOWNLOAD_TASK_UPDATED_EVENT, (event) =>
    updateTask(event.payload),
  );
  await refreshTools();
});

onUnmounted(() => {
  stopDownloadListener?.();
  stopDownloadListener = null;
});

async function refreshTools(): Promise<void> {
  loading.value = true;
  pageError.value = "";
  try {
    const [nextCatalog, nextTasks, nextCliOverview, nextRepair] = await Promise.all([
      getDownloadCatalog(),
      listDownloadTasks(),
      getCliToolsOverview(),
      getRepairOverview(),
    ]);
    catalog.value = nextCatalog;
    tasks.value = nextTasks;
    cliOverview.value = nextCliOverview;
    repair.value = nextRepair;
  } catch (error) {
    pageError.value = errorMessage(error, "功能状态读取失败");
  } finally {
    loading.value = false;
  }
}

function artifactFor(product: SoftwareProductSummary): SoftwareArtifactSummary | null {
  return product.artifacts.find((item) => item.available) ?? null;
}

function taskFor(product: SoftwareProductSummary): DownloadTaskSnapshot | null {
  const artifact = artifactFor(product);
  if (!artifact) {
    return null;
  }
  return (
    tasks.value.find((item) => item.productId === product.id && item.artifactId === artifact.id) ??
    null
  );
}

function progressFor(task: DownloadTaskSnapshot | null): number {
  if (!task?.totalBytes || task.totalBytes <= 0) {
    return 0;
  }
  return Math.min(100, Math.round((task.downloadedBytes / task.totalBytes) * 100));
}

function downloadStatus(product: SoftwareProductSummary): string {
  const task = taskFor(product);
  const productError = downloadErrors.value[product.id];
  if (productError) {
    return productError;
  }
  if (!task) {
    return artifactFor(product) ? "可下载" : "当前设备不可用";
  }
  switch (task.state) {
    case "queued":
      return "等待下载";
    case "resolving":
      return "正在解析官方下载地址";
    case "downloading":
      return `${progressFor(task)}% · ${formatBytes(task.downloadedBytes)}`;
    case "ready":
      return "下载完成";
    case "launching":
      return "正在打开";
    case "launched":
      return "安装包已打开";
    case "cancelled":
      return "已取消";
    case "failed":
      return task.error?.message ?? "下载失败";
    default:
      return "状态未知";
  }
}

function downloadAction(product: SoftwareProductSummary): string {
  switch (taskFor(product)?.state) {
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
      return "下载";
  }
}

function updateTask(next: DownloadTaskSnapshot): void {
  const index = tasks.value.findIndex((item) => item.id === next.id);
  tasks.value =
    index === -1
      ? [...tasks.value, next]
      : tasks.value.map((item) => (item.id === next.id ? next : item));
}

async function handleDownload(product: SoftwareProductSummary): Promise<void> {
  const artifact = artifactFor(product);
  if (!artifact || downloadBusy.value[product.id]) {
    return;
  }
  downloadBusy.value = { ...downloadBusy.value, [product.id]: true };
  downloadErrors.value = { ...downloadErrors.value, [product.id]: "" };
  try {
    const current = taskFor(product);
    let next: DownloadTaskSnapshot;
    if (!current) {
      next = await startDownload(product.id, artifact.id);
    } else if (["queued", "resolving", "downloading"].includes(current.state)) {
      next = await cancelDownload(current.id);
    } else if (["cancelled", "failed"].includes(current.state)) {
      next = await retryDownload(current.id);
    } else if (["ready", "launched"].includes(current.state)) {
      next = await launchInstaller(current.id);
    } else {
      return;
    }
    updateTask(next);
  } catch (error) {
    downloadErrors.value = {
      ...downloadErrors.value,
      [product.id]: errorMessage(error, "下载操作失败"),
    };
  } finally {
    downloadBusy.value = { ...downloadBusy.value, [product.id]: false };
  }
}

function cliStatus(toolId: CliToolId): string {
  const error = cliErrors.value[toolId];
  if (error) {
    return error;
  }
  const tool = cliOverview.value?.tools.find((item) => item.id === toolId);
  if (tool?.installed) {
    return tool.version ?? "已安装";
  }
  if (!cliOverview.value?.nodeVersion || !cliOverview.value?.npmVersion) {
    return "需要先安装 Node.js LTS";
  }
  return "可安装";
}

async function handleCli(toolId: CliToolId): Promise<void> {
  if (cliBusy.value[toolId]) {
    return;
  }
  if (!cliOverview.value?.nodeVersion || !cliOverview.value?.npmVersion) {
    if (nodeProduct.value) {
      await handleDownload(nodeProduct.value);
    }
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
    cliErrors.value = {
      ...cliErrors.value,
      [toolId]: errorMessage(error, "CLI 安装失败"),
    };
  } finally {
    cliBusy.value = { ...cliBusy.value, [toolId]: false };
  }
}

async function restoreLocale(): Promise<void> {
  if (localeBusy.value || !repair.value?.locale.restoreAvailable) {
    return;
  }
  localeBusy.value = true;
  localeMessage.value = "";
  try {
    const result = await restoreLocaleConfiguration(repair.value.app?.executablePath);
    localeMessage.value = result.restartWarning?.message ?? "中文配置已撤销";
    repair.value = await getRepairOverview();
  } catch (error) {
    localeMessage.value = errorMessage(error, "撤销中文配置失败");
  } finally {
    localeBusy.value = false;
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) {
    return `${Math.max(0, Math.round(bytes / 1024))} KB`;
  }
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function errorMessage(error: unknown, fallback: string): string {
  if (isCommandError(error)) {
    return error.message;
  }
  return fallback;
}
</script>

<template>
  <section class="single-screen" aria-label="工具">
    <article class="tools-card surface-card">
      <header class="tools-header">
        <strong>工具</strong>
        <LoaderCircle v-if="loading" class="spinning" :size="17" />
      </header>

      <div v-for="product in products" :key="product.id" class="tool-row">
        <span class="tool-icon"><Download :size="20" /></span>
        <div class="tool-copy">
          <strong>{{ product.displayName }}</strong>
          <span
            :class="{
              error: Boolean(downloadErrors[product.id] || taskFor(product)?.error),
              success: ['ready', 'launched'].includes(taskFor(product)?.state ?? ''),
            }"
          >
            {{ downloadStatus(product) }}
          </span>
          <div
            v-if="taskFor(product)?.state === 'downloading'"
            class="tool-progress"
            aria-hidden="true"
          >
            <i :style="{ width: `${progressFor(taskFor(product))}%` }" />
          </div>
        </div>
        <button
          class="tool-button"
          type="button"
          :disabled="
            !artifactFor(product) ||
              downloadBusy[product.id] ||
              taskFor(product)?.state === 'launching'
          "
          @click="handleDownload(product)"
        >
          <LoaderCircle v-if="downloadBusy[product.id]" class="spinning" :size="15" />
          <X
            v-else-if="
              ['queued', 'resolving', 'downloading'].includes(taskFor(product)?.state ?? '')
            "
            :size="15"
          />
          <Play
            v-else-if="['ready', 'launched'].includes(taskFor(product)?.state ?? '')"
            :size="15"
          />
          <RotateCcw
            v-else-if="['cancelled', 'failed'].includes(taskFor(product)?.state ?? '')"
            :size="15"
          />
          <Download v-else :size="15" />
          {{ downloadAction(product) }}
        </button>
      </div>

      <div v-for="tool in cliOverview?.tools ?? []" :key="tool.id" class="tool-row">
        <span class="tool-icon"><Terminal :size="20" /></span>
        <div class="tool-copy">
          <strong>{{ tool.displayName }}</strong>
          <span :class="{ success: tool.installed, error: Boolean(cliErrors[tool.id]) }">
            {{ cliStatus(tool.id) }}
          </span>
        </div>
        <button
          class="tool-button"
          type="button"
          :disabled="cliBusy[tool.id] || tool.installed"
          @click="handleCli(tool.id)"
        >
          <LoaderCircle v-if="cliBusy[tool.id]" class="spinning" :size="15" />
          <Check v-else-if="tool.installed" :size="15" />
          <Download v-else :size="15" />
          {{
            tool.installed
              ? "已安装"
              : cliOverview?.nodeVersion && cliOverview?.npmVersion
                ? "安装"
                : "安装 Node"
          }}
        </button>
      </div>

      <div class="tool-row">
        <span class="tool-icon"><RotateCcw :size="20" /></span>
        <div class="tool-copy">
          <strong>恢复</strong>
          <span
            :class="{
              success: localeMessage.includes('已'),
              error: localeMessage.includes('失败'),
            }"
          >
            {{ localeMessage || "中文配置" }}
          </span>
        </div>
        <div class="tool-actions-inline">
          <button
            class="tool-icon-button"
            type="button"
            title="撤销中文配置"
            aria-label="撤销中文配置"
            :disabled="localeBusy || !repair?.locale.restoreAvailable"
            @click="restoreLocale"
          >
            <LoaderCircle v-if="localeBusy" class="spinning" :size="16" />
            <RotateCcw v-else :size="16" />
          </button>
        </div>
      </div>

      <div v-if="pageError" class="tools-error" role="alert">
        <CircleAlert :size="15" />
        {{ pageError }}
      </div>
    </article>
  </section>
</template>
