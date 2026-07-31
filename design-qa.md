# 哒哒助手 Design QA

## Evidence

- Source visual truth: `/var/folders/y_/46lw4gt95s519b6dr5zzlwbh0000gn/T/codex-clipboard-7bf1bef3-746f-4080-8e45-f1cbd4f4013b.png`
- Source pixels: 2782 x 1652 RGBA PNG.
- Normalized source: `/tmp/dada-source-normalized-1240x780.jpg`, aspect-preserving resize on a white 1240 x 780 canvas.
- Full-view comparison: `/tmp/dada-brand-final-comparison.jpg`, normalized source and implementation placed side by side at 2480 x 780.
- Main implementation: `/tmp/dada-assistant-home-final-1240x780.jpg`.
- Narrow implementation: `/tmp/dada-assistant-home-final-900x640.jpg`.
- Additional implementation states:
  - `/tmp/dada-assistant-locale-final-1240x780.jpg`
  - `/tmp/dada-assistant-software-final-1240x780.jpg`
  - `/tmp/dada-assistant-cli-final-1240x780.jpg`
  - `/tmp/dada-assistant-repair-final-1240x780.jpg`
- CSS viewports: 1240 x 780 and 900 x 640 at device scale factor 1.
- Implementation pixels match the corresponding CSS viewport dimensions.
- State: fixed light theme; Tauri APIs unavailable in browser preview, so the captured app state intentionally exercises detection and loading-error fallbacks.

## Full-view Comparison

The desktop workspace deliberately adapts the landing-page brand rather than copying its marketing composition. The comparison confirms a consistent real logo, ink-black typography, deep teal primary actions, bright-teal accents, white canvas, restrained gray dividers, and Little D artwork. The application uses a compact operational hierarchy suitable for repeated desktop tasks while retaining the brand's visual identity.

At 1240 x 780, the 210 px navigation rail, task header, mascot, quick actions, and service links remain fully visible without overlap or clipping. At 900 x 640, the rail collapses to icons, quick actions become rows, service links wrap to two rows, and the document width remains exactly 900 px with no horizontal overflow.

## Focused-region Comparison

Separate focused crops were not required because the original-density full screenshots keep the logo, typography, icons, status labels, borders, and mascots readable. The additional locale, desktop software, CLI, and repair screenshots provide focused evidence for the denser operational areas that are not represented by the marketing source.

## Fidelity Surfaces

- Fonts and typography: Chinese uses the configured PingFang SC / Microsoft YaHei fallbacks and Latin text uses Instrument Sans. Headings stay near 28 px, body copy is 14-15 px, letter spacing is zero, and no text truncates or collides at either tested viewport.
- Spacing and layout rhythm: full-width task sections, 8 px-or-less container radii, stable controls, light dividers, and compact cards create a consistent operational rhythm. No nested decorative cards, oversized hero typography, or layout jumps remain.
- Colors and tokens: `#080D14`, `#0B8A83`, and `#12CFC3` map directly to the source brand. Surfaces remain white/light gray, with semantic error and status colors expressed through text, icons, and color together. No gradients or dark-theme tokens remain.
- Image quality and asset fidelity: official SVG logo/favicon assets and supplied transparent Little D poses are used directly. Mascots preserve their aspect ratio and transparency without crop or halo artifacts. Product icons use their real supplied assets or the selected icon library; no emoji, CSS illustration, or approximate mascot was introduced.
- Copy and content: navigation and home shortcuts consistently use the task-oriented names `配置中文`, `安装软件`, and `恢复与诊断`. User-facing fallbacks are Simplified Chinese and no implementation notes, roadmap copy, version label, or settings content appears in the product UI.
- Accessibility and interaction: navigation and segmented controls are semantic buttons with visible selected/focus states and image alternative text. Status is not communicated by color alone. Primary navigation and the desktop/CLI segmented control were exercised in the in-app browser.

## Comparison History

### Pass 1 - blocked

- [P2] Raw runtime error leaked into user-facing status.
  - Evidence: browser-only preview surfaced `Cannot read properties of undefined (reading 'invoke')` on home, software, and repair views when Tauri APIs were unavailable.
  - Impact: an English JavaScript implementation error replaced the intended Simplified Chinese operational message.
  - Fix: added a shared `isCommandError` guard that accepts only structured errors with string `code` and `message`; all other values now use the page-specific Chinese fallback.

### Pass 2 - passed

- Post-fix evidence: the final home capture shows `无法检测 ChatGPT/Codex`, software shows `无法读取下载任务`, and repair shows `无法读取当前网络恢复状态。`.
- Navigation labels were then aligned with the home shortcuts and all screenshots were recaptured.
- Browser interactions tested: home, locale, software, CLI segment, repair, sidebar collapse, and page reload.
- Browser console errors/warnings checked after reload and primary navigation: none observed.
- External destinations checked on 2026-07-31: home, pricing, console, referral, top-up, and docs all returned HTTP 200 after redirects.
- No actionable P0, P1, or P2 findings remain.

## Findings

No actionable P0, P1, or P2 visual, responsive, content, or interaction mismatches remain.

## Open Questions

None. Native-only download, cancellation, installation, proxy recovery, and activation outcomes continue to be covered by Rust/Vue checks because the browser preview cannot invoke Tauri commands.

## Implementation Checklist

- [x] Match the Dada API light brand palette and typography.
- [x] Use the official logo, favicon, and supplied Little D poses.
- [x] Remove settings, dark theme, self-update UI and updater integration.
- [x] Verify task navigation and desktop/CLI switching.
- [x] Verify 1240 x 780 and 900 x 640 layouts without overflow.
- [x] Replace raw runtime errors with Simplified Chinese fallbacks.
- [x] Align sidebar task names with home quick actions.

## Follow-up Polish

No P3 follow-up is required for the current scope.

final result: passed
