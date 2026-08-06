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

defineProps<{
  overview: LocaleOverview | null;
  loading: boolean;
  error: string;
}>();

const emit = defineEmits<{ refresh: [] }>();
const activation = useActivationStore();
const { running, recoveryRunning } = storeToRefs(activation);
const drawerOpen = ref(false);
const drawer = ref<globalThis.HTMLElement | null>(null);
const drawerLocked = computed(() => running.value || recoveryRunning.value);
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
        <img :src="brandLogo" alt="" />
        <span><strong>哒哒助手</strong><small>DADA API</small></span>
      </button>
      <p>让好模型，更好用。</p>
    </header>

    <main class="page home-page">
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
                <span>ChatGPT / Codex</span>
                <h2 id="locale-drawer-title">配置中文</h2>
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
