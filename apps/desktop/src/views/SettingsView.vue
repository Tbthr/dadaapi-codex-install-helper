<script setup lang="ts">
import { PhBell, PhDownloadSimple, PhInfo, PhMoon } from "@phosphor-icons/vue";
import { computed, ref } from "vue";
import { useUpdaterStore } from "../stores/updater";

const followSystemTheme = ref(true);
const updater = useUpdaterStore();

const updateProgress = computed(() => {
  const total = updater.state.totalBytes;
  if (!total || total <= 0) {
    return 0;
  }
  return Math.min(100, Math.round((updater.state.downloadedBytes / total) * 100));
});

const updateStatus = computed(() => {
  switch (updater.state.phase) {
    case "checking":
      return "正在检查 GitHub Releases";
    case "available":
      return `发现新版本 v${updater.state.update?.version ?? ""}`;
    case "downloading":
      return updater.state.totalBytes ? `正在下载 ${updateProgress.value}%` : "正在下载安装包";
    case "ready":
      return "更新已安装，重启后生效";
    case "current":
      return "已是最新版本";
    case "error":
      return updater.state.message;
    default:
      return "尚未检查更新";
  }
});

const updateAction = computed(() => {
  switch (updater.state.phase) {
    case "available":
      return "下载并安装";
    case "downloading":
      return `${updateProgress.value}%`;
    case "ready":
      return "立即重启";
    case "checking":
      return "检查中";
    default:
      return "检查更新";
  }
});

async function handleUpdateAction(): Promise<void> {
  if (updater.state.phase === "available") {
    await updater.install();
  } else if (updater.state.phase === "ready") {
    await updater.restart();
  } else {
    await updater.checkNow();
  }
}
</script>

<template>
  <div class="page settings-page">
    <header class="page-header">
      <span class="eyebrow">偏好设置</span>
      <h1>设置</h1>
      <p>管理更新、外观和下载行为。</p>
    </header>

    <section class="settings-group">
      <span class="group-title">软件更新</span>
      <div class="setting-row">
        <span class="list-icon"><PhBell :size="21" /></span>
        <div class="list-copy">
          <strong>自动检查更新</strong>
          <span>每次启动检查一次，下载与重启由你确认</span>
        </div>
        <span class="setting-value success-text">已启用</span>
      </div>
      <div class="setting-row">
        <span class="list-icon"><PhDownloadSimple :size="21" /></span>
        <div class="list-copy">
          <strong>当前版本</strong>
          <span>Wocao Hub v{{ updater.state.currentVersion }} · {{ updateStatus }}</span>
          <div v-if="updater.state.phase === 'downloading'" class="setting-progress">
            <i :style="{ width: `${updateProgress}%` }" />
          </div>
        </div>
        <button
          type="button"
          class="row-button"
          :disabled="updater.state.phase === 'checking' || updater.state.phase === 'downloading'"
          @click="handleUpdateAction"
        >
          {{ updateAction }}
        </button>
      </div>
    </section>

    <section class="settings-group">
      <span class="group-title">外观</span>
      <div class="setting-row">
        <span class="list-icon"><PhMoon :size="21" /></span>
        <div class="list-copy">
          <strong>跟随系统主题</strong>
          <span>根据 Windows 或 macOS 外观自动切换</span>
        </div>
        <button
          type="button"
          role="switch"
          :aria-checked="followSystemTheme"
          :class="['switch-control', { active: followSystemTheme }]"
          @click="followSystemTheme = !followSystemTheme"
        >
          <i />
        </button>
      </div>
    </section>

    <section class="settings-group">
      <span class="group-title">关于</span>
      <div class="setting-row">
        <span class="list-icon"><PhInfo :size="21" /></span>
        <div class="list-copy">
          <strong>Wocao Hub</strong>
          <span>wocao.ai 开源跨平台 AI 工具</span>
        </div>
        <button type="button" class="row-button">项目主页</button>
      </div>
    </section>
  </div>
</template>
