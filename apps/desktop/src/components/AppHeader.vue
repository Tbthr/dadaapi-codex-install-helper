<script setup lang="ts">
import { Home, RefreshCw, Wrench } from "lucide-vue-next";
import brandIcon from "../assets/brand/app-icon.png";

defineProps<{
  loading: boolean;
  busy: boolean;
  toolsOpen: boolean;
}>();

const emit = defineEmits<{
  refresh: [];
  toggleTools: [];
}>();
</script>

<template>
  <header class="app-header">
    <div class="app-bar">
      <div class="brand-lockup">
        <img :src="brandIcon" alt="" />
        <strong>Wocao Hub</strong>
      </div>

      <div class="header-actions">
        <button
          class="refresh-button"
          type="button"
          :aria-label="toolsOpen ? '返回中文设置' : '打开工具'"
          :title="toolsOpen ? '返回中文设置' : '打开工具'"
          :disabled="busy"
          @click="emit('toggleTools')"
        >
          <Home v-if="toolsOpen" :size="18" />
          <Wrench v-else :size="18" />
        </button>

        <button
          v-if="!toolsOpen"
          class="refresh-button"
          type="button"
          :aria-label="loading ? '检测中' : '重新检测'"
          :title="loading ? '检测中' : '重新检测'"
          :disabled="loading || busy"
          @click="emit('refresh')"
        >
          <RefreshCw :size="18" :class="{ spinning: loading }" />
        </button>
      </div>
    </div>
  </header>
</template>
