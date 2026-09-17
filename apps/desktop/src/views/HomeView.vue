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
import brandLogo from "../assets/brand/dada-logo.svg";
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
const trackedApps = computed(() => [
  {
    key: "chatGpt",
    name: "ChatGPT",
    app: props.overview?.apps.find((item) => item.product === "chatGpt") ?? null,
  },
  {
    key: "codex",
    name: "Codex",
    app: props.overview?.apps.find((item) => item.product === "codex") ?? null,
  },
]);
const configurationLabel = computed(() => (networkPending.value ? "恢复原网络" : "配置中文"));
const configurationSummary = computed(() => {
  if (networkPending.value) return "中文已经生效 · 等待手动恢复原网络";
  if (result.value) return "中文已经生效 · 可重新设置";
  return "五步本地流程 · 状态可验证、网络可恢复";
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

function appStatusLabel(app: (typeof trackedApps.value)[number]): string {
  if (props.loading) return "检测中";
  if (props.error) return "检测失败";
  if (!app.app) return "未安装";
  return app.app.running ? "运行中" : "已安装";
}

function appStatusDetail(app: (typeof trackedApps.value)[number]): string {
  if (app.app?.version) return `版本 ${app.app.version}`;
  if (app.app?.running) return "进程正在运行";
  return app.app ? "已找到本机安装" : "等待本机检测";
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
        <img :src="brandLogo" alt="" />
        <span><strong>哒哒助手</strong><small>DADA API</small></span>
      </button>
      <div class="brand-rail-meta" aria-label="当前工作区">
        <span>LOCAL CONFIGURATION</span>
        <strong>本机工作台 / SIDE A</strong>
      </div>
    </header>

    <main class="page home-page">
      <section class="record-sleeve" aria-labelledby="record-sleeve-title">
        <div class="record-sleeve-art" aria-hidden="true">
          <div class="record-disc">
            <span>SIDE A</span>
            <i />
          </div>
          <p>LOCAL CONFIGURATION</p>
          <strong>01 / SIDE A</strong>
        </div>

        <div class="record-sleeve-main">
          <div class="record-sleeve-topline">
            <span>哒哒助手 / 本地配置唱片</span>
            <span>ORIGINAL SIGNAL · #12CFC3</span>
          </div>

          <div class="record-intro">
            <div>
              <h1 id="record-sleeve-title">让好模型，更好用。</h1>
              <p>为 ChatGPT 与 Codex 完成本地中文配置，所有步骤都能看见、验证和恢复。</p>
            </div>
            <span class="record-edition">DADA API<br />UTILITY EDITION</span>
          </div>

          <div class="record-status-grid" aria-label="ChatGPT 与 Codex 状态">
            <article v-for="app in trackedApps" :key="app.key" class="record-status-strip">
              <span class="record-track-number">{{ app.key === "chatGpt" ? "01" : "02" }}</span>
              <div>
                <strong>{{ app.name }}</strong>
                <span>{{ appStatusDetail(app) }}</span>
              </div>
              <span :class="['record-status', { active: app.app?.running }]">
                <i />
                {{ appStatusLabel(app) }}
              </span>
            </article>
          </div>

          <div class="record-action-band">
            <div>
              <h2>配置中文</h2>
              <span class="record-label">NOW PLAYING / PRIMARY TRACK</span>
              <p>{{ configurationSummary }}</p>
            </div>
            <button type="button" class="record-play-button" @click="openLocale()">
              {{ configurationLabel }}
              <PhArrowUpRight :size="18" />
            </button>
          </div>

          <ol class="record-step-strip" aria-label="中文配置五步流程">
            <li v-for="(step, index) in ['打开应用', '路由确认', '旧记录', '中文验证', '恢复原网络']" :key="step">
              <span>{{ String(index + 1).padStart(2, "0") }}</span>
              <strong>{{ step }}</strong>
            </li>
          </ol>
        </div>

        <aside class="record-track-list" aria-label="哒哒助手工作区">
          <span class="record-label">TRACK LIST / SIDE A</span>
          <div class="record-track-row active">
            <span>01</span>
            <strong>配置中文</strong>
            <small>PRIMARY</small>
          </div>
          <div class="record-track-row">
            <span>02</span>
            <strong>桌面应用</strong>
            <small>TOOLS</small>
          </div>
          <div class="record-track-row">
            <span>03</span>
            <strong>命令行工具</strong>
            <small>CLI</small>
          </div>
          <div class="record-track-row">
            <span>04</span>
            <strong>恢复原网络</strong>
            <small>SAFE EXIT</small>
          </div>
          <p class="record-track-note">青绿色只在需要你注意的状态和动作上亮起。</p>
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
                <span>ChatGPT / Codex</span>
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
