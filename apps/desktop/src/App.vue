<script setup lang="ts">
import {
  AppWindow,
  CheckCircle2,
  CircleAlert,
  Languages,
  LoaderCircle,
  RefreshCw,
  RotateCcw,
  ShieldAlert,
} from "lucide-vue-next";
import { storeToRefs } from "pinia";
import { onMounted, ref } from "vue";
import { getLocaleOverview } from "./services/locale";
import { useActivationStore } from "./stores/activation";
import type { CommandError, LocaleOverview } from "./types/locale";

const overview = ref<LocaleOverview | null>(null);
const loading = ref(true);
const loadError = ref("");
const activation = useActivationStore();
const {
  available: activationAvailable,
  running: activationRunning,
  selectedExecutablePath,
  message: activationMessage,
  error: activationError,
  result: activationResult,
  networkStatus,
  recoveryRunning,
  recoveryError,
} = storeToRefs(activation);

function getErrorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as CommandError).message);
  }
  return typeof error === "string" ? error : "检测失败，请稍后重试。";
}

async function refresh(): Promise<void> {
  loading.value = true;
  loadError.value = "";
  try {
    overview.value = await getLocaleOverview();
  } catch (error) {
    loadError.value = getErrorMessage(error);
  } finally {
    loading.value = false;
  }
}

async function activateApp(executablePath: string): Promise<void> {
  const succeeded = await activation.activate(executablePath);
  if (succeeded) {
    await refresh();
  }
}

async function restoreOriginalNetwork(): Promise<void> {
  await activation.restoreOriginalNetwork();
}

function activationButtonLabel(executablePath: string): string {
  if (activationRunning.value && selectedExecutablePath.value === executablePath) {
    return "正在激活…";
  }
  return overview.value?.locale.chineseEnabled ? "重新应用中文" : "一键中文";
}

onMounted(() => {
  void Promise.all([refresh(), activation.initialize()]);
});
</script>

<template>
  <main class="app-shell">
    <header class="topbar">
      <div class="brand">
        <span class="brand-mark">W</span>
        <div>
          <strong>Wocao Hub</strong>
          <span>wocao.ai</span>
        </div>
      </div>
      <button
        class="icon-button"
        type="button"
        :disabled="loading || activationRunning"
        aria-label="重新检测桌面应用"
        title="重新检测"
        @click="refresh"
      >
        <RefreshCw :size="18" :class="{ spinning: loading }" />
      </button>
    </header>

    <section class="content">
      <div class="page-heading">
        <div class="feature-icon">
          <AppWindow :size="24" />
        </div>
        <div>
          <p class="eyebrow">桌面应用检测</p>
          <h1>ChatGPT / Codex</h1>
        </div>
      </div>

      <div v-if="networkStatus.pending" class="network-recovery" role="alert">
        <ShieldAlert :size="22" />
        <div>
          <strong>{{ networkStatus.localProxyActive ? "代理仍在使用" : "检测到遗留代理设置" }}</strong>
          <p v-if="networkStatus.localProxyActive">
            ChatGPT 当前继续通过已选节点联网；使用结束后，请手动恢复原网络。
          </p>
          <p v-else>本地代理已经停止，请立即恢复原网络后再进行其他操作。</p>
          <p v-if="recoveryError" class="recovery-error">{{ recoveryError }}</p>
        </div>
        <button
          class="secondary-button recovery-button"
          type="button"
          :disabled="recoveryRunning || activationRunning"
          @click="restoreOriginalNetwork"
        >
          <LoaderCircle v-if="recoveryRunning" class="spinning" :size="17" />
          <RotateCcw v-else :size="17" />
          {{ recoveryRunning ? "正在恢复…" : "恢复原网络" }}
        </button>
      </div>

      <div v-if="loading" class="state-panel">
        <LoaderCircle class="spinning" :size="22" />
        <span>正在检测桌面应用…</span>
      </div>

      <div v-else-if="loadError" class="state-panel error-panel">
        <CircleAlert :size="22" />
        <div>
          <strong>检测失败</strong>
          <p>{{ loadError }}</p>
        </div>
        <button class="secondary-button" type="button" @click="refresh">重新检测</button>
      </div>

      <div v-else-if="overview && overview.apps.length === 0" class="state-panel">
        <CircleAlert :size="22" />
        <div>
          <strong>未检测到 ChatGPT 或 Codex</strong>
          <p>安装并打开一次应用后，可以重新检测。</p>
        </div>
      </div>

      <section v-else-if="overview" class="app-list" aria-label="已检测的桌面应用">
        <article v-for="app in overview.apps" :key="app.executablePath" class="app-card">
          <div class="app-summary">
            <div class="app-icon">
              <AppWindow :size="25" />
            </div>
            <div class="app-name">
              <div>
                <h2>{{ app.displayName }}</h2>
                <span v-if="app.version">v{{ app.version }}</span>
              </div>
              <p>{{ app.installPath }}</p>
            </div>
            <div class="status-group">
              <span :class="['locale-badge', { enabled: app.running }]">
                {{ app.running ? "正在运行" : "未运行" }}
              </span>
              <span :class="['locale-badge', { enabled: overview.locale.chineseEnabled }]">
                <CheckCircle2 v-if="overview.locale.chineseEnabled" :size="15" />
                {{ overview.locale.chineseEnabled ? "已写入中文配置" : "默认语言配置" }}
              </span>
            </div>
          </div>
          <div v-if="activationAvailable" class="app-actions">
            <button
              class="primary-button"
              type="button"
              :disabled="activationRunning || networkStatus.pending"
              @click="activateApp(app.executablePath)"
            >
              <LoaderCircle
                v-if="activationRunning && selectedExecutablePath === app.executablePath"
                class="spinning"
                :size="17"
              />
              <Languages v-else :size="17" />
              {{ activationButtonLabel(app.executablePath) }}
            </button>
          </div>
          <div
            v-if="activationAvailable && selectedExecutablePath === app.executablePath"
            :class="['notice', 'activation-status', { error: activationError }]"
            role="status"
          >
            <LoaderCircle v-if="activationRunning" class="spinning" :size="20" />
            <CircleAlert v-else-if="activationError" :size="20" />
            <CheckCircle2 v-else-if="activationResult" :size="20" />
            <span v-if="activationRunning">{{ activationMessage }}</span>
            <span v-else-if="activationError">{{ activationError }}</span>
            <span v-else-if="activationResult">中文已生效，代理仍在使用，请按需手动恢复网络。</span>
          </div>
        </article>
      </section>
    </section>
  </main>
</template>

<style>
:root {
  font-family:
    Inter,
    "PingFang SC",
    "Microsoft YaHei",
    system-ui,
    -apple-system,
    BlinkMacSystemFont,
    "Segoe UI",
    sans-serif;
  color: #f2f5f3;
  background: #0b0d0c;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  min-width: 320px;
  min-height: 100vh;
}

button {
  font: inherit;
}

button:focus-visible {
  outline: 2px solid #63d98b;
  outline-offset: 2px;
}

.app-shell {
  min-height: 100vh;
  background: radial-gradient(circle at 50% -20%, rgb(74 170 107 / 10%), transparent 42%), #0b0d0c;
}

.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 74px;
  padding: 0 36px;
  border-bottom: 1px solid #222724;
  background: rgb(11 13 12 / 88%);
}

.brand {
  display: flex;
  gap: 12px;
  align-items: center;
}

.brand-mark {
  display: grid;
  width: 36px;
  height: 36px;
  place-items: center;
  border-radius: 10px;
  color: #071109;
  background: #63d98b;
  font-weight: 800;
}

.brand div {
  display: grid;
  gap: 2px;
}

.brand strong {
  font-size: 15px;
}

.brand div span {
  color: #78827c;
  font-size: 11px;
}

.icon-button {
  display: grid;
  width: 38px;
  height: 38px;
  place-items: center;
  border: 1px solid #29302c;
  border-radius: 10px;
  color: #aeb8b2;
  background: #121513;
  cursor: pointer;
}

.icon-button:hover:not(:disabled) {
  color: #f2f5f3;
  border-color: #3b463f;
}

button:disabled {
  cursor: not-allowed;
  opacity: 0.55;
}

.content {
  width: min(860px, calc(100% - 56px));
  margin: 0 auto;
  padding: 66px 0 72px;
}

.page-heading {
  display: flex;
  gap: 18px;
  align-items: flex-start;
  margin-bottom: 38px;
}

.feature-icon {
  display: grid;
  flex: 0 0 auto;
  width: 48px;
  height: 48px;
  place-items: center;
  border: 1px solid #285d3a;
  border-radius: 14px;
  color: #63d98b;
  background: #102318;
}

.eyebrow {
  margin: 0 0 7px;
  color: #63d98b;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.12em;
}

h1,
h2,
p {
  margin-top: 0;
}

h1 {
  margin-bottom: 10px;
  font-size: 30px;
  letter-spacing: -0.02em;
}

.subtitle {
  margin-bottom: 0;
  color: #8e9992;
  font-size: 14px;
  line-height: 1.65;
}

.state-panel,
.notice {
  display: flex;
  gap: 13px;
  align-items: center;
  padding: 20px;
  border: 1px solid #29302c;
  border-radius: 14px;
  color: #aeb8b2;
  background: #111412;
}

.state-panel > div {
  flex: 1;
}

.state-panel strong {
  display: block;
  margin-bottom: 5px;
  color: #f2f5f3;
}

.state-panel p {
  margin-bottom: 0;
  color: #8e9992;
  font-size: 13px;
}

.error-panel,
.notice.error {
  color: #ff9b91;
  border-color: #61352f;
  background: #211310;
}

.network-recovery {
  display: flex;
  gap: 13px;
  align-items: center;
  margin-bottom: 18px;
  padding: 18px 20px;
  border: 1px solid #72552b;
  border-radius: 14px;
  color: #e6b66f;
  background: #211a10;
}

.network-recovery > div {
  min-width: 0;
  flex: 1;
}

.network-recovery strong {
  display: block;
  margin-bottom: 5px;
  color: #f1c47f;
}

.network-recovery p {
  margin-bottom: 0;
  color: #ba9b6b;
  font-size: 13px;
  line-height: 1.55;
}

.network-recovery .recovery-error {
  margin-top: 5px;
  color: #ff9b91;
}

.recovery-button {
  display: inline-flex;
  flex: 0 0 auto;
  gap: 8px;
  align-items: center;
}

.app-selector {
  display: inline-flex;
  gap: 4px;
  margin-bottom: 14px;
  padding: 4px;
  border: 1px solid #29302c;
  border-radius: 11px;
  background: #111412;
}

.app-choice {
  padding: 7px 13px;
  border: 0;
  border-radius: 7px;
  color: #89938d;
  background: transparent;
  cursor: pointer;
}

.app-choice.active {
  color: #f2f5f3;
  background: #252b27;
}

.app-card {
  overflow: hidden;
  border: 1px solid #29302c;
  border-radius: 16px;
  background: #111412;
  box-shadow: 0 18px 50px rgb(0 0 0 / 20%);
}

.app-list {
  display: grid;
  gap: 12px;
}

.app-summary {
  display: flex;
  gap: 15px;
  align-items: center;
  padding: 24px;
}

.app-icon {
  display: grid;
  flex: 0 0 auto;
  width: 50px;
  height: 50px;
  place-items: center;
  border-radius: 13px;
  color: #d7e1db;
  background: #252b27;
}

.app-name {
  min-width: 0;
  flex: 1;
}

.app-name > div {
  display: flex;
  gap: 10px;
  align-items: baseline;
}

.app-name h2 {
  margin-bottom: 5px;
  font-size: 18px;
}

.app-name span {
  color: #778179;
  font-size: 12px;
}

.app-name p {
  overflow: hidden;
  margin-bottom: 0;
  color: #778179;
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.locale-badge {
  display: inline-flex;
  gap: 7px;
  align-items: center;
  padding: 7px 10px;
  border-radius: 999px;
  color: #d6a56b;
  background: #291f13;
  font-size: 12px;
  font-weight: 600;
}

.locale-badge.enabled {
  color: #79e39c;
  background: #102318;
}

.status-group {
  display: grid;
  gap: 7px;
  justify-items: end;
}

.app-actions {
  display: flex;
  justify-content: flex-end;
  padding: 18px 24px;
  border-top: 1px solid #252b27;
}

.activation-status {
  margin: 0 24px 24px;
}

.details {
  display: grid;
  grid-template-columns: 1fr 1fr;
  margin: 0;
  border-top: 1px solid #252b27;
}

.details > div {
  display: grid;
  gap: 6px;
  padding: 17px 24px;
  border-right: 1px solid #252b27;
  border-bottom: 1px solid #252b27;
}

.details > div:nth-child(2) {
  border-right: 0;
}

.details .path-detail {
  grid-column: 1 / -1;
  border-right: 0;
  border-bottom: 0;
}

dt {
  color: #707a73;
  font-size: 11px;
}

dd {
  overflow: hidden;
  margin: 0;
  color: #cbd3ce;
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.notice {
  margin-top: 14px;
  color: #79e39c;
  border-color: #285d3a;
  background: #102318;
  font-size: 13px;
}

.actions {
  display: flex;
  gap: 10px;
  margin-top: 22px;
}

.primary-button,
.secondary-button {
  display: inline-flex;
  gap: 8px;
  align-items: center;
  justify-content: center;
  min-height: 42px;
  padding: 0 17px;
  border-radius: 10px;
  font-size: 13px;
  font-weight: 650;
  cursor: pointer;
}

.primary-button {
  border: 1px solid #63d98b;
  color: #071109;
  background: #63d98b;
}

.primary-button:hover:not(:disabled) {
  background: #78e49a;
}

.secondary-button {
  border: 1px solid #323a35;
  color: #c7d0ca;
  background: #161a17;
}

.secondary-button:hover:not(:disabled) {
  border-color: #48534c;
  background: #1c211e;
}

.spinning {
  animation: spin 0.9s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 700px) {
  .topbar {
    padding: 0 22px;
  }

  .content {
    width: calc(100% - 36px);
    padding-top: 42px;
  }

  .app-summary {
    align-items: flex-start;
    flex-wrap: wrap;
  }

  .locale-badge {
    margin-left: 65px;
  }

  .status-group {
    width: 100%;
    justify-items: start;
    margin-left: 65px;
  }

  .details {
    grid-template-columns: 1fr;
  }

  .details > div,
  .details > div:nth-child(2) {
    border-right: 0;
  }

  .details .path-detail {
    grid-column: auto;
  }
}
</style>
