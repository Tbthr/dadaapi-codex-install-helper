import { defineStore } from "pinia";
import { ref } from "vue";

export type ThemeMode = "system" | "dark" | "light";

const STORAGE_KEY = "wocao-hub-theme";

export const useThemeStore = defineStore("theme", () => {
  const mode = ref<ThemeMode>("system");
  let initialized = false;
  let systemTheme: MediaQueryList | null = null;

  function initialize(): void {
    if (initialized) return;
    initialized = true;
    const saved = readSavedMode();
    if (saved) mode.value = saved;
    systemTheme = globalThis.matchMedia("(prefers-color-scheme: dark)");
    systemTheme.addEventListener("change", applyTheme);
    applyTheme();
  }

  function setMode(nextMode: ThemeMode): void {
    mode.value = nextMode;
    try {
      globalThis.localStorage.setItem(STORAGE_KEY, nextMode);
    } catch {
      // 系统禁止本地存储时仍应用当前会话的主题。
    }
    applyTheme();
  }

  function applyTheme(): void {
    const resolved =
      mode.value === "system" ? (systemTheme?.matches ? "dark" : "light") : mode.value;
    document.documentElement.dataset.theme = resolved;
    document.documentElement.style.colorScheme = resolved;
  }

  return { mode, initialize, setMode };
});

function readSavedMode(): ThemeMode | null {
  try {
    const value = globalThis.localStorage.getItem(STORAGE_KEY);
    return value === "system" || value === "dark" || value === "light" ? value : null;
  } catch {
    return null;
  }
}
