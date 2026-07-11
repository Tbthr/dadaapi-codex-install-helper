<script setup lang="ts">
import { PhDownloadSimple, PhPlay, PhSpinnerGap, PhTranslate } from "@phosphor-icons/vue";
import { storeToRefs } from "pinia";
import { computed } from "vue";
import BrandIcon from "../components/BrandIcon.vue";
import { useActivationStore } from "../stores/activation";
import type { LocaleOverview } from "../types/locale";
import type { WorkspacePage } from "../types/ui";

const props = defineProps<{
  overview: LocaleOverview | null;
  loading: boolean;
  error: string;
}>();

const emit = defineEmits<{
  navigate: [page: WorkspacePage];
  refresh: [];
}>();

const activation = useActivationStore();
const {
  canActivate,
  running,
  message,
  error: activationError,
  networkPending,
  recoveryRunning,
  recoveryError,
} = storeToRefs(activation);
const app = computed(() => props.overview?.apps[0] ?? null);
const appInstalled = computed(() => Boolean(app.value));
const appReady = computed(() => Boolean(app.value?.running));

const appStatusLabel = computed(() => {
  if (props.loading) return "检测中";
  if (props.error) return "检测失败";
  if (!app.value) return "未安装";
  return app.value.running ? "运行中" : "已安装";
});

const primaryLabel = computed(() => {
  if (!appInstalled.value) return "去安装";
  if (running.value) return "正在设置";
  if (recoveryRunning.value) return "正在恢复";
  if (activationError.value || networkPending.value) return "恢复网络";
  if (!appReady.value) return "请先打开应用";
  return props.overview?.locale.chineseEnabled ? "重新设置" : "开始设置";
});

const primaryDisabled = computed(() => {
  if (!appInstalled.value) return false;
  if (running.value || recoveryRunning.value) return true;
  if (activationError.value || networkPending.value) return false;
  if (!appReady.value) return true;
  return !canActivate.value;
});

async function handlePrimaryAction(): Promise<void> {
  if (!app.value) {
    emit("navigate", "software");
    return;
  }
  if (running.value || recoveryRunning.value) {
    return;
  }
  if (activationError.value || networkPending.value) {
    if (await activation.prepareNetworkForActivation()) {
      emit("refresh");
    }
    return;
  }
  if (!app.value.running || !canActivate.value) return;
  if (!(await activation.prepareNetworkForActivation())) return;
  if (await activation.activate(app.value.executablePath)) {
    emit("refresh");
  }
}
</script>

<template>
  <div class="page locale-page">
    <header class="page-header">
      <span class="eyebrow">ChatGPT / Codex</span>
      <h1>中文设置</h1>
      <p>自动选择稳定节点，启动应用并验证中文界面真实生效。</p>
    </header>

    <section class="primary-panel setup-panel">
      <div class="setup-app-row">
        <span class="app-symbol brand-openai"><BrandIcon brand="openai" :size="28" /></span>
        <div>
          <strong>ChatGPT</strong>
          <span v-if="app">版本 {{ app.version ?? "未知" }} · {{ app.running ? "正在运行" : "未运行" }}</span>
          <span v-else-if="loading">正在检测本机应用</span>
          <span v-else-if="error">{{ error }}</span>
          <span v-else>未检测到 ChatGPT 或 Codex 桌面应用</span>
        </div>
        <span :class="['status-pill', { success: appInstalled }]">{{ appStatusLabel }}</span>
      </div>

      <div class="setup-checklist">
        <div :class="['check-row', { complete: appReady }]">
          <div>
            <strong>{{ appReady ? "应用已就绪" : appInstalled ? "应用尚未运行" : "尚未安装应用" }}</strong>
            <span>{{ appReady ? `已找到正在运行的 ${app?.displayName ?? "ChatGPT"}` : appInstalled ? "请先打开 ChatGPT 或 Codex，再进行中文设置" : "请先安装 ChatGPT 或 Codex，再进行中文设置" }}</span>
          </div>
          <span :class="['step-state', { pending: !appReady }]">{{ appReady ? "就绪" : appInstalled ? "未运行" : "未安装" }}</span>
        </div>
        <div :class="['check-row', { complete: !networkPending }]">
          <div>
            <strong>路由配置已就绪</strong>
            <span>签名、完整性和有效期将在执行时验证</span>
          </div>
          <span class="step-state">就绪</span>
        </div>
        <div class="check-row complete">
          <div>
            <strong>{{ networkPending ? "系统网络待恢复" : "系统网络正常" }}</strong>
            <span>{{ networkPending ? "请先在恢复与诊断中恢复原网络" : "当前没有待恢复的代理状态" }}</span>
          </div>
          <span :class="['step-state', { pending: networkPending }]">{{ networkPending ? "待恢复" : "正常" }}</span>
        </div>
        <div class="check-row">
          <div>
            <strong>等待中文设置</strong>
            <span>完成后会验证应用进程使用 zh-CN 启动</span>
          </div>
          <span class="step-state pending">待执行</span>
        </div>
      </div>

      <div class="setup-action-area">
        <div class="setup-action-copy">
          <PhTranslate :size="21" />
          <p v-if="!appInstalled">安装完成并打开一次后，返回这里会自动重新检测。</p>
          <p v-else-if="recoveryError">{{ recoveryError }}</p>
          <p v-else-if="activationError">{{ activationError }}</p>
          <p v-else-if="!appReady">请先打开 ChatGPT 或 Codex。</p>
          <p v-else>{{ running ? message : "执行期间可能会短暂重启 ChatGPT。" }}</p>
        </div>
        <button
          class="primary-button large"
          type="button"
          :disabled="primaryDisabled"
          @click="handlePrimaryAction"
        >
          <PhSpinnerGap v-if="running || recoveryRunning" class="spinning" :size="17" />
          <PhDownloadSimple v-else-if="!appInstalled" :size="17" />
          <PhPlay v-else :size="17" weight="fill" />
          {{ primaryLabel }}
        </button>
      </div>
    </section>
  </div>
</template>
