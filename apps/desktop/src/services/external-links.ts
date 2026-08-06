import { openUrl } from "@tauri-apps/plugin-opener";

export const DADA_LINKS = {
  home: "https://dadaapi.com/",
  pricing: "https://dadaapi.com/pricing",
  console: "https://dadaapi.com/console",
  topup: "https://dadaapi.com/console/topup",
  guide: "https://my.feishu.cn/wiki/VE6Cwa1LsiKSZLkhZYCcXWSqn3b?from=from_copylink",
  qqGroup: "https://qm.qq.com/q/JYMSL80HSw",
} as const;

export async function openExternalLink(url: string): Promise<void> {
  try {
    await openUrl(url);
  } catch (error) {
    console.error("无法打开外部链接", error);
  }
}
