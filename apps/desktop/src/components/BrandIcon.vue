<script setup lang="ts">
import { siClaude, siClaudecode, siNodedotjs } from "simple-icons";
import { computed } from "vue";
import ccSwitchIcon from "../assets/brands/cc-switch.png";
import openAiIcon from "../assets/brands/openai.svg";

import vsCodeIcon from "../assets/brands/vscode.svg";

type BrandName = "openai" | "claude" | "claudeCode" | "node" | "ccSwitch" | "vscode";

const props = withDefaults(
  defineProps<{
    brand: BrandName;
    size?: number;
  }>(),
  { size: 28 },
);

const simpleIcon = computed(() => {
  switch (props.brand) {
    case "claude":
      return siClaude;
    case "claudeCode":
      return siClaudecode;
    case "node":
      return siNodedotjs;
    default:
      return null;
  }
});
</script>

<template>
  <img
    v-if="brand === 'ccSwitch' || brand === 'openai' || brand === 'vscode'"
    :class="['brand-icon-image', `brand-image-${brand}`]"
    :src="brand === 'ccSwitch' ? ccSwitchIcon : brand === 'vscode' ? vsCodeIcon : openAiIcon"
    :alt="brand === 'ccSwitch' ? 'CC Switch' : brand === 'vscode' ? 'Visual Studio Code' : 'OpenAI'"
    :width="size"
    :height="size"
  />
  <svg
    v-else-if="simpleIcon"
    class="brand-icon-svg"
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    role="img"
    :aria-label="simpleIcon.title"
  >
    <path :d="simpleIcon.path" fill="currentColor" />
  </svg>
</template>
