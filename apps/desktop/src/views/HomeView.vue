<script setup lang="ts">
import {
  PhArrowRight,
  PhArrowUpRight,
  PhBookOpenText,
  PhCheckCircle,
  PhCirclesThreePlus,
  PhGlobeSimple,
  PhGift,
  PhPackage,
  PhTranslate,
  PhWallet,
} from "@phosphor-icons/vue";
import { computed } from "vue";
import welcomeMascot from "../assets/brand/mascot/welcome.png";
import BrandIcon from "../components/BrandIcon.vue";
import { DADA_LINKS, openExternalLink } from "../services/external-links";
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
const app = computed(() => props.overview?.apps[0] ?? null);

const appHeading = computed(() => {
  if (props.loading) return "正在检测 ChatGPT";
  if (props.error) return "暂时无法读取应用状态";
  if (!app.value) return "安装 ChatGPT 后开始设置";
  return `${app.value.displayName} 已就绪`;
});

const appDescription = computed(() => {
  if (props.loading) return "正在确认本机安装与运行状态，请稍候。";
  if (props.error) return props.error;
  if (!app.value) return "从官方地址下载安装，打开一次后即可配置中文。";
  if (!app.value.running) return "应用已经安装。打开 ChatGPT 后即可继续中文设置。";
  return "应用正在运行，可以直接开始中文设置。";
});

const actionLabel = computed(() => (app.value ? "配置中文" : "安装 ChatGPT"));

function handlePrimaryAction(): void {
  emit("navigate", app.value ? "locale" : "software");
}

const shortcuts: Array<{
  page: WorkspacePage;
  title: string;
  description: string;
  icon: typeof PhTranslate;
}> = [
  {
    page: "locale",
    title: "配置中文",
    description: "检测应用、选择可用节点并验证中文界面",
    icon: PhTranslate,
  },
  {
    page: "software",
    title: "安装软件",
    description: "从官方来源下载桌面应用和命令行工具",
    icon: PhPackage,
  },
];

const serviceLinks = [
  { title: "模型价格", url: DADA_LINKS.pricing, icon: PhCirclesThreePlus },
  { title: "使用文档", url: DADA_LINKS.docs, icon: PhBookOpenText },
  { title: "新人福利", url: DADA_LINKS.referral, icon: PhGift },
  { title: "控制台", url: DADA_LINKS.console, icon: PhGlobeSimple },
  { title: "账户充值", url: DADA_LINKS.topup, icon: PhWallet },
];
</script>

<template>
  <div class="page home-page">
    <section class="home-hero">
      <div class="home-hero-copy">
        <span class="eyebrow">哒哒助手</span>
        <div class="home-app-title">
          <span class="app-symbol brand-openai"><BrandIcon brand="openai" :size="30" /></span>
          <div>
            <h1>{{ appHeading }}</h1>
            <p>{{ appDescription }}</p>
          </div>
        </div>

        <div class="home-meta">
          <span>
            <i :class="['status-dot', { ready: Boolean(app) && !error }]" />
            {{ loading ? "检测中" : error ? "检测失败" : app ? "已安装" : "未安装" }}
          </span>
          <span v-if="app?.version">版本 {{ app.version }}</span>
          <span v-if="app">
            <PhCheckCircle :size="15" weight="fill" />
            {{ app.running ? "正在运行" : "等待打开" }}
          </span>
        </div>

        <div class="hero-actions">
          <button class="primary-button large" type="button" @click="handlePrimaryAction">
            {{ actionLabel }}
            <PhArrowRight :size="18" weight="bold" />
          </button>
          <button v-if="error" class="secondary-button" type="button" @click="emit('refresh')">
            重新检测
          </button>
        </div>
      </div>
      <img class="home-mascot" :src="welcomeMascot" alt="Little D 向你挥手" />
    </section>

    <section class="home-section">
      <div class="section-heading">
        <h2>快捷操作</h2>
        <span>完成常用任务</span>
      </div>
      <div class="command-list">
        <button
          v-for="shortcut in shortcuts"
          :key="shortcut.page"
          type="button"
          class="command-row"
          @click="emit('navigate', shortcut.page)"
        >
          <span class="shortcut-icon"><component :is="shortcut.icon" :size="20" /></span>
          <span class="shortcut-copy">
            <strong>{{ shortcut.title }}</strong>
            <small>{{ shortcut.description }}</small>
          </span>
          <PhArrowRight class="shortcut-arrow" :size="17" />
        </button>
      </div>
    </section>

    <section class="home-section service-section">
      <div class="section-heading">
        <h2>哒哒 API</h2>
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
  </div>
</template>
