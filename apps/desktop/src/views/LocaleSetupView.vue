<script setup lang="ts">
import { PhPlay, PhTranslate } from "@phosphor-icons/vue";
import BrandIcon from "../components/BrandIcon.vue";
</script>

<template>
  <div class="page locale-page">
    <header class="page-header">
      <span class="eyebrow">ChatGPT / Codex</span>
      <h1>中文设置</h1>
      <p>自动选择稳定节点，启动应用并验证中文界面真实生效。</p>
    </header>

    <section class="primary-panel setup-panel">
      <div class="setup-app-row">
        <span class="app-symbol brand-openai"><BrandIcon brand="openai" :size="28" /></span>
        <div>
          <strong>ChatGPT</strong>
          <span>版本 26.707.31428 · 正在运行</span>
        </div>
        <span class="status-pill success">已检测</span>
      </div>

      <div class="setup-checklist">
        <div class="check-row complete">
          <div>
            <strong>应用已就绪</strong>
            <span>已找到可启动的 ChatGPT 桌面应用</span>
          </div>
          <span class="step-state">就绪</span>
        </div>
        <div class="check-row complete">
          <div>
            <strong>路由配置已就绪</strong>
            <span>签名、完整性和有效期将在执行时验证</span>
          </div>
          <span class="step-state">就绪</span>
        </div>
        <div class="check-row complete">
          <div>
            <strong>系统网络正常</strong>
            <span>当前没有待恢复的代理状态</span>
          </div>
          <span class="step-state">正常</span>
        </div>
        <div class="check-row">
          <div>
            <strong>等待中文设置</strong>
            <span>完成后会验证应用进程使用 zh-CN 启动</span>
          </div>
          <span class="step-state pending">待执行</span>
        </div>
      </div>

      <div class="setup-action-area">
        <div class="setup-action-copy">
          <PhTranslate :size="21" />
          <p>执行期间可能会短暂重启 ChatGPT。</p>
        </div>
        <button class="primary-button large" type="button">
          <PhPlay :size="17" weight="fill" />
          开始设置
        </button>
      </div>
    </section>
  </div>
</template>
