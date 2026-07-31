<script setup lang="ts">
import { PhArrowRight, PhCheck, PhDownloadSimple, PhPlay, PhSpinnerGap } from "@phosphor-icons/vue";
import { storeToRefs } from "pinia";
import { computed } from "vue";
import BrandIcon from "../components/BrandIcon.vue";
import { useActivationStore } from "../stores/activation";
import type { LocaleOverview } from "../types/locale";

const props = defineProps<{
  overview: LocaleOverview | null;
  loading: boolean;
  error: string;
}>();

const emit = defineEmits<{
  refresh: [];
}>();

const activation = useActivationStore();
const {
  availabilityState,
  availabilityError,
  canActivate,
  running,
  message,
  error: activationError,
  result,
  networkStatusState,
  networkStatusError,
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
  if (!appInstalled.value) return "请先安装";
  if (running.value) return "正在设置";
  if (recoveryRunning.value) return "正在恢复";
  if (result.value) {
    return networkPending.value ? "恢复原网络" : "重新设置";
  }
  if (activationError.value || networkPending.value) return "恢复网络";
  if (!appReady.value) return "等待应用运行";
  return props.overview?.locale.chineseEnabled ? "重新设置" : "开始设置";
});

const primaryDisabled = computed(() => {
  if (!appInstalled.value) return true;
  if (running.value || recoveryRunning.value) return true;
  if (networkStatusState.value !== "ready") return true;
  if (result.value) return networkPending.value === false && !canActivate.value;
  if (activationError.value || networkPending.value) return false;
  if (!appReady.value) return true;
  return !canActivate.value;
});

const routeConfirmed = computed(() => availabilityState.value === "available");
const staleRecoveryHandled = computed(
  () => networkPending.value === false || Boolean(result.value),
);
const networkRestored = computed(() => Boolean(result.value) && networkPending.value === false);

const actionMessage = computed(() => {
  if (!appInstalled.value) return "安装并打开 ChatGPT 后，哒哒助手会自动重新检测。";
  if (recoveryError.value) return recoveryError.value;
  if (activationError.value) return activationError.value;
  if (networkStatusState.value === "error") return networkStatusError.value;
  if (availabilityState.value === "error") return availabilityError.value;
  if (availabilityState.value === "unavailable") return "当前构建未配置中文路由服务。";
  if (!appReady.value) return "请先打开 ChatGPT 或 Codex，再开始设置。";
  if (result.value) return message.value || "中文已经生效。";
  return running.value ? message.value : "执行期间 ChatGPT 可能会短暂重启。";
});

async function handlePrimaryAction(): Promise<void> {
  if (!app.value) {
    return;
  }
  if (running.value || recoveryRunning.value) return;
  if (result.value) {
    if (networkPending.value && (await activation.restoreOriginalNetwork())) emit("refresh");
    return;
  }
  if (activationError.value || networkPending.value) {
    if (await activation.prepareNetworkForActivation()) emit("refresh");
    return;
  }
  if (!app.value.running || !canActivate.value) return;
  if (!(await activation.prepareNetworkForActivation())) return;
  if (await activation.activate(app.value.executablePath)) emit("refresh");
}
</script>

<template>
  <section class="locale-drawer-content">
    <div class="setup-main">
      <div class="setup-app-row">
        <span class="app-symbol brand-openai"><BrandIcon brand="openai" :size="28" /></span>
        <div>
          <strong>{{ app?.displayName ?? "ChatGPT" }}</strong>
          <span v-if="app">版本 {{ app.version ?? "未知" }}</span>
          <span v-else-if="loading">正在检测本机应用</span>
          <span v-else-if="error">{{ error }}</span>
          <span v-else>未检测到桌面应用</span>
        </div>
        <span :class="['status-pill', { success: appReady }]">{{ appStatusLabel }}</span>
      </div>

      <ol class="setup-steps">
        <li :class="{ complete: appReady }">
          <span class="step-index">
            <PhCheck v-if="appReady" :size="14" weight="bold" />
            <b v-else>1</b>
          </span>
          <div>
            <strong>{{ appReady ? "应用已经就绪" : "打开桌面应用" }}</strong>
            <span>{{ appReady ? "已找到正在运行的应用进程" : "需要先启动 ChatGPT 或 Codex" }}</span>
          </div>
        </li>
        <li :class="{ complete: routeConfirmed }">
          <span class="step-index">
            <PhCheck v-if="routeConfirmed" :size="14" weight="bold" />
            <b v-else>2</b>
          </span>
          <div>
            <strong>路由确认</strong>
            <span>{{
              routeConfirmed ? "配置可用，执行时会再次验签" : "正在确认当前构建配置"
            }}</span>
          </div>
        </li>
        <li :class="{ complete: staleRecoveryHandled }">
          <span class="step-index">
            <PhCheck v-if="staleRecoveryHandled" :size="14" weight="bold" />
            <b v-else>3</b>
          </span>
          <div>
            <strong>处理旧恢复记录</strong>
            <span>{{
              staleRecoveryHandled ? "当前没有阻断设置的遗留代理状态" : "检测到上次遗留的代理状态"
            }}</span>
          </div>
        </li>
        <li :class="{ complete: Boolean(result) }">
          <span class="step-index">
            <PhCheck v-if="result" :size="14" weight="bold" />
            <b v-else>4</b>
          </span>
          <div>
            <strong>{{ result ? "中文已经生效" : "中文设置验证" }}</strong>
            <span>{{ result ? "应用已使用 zh-CN 启动" : "设置完成后验证应用进程语言" }}</span>
          </div>
        </li>
        <li :class="{ complete: networkRestored }">
          <span class="step-index">
            <PhCheck v-if="networkRestored" :size="14" weight="bold" />
            <b v-else>5</b>
          </span>
          <div>
            <strong>{{ networkRestored ? "网络已恢复" : "恢复原网络" }}</strong>
            <span>{{
              networkRestored
                ? "已恢复原网络配置并关闭临时代理"
                : result
                  ? "中文验证成功后，请手动恢复原网络"
                  : "完成中文设置后可恢复原网络"
            }}</span>
          </div>
        </li>
      </ol>

      <div :class="['setup-action-bar', { error: Boolean(activationError || recoveryError) }]">
        <p>{{ actionMessage }}</p>
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
          <PhArrowRight v-if="!running && !recoveryRunning" :size="16" />
        </button>
      </div>
    </div>
  </section>
</template>
