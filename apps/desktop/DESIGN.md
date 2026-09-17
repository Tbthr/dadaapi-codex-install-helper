---
name: 哒哒助手
description: 清爽、可信的本地 AI 工具配置工作台。
colors:
  canvas: "#ffffff"
  surface: "#ffffff"
  surface-soft: "#f3f6f5"
  surface-hover: "#edf4f3"
  header-soft: "#f8faf9"
  ink: "#080d14"
  ink-secondary: "#455260"
  ink-muted: "#768391"
  border: "#e5e9e8"
  border-strong: "#cbd4d2"
  dada-signal-teal: "#0b8a83"
  signal-teal-hover: "#08766f"
  signal-teal-soft: "#e5f8f6"
  signal-bright: "#12cfc3"
  success: "#14745e"
  success-soft: "#e8f6f1"
  success-border: "#b9e2d4"
  warning: "#99631e"
  danger: "#ba3f48"
  danger-soft: "#fff0f1"
  brand-openai: "#171d1c"
  brand-claude: "#d66d45"
  brand-claude-soft: "#fff4ef"
  brand-node: "#4d9445"
  brand-node-soft: "#eef8ed"
  brand-ccswitch-soft: "#f5f7f6"
  brand-vscode-soft: "#edf6fc"
  card-hover: "#fcfefd"
  card-hover-border: "#b9ceca"
  locale-border: "#b9ddd8"
  locale-hover: "#d9f3f0"
typography:
  title:
    fontFamily: '"PingFang SC", "Microsoft YaHei UI", "Instrument Sans Variable", sans-serif'
    fontSize: "19px"
    fontWeight: 720
  section-title:
    fontFamily: '"PingFang SC", "Microsoft YaHei UI", "Instrument Sans Variable", sans-serif'
    fontSize: "16px"
    fontWeight: 700
  card-title:
    fontFamily: '"PingFang SC", "Microsoft YaHei UI", "Instrument Sans Variable", sans-serif'
    fontSize: "15px"
    fontWeight: 690
  body:
    fontFamily: '"PingFang SC", "Microsoft YaHei UI", "Instrument Sans Variable", sans-serif'
    fontSize: "12px"
    fontWeight: 560
    lineHeight: 1.45
  label:
    fontFamily: '"PingFang SC", "Microsoft YaHei UI", "Instrument Sans Variable", sans-serif'
    fontSize: "10px"
    fontWeight: 650
  brand-micro:
    fontFamily: '"Instrument Sans Variable", sans-serif'
    fontSize: "10px"
    fontWeight: 650
rounded:
  sm: "6px"
  md: "8px"
  pill: "999px"
spacing:
  content-gutter: "34px"
  page-top: "29px"
  page-bottom: "36px"
  section-gap: "25px"
  card-gap: "10px"
  card-padding: "15px"
  drawer-content: "20px"
  action-gap: "12px"
components:
  button-primary:
    backgroundColor: "{colors.dada-signal-teal}"
    textColor: "{colors.surface}"
    typography: "{typography.body}"
    rounded: "{rounded.sm}"
    padding: "0 16px"
    height: "42px"
  button-primary-hover:
    backgroundColor: "{colors.signal-teal-hover}"
    textColor: "{colors.surface}"
    typography: "{typography.body}"
    rounded: "{rounded.sm}"
    padding: "0 16px"
    height: "42px"
  button-quiet:
    backgroundColor: "transparent"
    textColor: "{colors.signal-teal-hover}"
    typography: "{typography.body}"
    rounded: "0"
    padding: "0"
    height: "30px"
  software-card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.md}"
    padding: "{spacing.card-padding}"
    height: "184px"
  status-pill:
    backgroundColor: "{colors.surface-soft}"
    textColor: "{colors.ink-muted}"
    rounded: "{rounded.pill}"
    padding: "0 8px"
    height: "25px"
  locale-action:
    backgroundColor: "{colors.signal-teal-soft}"
    textColor: "{colors.signal-teal-hover}"
    rounded: "{rounded.sm}"
    padding: "0 10px"
    height: "32px"
---

# Design System: 哒哒助手

## Overview

**Creative North Star: "青色信号台"**

哒哒助手当前是一套轻量的桌面操作界面：白色画布承载紧凑的工具卡片，深墨色文字负责清晰阅读，哒哒信号青只在动作、进度和状态确认处发出信号。它更像一座安静、可信的本地工作台，而不是需要被浏览的营销页面。

信息密度是实用而克制的。固定桌面网格、细边框、柔和的冷灰表面和右侧滑入的配置抽屉共同建立层级；状态和恢复路径始终优先于装饰。现有风格明确避免暗黑霓虹、强渐变、过度玻璃拟态和营销化大标题。

**Key Characteristics:**

- 浅色优先，以白色和冷灰绿色表面建立秩序。
- 哒哒信号青是稀缺的操作与状态信号，不是大面积背景。
- 紧凑的卡片网格承载软件和 CLI 工具，右侧抽屉承载中文配置流程。
- 中文 UI 使用明确的状态、进度、步骤和恢复反馈。
- 交互反馈精准而有触感，但不以动画或阴影制造噪声。

## Colors

整体调色板以冷白和深墨为骨架，以哒哒信号青提供唯一主要强调，再用少量状态色和软件品牌色区分语义。颜色承担功能分层：中性表面负责背景与容器，青色负责可操作动作，绿色/琥珀/红色只表达结果或风险。

### Primary

- **哒哒信号青** (#0b8a83): 主要操作、已完成步骤、可用链接和核心品牌信号。
- **信号青悬停** (#08766f): 交互悬停和更高对比度的动作文本。
- **信号青柔光** (#e5f8f6): 配置中文等辅助动作的轻量背景。
- **青色进度光** (#12cfc3): 下载进度和键盘焦点环的明亮反馈。

### Neutral

- **白色画布** (#ffffff): 页面、卡片和抽屉的基础表面。
- **冷灰表面** (#f3f6f5): 状态标签和操作栏的次级容器。
- **浅青灰悬停层** (#edf4f3): 品牌按钮、链接和局部交互的悬停背景。
- **冷白顶栏** (#f8faf9): 品牌栏的轻微分离。
- **深墨文字** (#080d14): 标题、正文和主要信息。
- **次级蓝灰文字** (#455260): 辅助说明和次要操作。
- **静音灰文字** (#768391): 元信息、版本和未激活状态。
- **细灰边框** (#e5e9e8): 卡片、分隔线和结构边界。
- **强灰边框** (#cbd4d2): 抽屉边界和未完成步骤的轮廓。

### Status & Brand Accents

- **成功绿** (#14745e) 与 **成功柔层** (#e8f6f1): 已安装、已完成和网络恢复成功。
- **警示琥珀** (#99631e): 需要注意但不阻断流程的信息。
- **错误红** (#ba3f48) 与 **错误柔层** (#fff0f1): 失败、不可用和需要重试的状态。
- 软件品牌色仅用于对应软件图标容器：OpenAI 深色 (#171d1c)、Claude 暖橙 (#d66d45)、Node 绿 (#4d9445) 及各自的浅色底。

### Named Rules

**The Signal-Sparingly Rule.** 哒哒信号青只出现在行动、状态和进度等需要用户注意的地方；大部分面积必须保持中性。

## Typography

**Display Font:** 无独立展示字体；当前界面不是标题驱动的展示型产品。
**Body Font:** `PingFang SC`, `Microsoft YaHei UI`, `Instrument Sans Variable`, sans-serif
**Label/Mono Font:** `Instrument Sans Variable`, sans-serif，仅用于 `DADA API` 品牌微标签。

**Character:** 中文 UI 采用系统优先的清晰字形和偏高字重，保证小尺寸状态文字仍可扫描；Instrument Sans Variable 只在品牌副标题处提供轻微的英文节奏变化。

### Hierarchy

- **Title** (720, 19px): 配置中文抽屉标题等当前任务的主标题。
- **Section title** (700, 16px): “哒哒 API”“桌面应用”“命令行工具”等内容区标题。
- **Card title** (690, 15px): 软件名称和品牌锁定区名称。
- **Body** (560–660, 11–14px, 1.4–1.45 line-height): 发布者、版本、辅助说明和操作文本。
- **Label** (650–700, 10–11px): 状态 pill、品牌副标题、步骤状态和区段辅助标签。

### Named Rules

**The Compact Hierarchy Rule.** 用字重、颜色和位置先建立层级，再使用字号差异；不要通过巨型标题或夸张字重改变桌面工作台的密度。

## Layout

整体是最小宽度 900px、最小高度 640px 的桌面画布，应用 shell 由 66px 品牌栏和可滚动内容区组成。内容最大宽度为 1280px，默认左右 gutter 为 34px，页面顶部/底部留白为 29px/36px；短窗口高度下顶部留白收紧到 22px。

桌面应用区域使用三列等宽卡片网格，卡片间距为 10px；视口宽度不超过 1040px 时收窄为两列。哒哒 API 服务链接默认五列排布，宽度不超过 940px 时改为三列并增加行分隔。命令行工具固定采用两列。内容本身在页面容器中滚动并隐藏滚动条，避免破坏紧凑的操作感。

中文配置使用右侧滑入抽屉，宽度为 `min(560px, calc(100vw - 48px))`。抽屉顶部是 74px 的粘性标题栏，下方是 20px 内边距的五步设置卡片；底部操作栏使用冷灰表面与主操作按钮保持视觉锚点。

### Named Rules

**The Task-First Layout Rule.** 页面空间优先留给可检测、可下载、可配置和可恢复的真实操作；服务链接保持轻量，中文设置流程在抽屉中保持连续。

## Elevation & Depth

系统采用平面优先、层级克制的深度策略。页面、卡片和抽屉主体都以白色表面和 1px 边框分层；卡片悬停只上移 1px 并轻微改变背景和边框。只有右侧配置抽屉使用阴影与 34% 深墨遮罩从主页面脱离。

### Shadow Vocabulary

- **Drawer shadow** (`-12px 0 36px rgb(8 13 20 / 14%)`): 仅用于右侧中文配置抽屉。
- **Backdrop veil** (`rgb(8 13 20 / 34%)`): 抽屉打开时压低背景，而不模糊或染色内容。

### Named Rules

**The Flat-By-Default Rule.** 静止状态不使用泛滥阴影；只有浮层或明确的交互状态才获得额外深度。

## Shapes

形状语言是温和但不膨胀的圆角：常规控件使用 6px，小型容器和卡片使用 8px，状态 pill 和步骤索引使用完全圆形。所有结构边界保持 1px 细线，软件图标容器和卡片共享 8px 轮廓；按钮不使用胶囊形，避免把每个动作都做成标签。

### Named Rules

**The Soft-Corner Rule.** 圆角负责减轻工具界面的硬度，但不能削弱网格边界；优先使用 6px/8px 和细边框，不引入大型圆角面板。

## Components

### Buttons

- **Shape:** 6px 轻圆角；主按钮最小高度 42px，静默操作最小高度 30px。
- **Primary:** 哒哒信号青背景、白色文字、`0 16px` 内边距，常用于“配置中文”等主流程动作。
- **Hover / Focus:** 悬停变为信号青悬停色并上移 1px；键盘焦点使用 3px 半透明青色轮廓，外偏移 2px。
- **Quiet / Locale:** 下载和安装动作使用无边框透明按钮；“配置中文”使用信号青柔层背景、1px 淡青边框和 `0 10px` 内边距。

### Chips

- **Style:** 软件状态和流程状态是 999px pill，最小高度 25px，冷灰表面与静音灰文字。
- **State:** 已安装或成功状态切换为成功绿文字、成功边框和成功柔层；默认状态保持低对比度。

### Cards / Containers

- **Corner Style:** 软件卡片使用 8px 圆角，最小高度 184px。
- **Background:** 默认白色；悬停使用近白卡片悬停色和更明确的边框。
- **Shadow Strategy:** 卡片不使用阴影，依靠边框、留白和 1px 悬停位移建立层次。
- **Border:** 默认细灰边框，悬停切换为浅青灰边框。
- **Internal Padding:** 默认 15px；图标容器为 44px，中文流程中的应用符号为 48px。

### Navigation

- **Style:** 顶部 66px 品牌栏是轻量全局入口，不是多级导航；品牌 Logo、名称和 `DADA API` 副标签靠左，品牌口号靠右。
- **State:** 品牌锁定区透明静止，悬停使用浅青灰背景；服务链接使用细分隔线、12px 文字和向外箭头。

### Chinese Setup Drawer

右侧抽屉是当前系统的签名组件：粘性标题栏、ChatGPT/Codex 微标签、应用状态行、五步列表和底部主操作栏。步骤索引从灰色数字过渡到青色勾选圆点，完成步骤标题使用成功绿；错误时只把底部操作栏切换为错误柔层，不改变整个抽屉的结构。

## Do's and Don'ts

### Do:

- **Do** 让哒哒信号青承担明确的动作、进度或成功语义。
- **Do** 使用白色/冷灰表面、1px 边框和 6px/8px 圆角维持轻量桌面密度。
- **Do** 保持软件卡片、服务链接和右侧配置抽屉的紧凑网格关系。
- **Do** 用真实的安装、下载、中文配置、网络恢复状态驱动视觉反馈。
- **Do** 为键盘焦点、悬停、下载进度和步骤完成提供与现有系统同等级的反馈。

### Don't:

- **Don't** 把哒哒信号青铺满页面，或用它替代所有中性层级。
- **Don't** 引入暗黑霓虹、强渐变、过度玻璃拟态或大面积装饰性阴影。
- **Don't** 添加营销化的大标题、虚构数据或与当前任务无关的展示模块。
- **Don't** 把每个按钮都做成胶囊形，也不要用大型圆角取代细边框网格。
- **Don't** 在没有真实状态支撑时添加“成功”、进度或可用性视觉。
