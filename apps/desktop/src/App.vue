<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import SidebarNav from "./components/SidebarNav.vue";
import { getLocaleOverview } from "./services/locale";
import type { WorkspacePage } from "./types/ui";
import type { CommandError, LocaleOverview } from "./types/locale";
import { useUpdaterStore } from "./stores/updater";
import { useActivationStore } from "./stores/activation";
import HomeView from "./views/HomeView.vue";
import LocaleSetupView from "./views/LocaleSetupView.vue";
import RepairView from "./views/RepairView.vue";
import SettingsView from "./views/SettingsView.vue";
import SoftwareView from "./views/SoftwareView.vue";

const activePage = ref<WorkspacePage>("home");
const updater = useUpdaterStore();
const activation = useActivationStore();
const overview = ref<LocaleOverview | null>(null);
const loading = ref(true);
const loadError = ref("");

onMounted(() => {
  void updater.initialize();
  void Promise.all([refreshOverview(), activation.initialize()]);
  globalThis.addEventListener("focus", refreshOverview);
});

onUnmounted(() => {
  globalThis.removeEventListener("focus", refreshOverview);
  activation.dispose();
});

const activeView = computed(() => {
  switch (activePage.value) {
    case "locale":
      return LocaleSetupView;
    case "software":
      return SoftwareView;
    case "repair":
      return RepairView;
    case "settings":
      return SettingsView;
    default:
      return HomeView;
  }
});

const activeProps = computed(() => {
  if (activePage.value !== "home" && activePage.value !== "locale") {
    return {};
  }
  return {
    overview: overview.value,
    loading: loading.value,
    error: loadError.value,
  };
});

async function refreshOverview(): Promise<void> {
  loading.value = true;
  loadError.value = "";
  try {
    overview.value = await getLocaleOverview();
  } catch (error) {
    loadError.value = errorMessage(error, "无法检测 ChatGPT/Codex");
  } finally {
    loading.value = false;
  }
}

function errorMessage(error: unknown, fallback: string): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as CommandError).message);
  }
  return typeof error === "string" ? error : fallback;
}

function navigate(page: WorkspacePage): void {
  activePage.value = page;
}
</script>

<template>
  <div class="workspace-shell">
    <SidebarNav :active-page="activePage" @navigate="navigate" />

    <main class="workspace-main">
      <Transition name="page" mode="out-in">
        <component
          :is="activeView"
          :key="activePage"
          v-bind="activeProps"
          @navigate="navigate"
          @refresh="refreshOverview"
        />
      </Transition>
    </main>
  </div>
</template>
