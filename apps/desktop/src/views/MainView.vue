<script setup lang="ts">
import { AppWindow, Check, CircleAlert, Languages, LoaderCircle, RotateCcw } from "lucide-vue-next";
import { storeToRefs } from "pinia";
import { computed, nextTick, ref, watch } from "vue";
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
  progressStep,
  message,
  error: activationError,
  result,
  resultMessage,
  networkStatusState,
  networkStatusError,
  networkPending,
  localProxyActive,
  recoveryRunning,
  recoveryError,
} = storeToRefs(activation);

const app = computed(() => props.overview?.apps[0] ?? null);
const chineseEnabled = computed(() => props.overview?.locale.chineseEnabled ?? false);
const confirmationOpen = ref(false);
const confirmationDialog = ref<{ focus: () => void } | null>(null);

const activationChecking = computed(
  () =>
    availabilityState.value === "unknown" ||
    availabilityState.value === "loading" ||
    networkStatusState.value === "unknown" ||
    networkStatusState.value === "loading",
);

const chineseStatus = computed(() => {
  if (result.value) {
    return "已生效";
  }
  return chineseEnabled.value ? "已配置" : "未配置";
});

const networkLabel = computed(() => {
  switch (networkStatusState.value) {
    case "unknown":
    case "loading":
      return "检测中";
    case "error":
      return "检测失败";
    case "ready":
      if (!networkPending.value) {
        return "已恢复";
      }
      return localProxyActive.value ? "代理使用中" : "待恢复";
    default:
      return "检测中";
  }
});

const appStateLabel = computed(() => {
  if (props.loading) {
    return "检测中";
  }
  if (props.error) {
    return "检测失败";
  }
  if (!app.value) {
    return "未找到";
  }
  return app.value.running ? "运行中" : "未运行";
});

const activationLabel = computed(() => {
  if (running.value) {
    return "正在启用";
  }
  if (activationChecking.value) {
    return "检测中";
  }
  if (activationError.value) {
    return "重试";
  }
  return chineseEnabled.value ? "重新启用" : "启用中文";
});

const progressLabel = computed(() => {
  return (
    ["检测节点", "连接代理", "写入配置", "验证结果"][Math.max(0, progressStep.value - 1)] ??
    "检测节点"
  );
});

const showActivationAction = computed(
  () =>
    availabilityState.value !== "error" &&
    availabilityState.value !== "unavailable" &&
    networkStatusState.value !== "error",
);

watch(confirmationOpen, async (open) => {
  if (!open) {
    return;
  }
  await nextTick();
  confirmationDialog.value?.focus();
});

function requestActivation(): void {
  if (app.value && canActivate.value) {
    confirmationOpen.value = true;
  }
}

async function confirmActivation(): Promise<void> {
  const selectedApp = app.value;
  confirmationOpen.value = false;
  if (!selectedApp) {
    return;
  }
  const succeeded = await activation.activate(selectedApp.executablePath);
  if (succeeded) {
    emit("refresh");
  }
}

async function restore(): Promise<void> {
  await activation.restoreOriginalNetwork();
}

async function retryAvailability(): Promise<void> {
  await activation.refreshAvailability();
}

async function retryNetworkStatus(): Promise<void> {
  await activation.refreshNetworkStatus();
}
</script>

<template>
  <section class="single-screen" aria-label="中文设置">
    <article class="control-card surface-card">
      <header class="control-header">
        <span class="app-icon"><AppWindow :size="24" /></span>
        <div class="app-copy">
          <strong>{{ app?.displayName ?? "ChatGPT" }}</strong>
          <span v-if="app?.version">版本 {{ app.version }}</span>
        </div>
        <span
          :class="[
            'status-badge',
            {
              success: app?.running && !loading && !error,
              error: Boolean(error),
              neutral: loading || !app,
            },
          ]"
        >
          <LoaderCircle v-if="loading" class="spinning" :size="14" />
          <CircleAlert v-else-if="error" :size="14" />
          <i v-else />
          {{ appStateLabel }}
        </span>
      </header>

      <div class="control-body">
        <div v-if="loading" class="center-state">
          <LoaderCircle class="spinning" :size="20" />
          <span>检测中</span>
        </div>

        <div v-else-if="error" class="inline-result error" role="alert">
          <CircleAlert :size="18" />
          <div>
            <strong>检测失败</strong>
            <span>{{ error }}</span>
          </div>
        </div>

        <div v-else-if="!app" class="empty-state">
          <strong>未找到 ChatGPT</strong>
          <span>安装并打开一次后重新检测。</span>
        </div>

        <template v-else>
          <dl class="status-rows">
            <div>
              <dt>中文</dt>
              <dd :class="{ success: chineseEnabled || result }">{{ chineseStatus }}</dd>
            </div>
            <div>
              <dt>网络</dt>
              <dd :class="{ warning: networkPending, success: networkPending === false }">
                {{ networkLabel }}
              </dd>
            </div>
          </dl>

          <div v-if="running" class="activation-progress" aria-live="polite">
            <div class="progress-heading">
              <strong>{{ progressLabel }}</strong>
              <span>{{ progressStep }}/4</span>
            </div>
            <div class="progress-track" aria-hidden="true">
              <span :style="{ width: `${progressStep * 25}%` }" />
            </div>
            <p><LoaderCircle class="spinning" :size="16" />{{ message }}</p>
          </div>

          <div v-else-if="activationError" class="inline-result error" role="alert">
            <CircleAlert :size="18" />
            <div>
              <strong>应用失败</strong>
              <span>{{ activationError }}</span>
            </div>
          </div>

          <div v-else-if="result" class="inline-result success" role="status">
            <Check :size="18" />
            <span>{{ resultMessage }}</span>
          </div>

          <div v-else-if="networkStatusState === 'error'" class="inline-result error" role="alert">
            <CircleAlert :size="18" />
            <div>
              <strong>网络检测失败</strong>
              <span>{{ networkStatusError }}</span>
            </div>
            <button class="text-button" type="button" @click="retryNetworkStatus">重试</button>
          </div>

          <div v-else-if="availabilityState === 'error'" class="inline-result error" role="alert">
            <CircleAlert :size="18" />
            <div>
              <strong>检测失败</strong>
              <span>{{ availabilityError }}</span>
            </div>
            <button class="text-button" type="button" @click="retryAvailability">重试</button>
          </div>

          <div
            v-else-if="availabilityState === 'unavailable'"
            class="inline-result error"
            role="alert"
          >
            <CircleAlert :size="18" />
            <strong>中文设置不可用</strong>
          </div>

          <div v-if="recoveryError" class="inline-result error" role="alert">
            <CircleAlert :size="18" />
            <div>
              <strong>恢复失败</strong>
              <span>{{ recoveryError }}</span>
            </div>
          </div>
        </template>
      </div>

      <footer v-if="!loading" class="control-actions">
        <button
          v-if="error || !app"
          class="secondary-button"
          type="button"
          @click="emit('refresh')"
        >
          <RotateCcw :size="17" />
          重新检测
        </button>

        <button
          v-else-if="networkStatusState === 'ready' && networkPending"
          class="primary-button"
          type="button"
          :disabled="recoveryRunning || running"
          @click="restore"
        >
          <LoaderCircle v-if="recoveryRunning" class="spinning" :size="17" />
          <RotateCcw v-else :size="17" />
          {{ recoveryRunning ? "正在恢复" : recoveryError ? "重试恢复" : "恢复原网络" }}
        </button>

        <button
          v-else-if="showActivationAction"
          class="primary-button"
          type="button"
          :disabled="!canActivate"
          @click="requestActivation"
        >
          <LoaderCircle v-if="running || activationChecking" class="spinning" :size="17" />
          <Languages v-else :size="17" />
          {{ activationLabel }}
        </button>
      </footer>
    </article>
  </section>

  <Teleport to="body">
    <div
      v-if="confirmationOpen"
      ref="confirmationDialog"
      class="dialog-backdrop"
      role="dialog"
      aria-modal="true"
      aria-labelledby="activation-dialog-title"
      tabindex="-1"
      @click.self="confirmationOpen = false"
      @keydown.esc="confirmationOpen = false"
    >
      <div class="confirm-dialog surface-card">
        <span class="dialog-icon"><Languages :size="22" /></span>
        <h2 id="activation-dialog-title">
          {{ chineseEnabled ? "重新启用" : "启用中文" }}
        </h2>
        <p>ChatGPT 将重新启动，系统代理会保持到你手动恢复。</p>
        <footer>
          <button class="ghost-button" type="button" @click="confirmationOpen = false">取消</button>
          <button class="primary-button" type="button" @click="confirmActivation">继续</button>
        </footer>
      </div>
    </div>
  </Teleport>
</template>
