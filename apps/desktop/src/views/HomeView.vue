<script setup lang="ts">
import {
  PhArrowRight,
  PhArrowUpRight,
  PhBookOpenText,
  PhCheckCircle,
  PhGlobeHemisphereWest,
  PhGlobeSimple,
  PhHeadset,
  PhImageSquare,
  PhPackage,
  PhTranslate,
  PhWallet,
} from "@phosphor-icons/vue";
import BrandIcon from "../components/BrandIcon.vue";
import { computed } from "vue";
import { openExternalLink, WOCAO_LINKS } from "../services/external-links";
import type { LocaleOverview } from "../types/locale";
import type { WorkspacePage } from "../types/ui";

const props = defineProps<{
  overview: LocaleOverview | null;
  loading: boolean;
  error: string;
}>();
const emit = defineEmits<{ navigate: [page: WorkspacePage] }>();
const app = computed(() => props.overview?.apps[0] ?? null);

const appHeading = computed(() => {
  if (props.loading) {
    return "正在检测应用";
  }
  if (props.error) {
    return "应用检测失败";
  }
  if (!app.value) {
    return "ChatGPT 尚未安装";
  }
  return `${app.value.displayName} 已就绪`;
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
    description: "为 ChatGPT 或 Codex 启用中文界面",
    icon: PhTranslate,
  },
  {
    page: "software",
    title: "安装工具",
    description: "桌面应用与命令行工具集中安装",
    icon: PhPackage,
  },
  {
    page: "repair",
    title: "恢复与诊断",
    description: "恢复网络、撤销配置或导出诊断",
    icon: PhGlobeHemisphereWest,
  },
];

const serviceLinks: Array<{
  title: string;
  description: string;
  url: string;
  icon: typeof PhWallet;
  tone: string;
}> = [
  {
    title: "账户充值",
    description: "余额与账单",
    url: WOCAO_LINKS.wallet,
    icon: PhWallet,
    tone: "wallet",
  },
  {
    title: "官方网站",
    description: "产品与服务",
    url: WOCAO_LINKS.home,
    icon: PhGlobeSimple,
    tone: "website",
  },
  {
    title: "快捷生图",
    description: "立即开始创作",
    url: WOCAO_LINKS.imageGenerator,
    icon: PhImageSquare,
    tone: "image",
  },
  {
    title: "使用文档",
    description: "指南与说明",
    url: WOCAO_LINKS.docs,
    icon: PhBookOpenText,
    tone: "docs",
  },
  {
    title: "技术支持",
    description: "在线咨询",
    url: WOCAO_LINKS.support,
    icon: PhHeadset,
    tone: "support",
  },
];
</script>

<template>
  <div class="page home-page">
    <section class="section-block home-actions">
      <div class="section-heading">
        <h2>快捷操作</h2>
      </div>

      <div class="command-links">
        <button
          v-for="shortcut in shortcuts"
          :key="shortcut.page"
          type="button"
          class="command-link"
          @click="emit('navigate', shortcut.page)"
        >
          <span class="shortcut-icon">
            <component :is="shortcut.icon" :size="22" weight="regular" />
          </span>
          <span class="shortcut-copy">
            <strong>{{ shortcut.title }}</strong>
            <small>{{ shortcut.description }}</small>
          </span>
          <PhArrowRight class="shortcut-arrow" :size="18" />
        </button>
      </div>
    </section>

    <section class="hero-command">
      <div class="hero-command-top">
        <div class="app-identity">
          <span class="app-symbol brand-openai"><BrandIcon brand="openai" :size="32" /></span>
          <div>
            <span class="eyebrow">当前应用</span>
            <h2>{{ appHeading }}</h2>
          </div>
        </div>
        <button class="primary-button large" type="button" @click="handlePrimaryAction">
          {{ actionLabel }}
          <PhArrowRight :size="18" weight="bold" />
        </button>
      </div>

      <div class="hero-status-grid">
        <div>
          <span>版本</span>
          <strong>{{ app?.version ?? (loading ? "检测中" : "—") }}</strong>
        </div>
        <div>
          <span>运行状态</span>
          <strong :class="{ 'success-text': Boolean(app) }">
            <PhCheckCircle v-if="app" :size="16" weight="fill" />
            {{
              loading
                ? "检测中"
                : error
                  ? "检测失败"
                  : app
                    ? app.running
                      ? "正在运行"
                      : "已安装"
                    : "未安装"
            }}
          </strong>
        </div>
      </div>
    </section>

    <section class="section-block service-section">
      <div class="section-heading service-heading">
        <h2>快捷服务</h2>
        <span>wocao.ai</span>
      </div>

      <div class="service-links">
        <button
          v-for="service in serviceLinks"
          :key="service.url"
          type="button"
          :class="['service-link', `service-${service.tone}`]"
          @click="openExternalLink(service.url)"
        >
          <span class="service-icon">
            <component :is="service.icon" :size="20" weight="regular" />
          </span>
          <span class="service-copy">
            <strong>{{ service.title }}</strong>
            <small>{{ service.description }}</small>
          </span>
          <PhArrowUpRight class="service-arrow" :size="15" />
        </button>
      </div>
    </section>
  </div>
</template>
