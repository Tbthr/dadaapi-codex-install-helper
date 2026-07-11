<script setup lang="ts">
import { PhFileZip, PhGlobeHemisphereWest } from "@phosphor-icons/vue";

const repairActions = [
  {
    title: "恢复原网络",
    description: "恢复设置中文前保存的系统代理状态",
    action: "恢复网络",
    icon: PhGlobeHemisphereWest,
    disabled: true,
  },
  {
    title: "导出诊断",
    description: "生成经过脱敏处理的日志与状态摘要",
    action: "导出文件",
    icon: PhFileZip,
    disabled: false,
  },
];
</script>

<template>
  <div class="page repair-page">
    <header class="page-header">
      <span class="eyebrow">系统工具</span>
      <h1>修复诊断</h1>
      <p>只处理 Wocao Hub 修改过的配置和网络状态。</p>
    </header>

    <section class="list-panel repair-list">
      <div v-for="item in repairActions" :key="item.title" class="repair-row">
        <span class="list-icon"><component :is="item.icon" :size="22" /></span>
        <div class="list-copy">
          <strong>{{ item.title }}</strong>
          <span>{{ item.description }}</span>
        </div>
        <button type="button" class="row-button" :disabled="item.disabled">
          {{ item.disabled ? "无需处理" : item.action }}
        </button>
      </div>
    </section>
  </div>
</template>
