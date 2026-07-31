import { openUrl } from "@tauri-apps/plugin-opener";

export const DADA_LINKS = {
  home: "https://dadaapi.com/",
  pricing: "https://dadaapi.com/pricing",
  console: "https://dadaapi.com/console",
  referral: "https://dadaapi.com/console/referral",
  topup: "https://dadaapi.com/console/topup",
  docs: "https://docs.dadaapi.com/",
} as const;

export async function openExternalLink(url: string): Promise<void> {
  try {
    await openUrl(url);
  } catch (error) {
    console.error("无法打开外部链接", error);
  }
}
