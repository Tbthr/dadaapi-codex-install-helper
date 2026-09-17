<script setup lang="ts">
import {
  PhArrowUpRight,
  PhBookOpenText,
  PhCirclesThreePlus,
  PhGlobeSimple,
  PhUsersThree,
  PhWallet,
  PhX,
} from "@phosphor-icons/vue";
import { storeToRefs } from "pinia";
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import appIcon from "../assets/brand/dada-app-icon.svg";
import { DADA_LINKS, openExternalLink } from "../services/external-links";
import { useActivationStore } from "../stores/activation";
import type { LocaleOverview } from "../types/locale";
import LocaleSetupView from "./LocaleSetupView.vue";
import SoftwareView from "./SoftwareView.vue";

const props = defineProps<{
  overview: LocaleOverview | null;
  loading: boolean;
  error: string;
}>();

const emit = defineEmits<{ refresh: [] }>();
const activation = useActivationStore();
const { networkPending, recoveryRunning, result, running } = storeToRefs(activation);
const drawerOpen = ref(false);
const drawer = ref<globalThis.HTMLElement | null>(null);
const drawerLocked = computed(() => running.value || recoveryRunning.value);
const openAiApp = computed(
  () =>
    props.overview?.apps.find((item) => item.product === "chatGpt" || item.product === "codex") ??
    null,
);
const discoveredAppCount = computed(() => (openAiApp.value ? 1 : 0));
const workspaceStatus = computed(() => {
  if (props.loading) return "正在检测目标应用";
  if (props.error) return "检测失败，请重试";
  if (discoveredAppCount.value === 0) return "尚未找到目标应用";
  return `${discoveredAppCount.value} 个目标应用已找到`;
});
const localeStatus = computed(() => {
  if (networkPending.value) return "等待恢复原网络";
  if (result.value || props.overview?.locale.chineseEnabled) return "中文已生效";
  return "尚未配置";
});
const recoveryStatus = computed(() => {
  if (networkPending.value === true) return "待手动恢复";
  if (networkPending.value === false) return "网络正常";
  return "等待检测";
});
const configurationLabel = computed(() => (networkPending.value ? "恢复原网络" : "配置中文"));
const configurationSummary = computed(() => {
  if (networkPending.value) return "中文已经生效 · 等待手动恢复原网络";
  if (result.value) return "中文已经生效 · 可重新设置";
  return "本地流程 · 状态可验证、网络可恢复";
});
const appStatusLabel = computed(() => {
  if (props.loading) return "检测中";
  if (props.error) return "检测失败";
  if (!openAiApp.value) return "未安装";
  return openAiApp.value.running ? "运行中" : "已安装";
});
const appStatusDetail = computed(() => {
  const app = openAiApp.value;
  if (app?.version) {
    return app.product === "codex"
      ? `兼容旧版 Codex · 版本 ${app.version}`
      : `ChatGPT 与 Codex 已合并 · 版本 ${app.version}`;
  }
  if (app?.running) {
    return app.product === "codex" ? "旧版 Codex 正在运行" : "ChatGPT 与 Codex 正在运行";
  }
  if (app) {
    return app.product === "codex" ? "已找到旧版 Codex" : "已找到 ChatGPT 桌面应用";
  }
  return "等待本机检测";
});
let returnFocus: globalThis.HTMLElement | null = null;

const serviceLinks = [
  { title: "模型价格", url: DADA_LINKS.pricing, icon: PhCirclesThreePlus },
  { title: "控制台", url: DADA_LINKS.console, icon: PhGlobeSimple },
  { title: "账户充值", url: DADA_LINKS.topup, icon: PhWallet },
  { title: "新手指南", url: DADA_LINKS.guide, icon: PhBookOpenText },
  { title: "加入 QQ 群", url: DADA_LINKS.qqGroup, icon: PhUsersThree },
];

function openLocale(trigger?: globalThis.HTMLElement): void {
  returnFocus =
    trigger ??
    (globalThis.document.activeElement instanceof globalThis.HTMLElement
      ? globalThis.document.activeElement
      : null);
  drawerOpen.value = true;
}

function closeLocale(): void {
  if (drawerLocked.value) return;
  drawerOpen.value = false;
}

function restoreDrawerFocus(): void {
  returnFocus?.focus();
  returnFocus = null;
}

function handleDrawerKeydown(event: globalThis.KeyboardEvent): void {
  if (!drawerOpen.value) return;
  if (event.key === "Escape") {
    event.preventDefault();
    closeLocale();
    return;
  }
  if (event.key !== "Tab" || !drawer.value) return;
  const focusable = [
    ...drawer.value.querySelectorAll<globalThis.HTMLElement>(
      'button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])',
    ),
  ].filter((element) => !element.hidden);
  if (focusable.length === 0) {
    event.preventDefault();
    drawer.value.focus();
    return;
  }
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && globalThis.document.activeElement === first) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && globalThis.document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

watch(drawerOpen, async (open) => {
  if (open) {
    await nextTick();
    drawer.value?.querySelector<globalThis.HTMLElement>("[data-drawer-close]")?.focus();
  }
});

onMounted(() => globalThis.document.addEventListener("keydown", handleDrawerKeydown));
onUnmounted(() => globalThis.document.removeEventListener("keydown", handleDrawerKeydown));
</script>

<template>
  <div class="app-shell">
    <header class="brand-bar">
      <button
        type="button"
        class="brand-lockup"
        title="访问哒哒 API 官网"
        aria-label="访问哒哒 API 官网"
        @click="openExternalLink(DADA_LINKS.home)"
      >
        <img :src="appIcon" alt="" />
        <span><strong>哒哒助手</strong><small>DADA API</small></span>
      </button>
    </header>

    <main class="page home-page">
      <section class="record-sleeve" aria-labelledby="record-sleeve-title">
        <div class="record-workspace-panel">
          <div class="workspace-panel-heading">
            <span class="record-label">LOCAL WORKSPACE</span>
            <span
              :class="[
                'workspace-status-mark',
                { active: !props.loading && !props.error, error: Boolean(props.error) },
              ]"
            >
              <i />
              {{ props.loading ? "检测中" : props.error ? "需重试" : "已连接" }}
            </span>
          </div>
          <div class="workspace-panel-identity">
            <div class="workspace-panel-copy">
              <span class="workspace-panel-label">OPENAI DESKTOP</span>
              <strong>ChatGPT</strong>
            </div>
          </div>
          <div class="workspace-panel-task">
            <span>当前流程</span>
            <strong>本地设置</strong>
            <p>可验证 · 可恢复</p>
          </div>
          <dl class="workspace-panel-facts">
            <div>
              <dt>应用数量</dt>
              <dd>{{ props.loading ? "—" : `${discoveredAppCount}/1` }}</dd>
            </div>
            <div>
              <dt>网络状态</dt>
              <dd>{{ recoveryStatus }}</dd>
            </div>
          </dl>
        </div>

        <div class="record-sleeve-main">
          <div class="record-sleeve-topline">
            <span>哒哒助手 / 本地配置工作台</span>
            <span>LOCAL-FIRST · VERIFIED &amp; RECOVERABLE</span>
          </div>

          <div class="record-intro">
            <div>
              <h1 id="record-sleeve-title">让好模型，更好用。</h1>
              <p>为 ChatGPT 完成本地中文配置，兼容旧版 Codex。</p>
            </div>
            <span class="record-edition">DADA API<br />UTILITY EDITION</span>
          </div>

          <div class="record-status-grid" aria-label="ChatGPT 桌面应用状态">
            <article class="record-status-strip">
              <span class="record-track-number">OPENAI</span>
              <div>
                <strong>ChatGPT</strong>
                <span>{{ appStatusDetail }}</span>
              </div>
              <span :class="['record-status', { active: openAiApp?.running }]">
                <i />
                {{ appStatusLabel }}
              </span>
            </article>
          </div>

          <div class="record-action-band">
            <div>
              <h2>配置中文</h2>
              <span class="record-label">LOCAL SETUP / VERIFIED FLOW</span>
              <p>{{ configurationSummary }}</p>
            </div>
            <button type="button" class="record-play-button" @click="openLocale()">
              {{ configurationLabel }}
              <PhArrowUpRight :size="18" />
            </button>
          </div>
        </div>

        <aside class="record-status-rail" aria-label="当前配置状态">
          <div class="status-rail-heading">
            <span class="record-label">CONFIGURATION STATUS</span>
            <span>实时</span>
          </div>
          <div class="status-rail-list">
            <div class="status-rail-row">
              <span class="status-rail-marker" :class="{ active: discoveredAppCount > 0 }" />
              <div>
                <strong>应用检测</strong>
                <small>{{ workspaceStatus }}</small>
              </div>
            </div>
            <div class="status-rail-row">
              <span
                class="status-rail-marker"
                :class="{ active: Boolean(result) || overview?.locale.chineseEnabled }"
              />
              <div>
                <strong>中文状态</strong>
                <small>{{ localeStatus }}</small>
              </div>
            </div>
            <div class="status-rail-row">
              <span class="status-rail-marker" :class="{ active: networkPending === false }" />
              <div>
                <strong>网络恢复</strong>
                <small>{{ recoveryStatus }}</small>
              </div>
            </div>
          </div>
          <p class="record-status-note">状态会随配置进度更新。</p>
        </aside>
      </section>

      <section class="home-section service-section" aria-labelledby="dada-links-title">
        <div class="section-heading">
          <h2 id="dada-links-title">哒哒 API</h2>
          <span>模型服务与账户</span>
        </div>
        <div class="service-links">
          <button
            v-for="service in serviceLinks"
            :key="service.url"
            type="button"
            class="service-link"
            @click="openExternalLink(service.url)"
          >
            <component :is="service.icon" :size="18" />
            <span>{{ service.title }}</span>
            <PhArrowUpRight :size="14" />
          </button>
        </div>
      </section>

      <SoftwareView @open-locale="openLocale" />
    </main>

    <Teleport to="body">
      <Transition name="drawer" @after-leave="restoreDrawerFocus">
        <div v-if="drawerOpen" class="drawer-backdrop" @click.self="closeLocale">
          <aside
            ref="drawer"
            class="locale-drawer"
            role="dialog"
            aria-modal="true"
            aria-labelledby="locale-drawer-title"
            tabindex="-1"
          >
            <header class="locale-drawer-header">
              <div>
                <h2 id="locale-drawer-title">配置中文</h2>
                <span>CHATGPT DESKTOP</span>
              </div>
              <button
                data-drawer-close
                type="button"
                class="icon-button"
                title="关闭"
                aria-label="关闭配置中文"
                :disabled="drawerLocked"
                @click="closeLocale"
              >
                <PhX :size="19" />
              </button>
            </header>
            <LocaleSetupView
              :overview="overview"
              :loading="loading"
              :error="error"
              @refresh="emit('refresh')"
            />
          </aside>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>
