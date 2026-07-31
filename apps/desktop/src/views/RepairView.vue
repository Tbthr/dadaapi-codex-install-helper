<script setup lang="ts">
import { PhFileZip, PhGlobeHemisphereWest, PhSpinnerGap } from "@phosphor-icons/vue";
import { storeToRefs } from "pinia";
import { onMounted, ref } from "vue";
import { exportDiagnostics, revealDiagnosticsExport } from "../services/diagnostics";
import { useActivationStore } from "../stores/activation";
import { isCommandError } from "../types/locale";
import type { DiagnosticExportResult } from "../types/diagnostics";

const activation = useActivationStore();
const {
  networkStatusState,
  networkStatusError,
  networkPending,
  localProxyActive,
  recoveryRunning,
  recoveryError,
} = storeToRefs(activation);
const exportRunning = ref(false);
const exportError = ref("");
const diagnosticExport = ref<DiagnosticExportResult | null>(null);

onMounted(() => {
  void activation.refreshNetworkStatus();
});

async function restoreNetwork(): Promise<void> {
  await activation.restoreOriginalNetwork();
}

async function handleDiagnostics(): Promise<void> {
  if (exportRunning.value) {
    return;
  }
  exportError.value = "";
  if (diagnosticExport.value) {
    try {
      await revealDiagnosticsExport(diagnosticExport.value.fileName);
    } catch (error) {
      exportError.value = errorMessage(error, "无法打开诊断文件所在目录");
    }
    return;
  }
  exportRunning.value = true;
  try {
    diagnosticExport.value = await exportDiagnostics();
    await revealDiagnosticsExport(diagnosticExport.value.fileName);
  } catch (error) {
    exportError.value = errorMessage(error, "无法导出诊断信息");
  } finally {
    exportRunning.value = false;
  }
}

function errorMessage(error: unknown, fallback: string): string {
  if (isCommandError(error)) {
    return error.message;
  }
  return fallback;
}
</script>

<template>
  <div class="page repair-page">
    <header class="page-header">
      <span class="eyebrow">系统工具</span>
      <h1>修复诊断</h1>
      <p>只处理哒哒助手修改过的配置和网络状态。</p>
    </header>

    <section class="list-panel repair-list">
      <div class="repair-row">
        <span class="list-icon"><PhGlobeHemisphereWest :size="22" /></span>
        <div class="list-copy">
          <strong>恢复原网络</strong>
          <span v-if="networkStatusState === 'error'">{{ networkStatusError }}</span>
          <span v-else-if="recoveryError">{{ recoveryError }}</span>
          <span v-else-if="networkPending && localProxyActive">
            临时代理正在使用，恢复后会关闭本地代理
          </span>
          <span v-else-if="networkPending">检测到待恢复的系统代理状态</span>
          <span v-else>当前没有哒哒助手遗留的代理状态</span>
        </div>
        <button
          type="button"
          class="row-button"
          :disabled="networkStatusState !== 'ready' || !networkPending || recoveryRunning"
          @click="restoreNetwork"
        >
          <PhSpinnerGap v-if="recoveryRunning" class="spinning" :size="15" />
          {{
            recoveryRunning
              ? "正在恢复"
              : networkStatusState === "loading" || networkStatusState === "unknown"
                ? "检测中"
                : networkPending
                  ? "恢复网络"
                  : "无需处理"
          }}
        </button>
      </div>

      <div class="repair-row">
        <span class="list-icon"><PhFileZip :size="22" /></span>
        <div class="list-copy">
          <strong>导出诊断</strong>
          <span v-if="exportError">{{ exportError }}</span>
          <span v-else-if="diagnosticExport">诊断文件已生成并完成脱敏</span>
          <span v-else>生成经过脱敏处理的日志与状态摘要</span>
        </div>
        <button
          type="button"
          class="row-button"
          :disabled="exportRunning"
          @click="handleDiagnostics"
        >
          <PhSpinnerGap v-if="exportRunning" class="spinning" :size="15" />
          {{ exportRunning ? "正在导出" : diagnosticExport ? "打开文件" : "导出文件" }}
        </button>
      </div>
    </section>
  </div>
</template>
