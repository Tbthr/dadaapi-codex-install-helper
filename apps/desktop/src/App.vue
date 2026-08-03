<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { getLocaleOverview } from "./services/locale";
import { useActivationStore } from "./stores/activation";
import { isCommandError, type LocaleOverview } from "./types/locale";
import HomeView from "./views/HomeView.vue";

const activation = useActivationStore();
const overview = ref<LocaleOverview | null>(null);
const loading = ref(true);
const loadError = ref("");

onMounted(() => {
  void Promise.all([refreshOverview(), activation.initialize()]);
  globalThis.addEventListener("focus", refreshOverview);
});

onUnmounted(() => {
  globalThis.removeEventListener("focus", refreshOverview);
  activation.dispose();
});

async function refreshOverview(): Promise<void> {
  loading.value = true;
  loadError.value = "";
  try {
    overview.value = await getLocaleOverview();
  } catch (error) {
    loadError.value = isCommandError(error) ? error.message : "无法检测 ChatGPT/Codex";
  } finally {
    loading.value = false;
  }
}
</script>

<template>
  <HomeView :overview="overview" :loading="loading" :error="loadError" @refresh="refreshOverview" />
</template>
