<script setup lang="ts">
import { PhGearSix, PhHouse, PhLifebuoy, PhPackage, PhTranslate } from "@phosphor-icons/vue";
import brandLogo from "../assets/brand/wocao-text.png";
import { useUpdaterStore } from "../stores/updater";
import type { WorkspacePage } from "../types/ui";

defineProps<{ activePage: WorkspacePage }>();
const emit = defineEmits<{ navigate: [page: WorkspacePage] }>();
const updater = useUpdaterStore();

const navigation: Array<{
  id: WorkspacePage;
  label: string;
  icon: typeof PhHouse;
}> = [
  { id: "home", label: "首页", icon: PhHouse },
  { id: "locale", label: "中文设置", icon: PhTranslate },
  { id: "software", label: "软件工具", icon: PhPackage },
  { id: "repair", label: "修复诊断", icon: PhLifebuoy },
];
</script>

<template>
  <aside class="sidebar">
    <div class="brand">
      <img :src="brandLogo" alt="wocao.ai" />
    </div>

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
      <button
        type="button"
        :class="['nav-item', { active: activePage === 'settings' }]"
        @click="emit('navigate', 'settings')"
      >
        <PhGearSix :size="19" weight="regular" />
        <span>设置</span>
        <i v-if="updater.state.phase === 'available'" class="update-dot" aria-label="发现新版本" />
      </button>
      <div class="version-line">
        <span>Wocao Hub</span>
        <span>v0.1.0</span>
      </div>
    </div>
  </aside>
</template>
