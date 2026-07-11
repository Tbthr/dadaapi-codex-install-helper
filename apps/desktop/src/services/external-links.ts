import { openUrl } from "@tauri-apps/plugin-opener";

export const WOCAO_LINKS = {
  home: "https://wocao.ai/",
  wallet: "https://wocao.ai/wallet",
  imageGenerator: "https://wocao.ai/p/image-generator",
  docs: "https://docs.wocao.ai/",
  support: "https://wocao.ai/p/support",
} as const;

export async function openExternalLink(url: string): Promise<void> {
  try {
    await openUrl(url);
  } catch (error) {
    console.error("无法打开外部链接", error);
  }
}
