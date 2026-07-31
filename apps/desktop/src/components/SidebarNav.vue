<script setup lang="ts">
import { PhArrowSquareOut, PhHouse, PhPackage, PhTranslate } from "@phosphor-icons/vue";
import brandLogo from "../assets/brand/dada-logo.svg";
import { DADA_LINKS, openExternalLink } from "../services/external-links";
import type { WorkspacePage } from "../types/ui";

defineProps<{ activePage: WorkspacePage }>();
const emit = defineEmits<{ navigate: [page: WorkspacePage] }>();

const navigation: Array<{
  id: WorkspacePage;
  label: string;
  icon: typeof PhHouse;
}> = [
  { id: "home", label: "首页", icon: PhHouse },
  { id: "locale", label: "配置中文", icon: PhTranslate },
  { id: "software", label: "安装软件", icon: PhPackage },
];
</script>

<template>
  <aside class="sidebar">
    <button
      type="button"
      class="brand-link"
      title="访问哒哒 API 官网"
      aria-label="访问哒哒 API 官网"
      @click="openExternalLink(DADA_LINKS.home)"
    >
      <img :src="brandLogo" alt="" />
      <span>
        <strong>哒哒助手</strong>
        <small>DADA API</small>
      </span>
    </button>

    <nav class="sidebar-nav" aria-label="主要导航">
      <button
        v-for="item in navigation"
        :key="item.id"
        type="button"
        :class="['nav-item', { active: activePage === item.id }]"
        @click="emit('navigate', item.id)"
      >
        <component :is="item.icon" :size="19" weight="regular" />
        <span>{{ item.label }}</span>
      </button>
    </nav>

    <div class="sidebar-footer">
      <button type="button" class="website-link" @click="openExternalLink(DADA_LINKS.home)">
        <span>访问官网</span>
        <PhArrowSquareOut :size="16" />
      </button>
      <p>让好模型，更好用。</p>
    </div>
  </aside>
</template>
