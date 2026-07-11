<script setup lang="ts">
import {
  PhArrowRight,
  PhCheckCircle,
  PhGlobeHemisphereWest,
  PhPackage,
  PhTranslate,
} from "@phosphor-icons/vue";
import BrandIcon from "../components/BrandIcon.vue";
import type { WorkspacePage } from "../types/ui";

const emit = defineEmits<{ navigate: [page: WorkspacePage] }>();

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
            <h2>ChatGPT 已就绪</h2>
          </div>
        </div>
        <button class="primary-button large" type="button" @click="emit('navigate', 'locale')">
          配置中文
          <PhArrowRight :size="18" weight="bold" />
        </button>
      </div>

      <div class="hero-status-grid">
        <div>
          <span>版本</span>
          <strong>26.707.31428</strong>
        </div>
        <div>
          <span>运行状态</span>
          <strong class="success-text"><PhCheckCircle :size="16" weight="fill" />正常</strong>
        </div>
      </div>
    </section>
  </div>
</template>
