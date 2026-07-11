<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import SidebarNav from "./components/SidebarNav.vue";
import type { WorkspacePage } from "./types/ui";
import { useUpdaterStore } from "./stores/updater";
import HomeView from "./views/HomeView.vue";
import LocaleSetupView from "./views/LocaleSetupView.vue";
import RepairView from "./views/RepairView.vue";
import SettingsView from "./views/SettingsView.vue";
import SoftwareView from "./views/SoftwareView.vue";

const activePage = ref<WorkspacePage>("home");
const updater = useUpdaterStore();

onMounted(() => {
  void updater.initialize();
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

function navigate(page: WorkspacePage): void {
  activePage.value = page;
}
</script>

<template>
  <div class="workspace-shell">
    <SidebarNav :active-page="activePage" @navigate="navigate" />

    <main class="workspace-main">
      <Transition name="page" mode="out-in">
        <component :is="activeView" :key="activePage" @navigate="navigate" />
      </Transition>
    </main>
  </div>
</template>
