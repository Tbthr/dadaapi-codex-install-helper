---
name: 哒哒助手
description: 用青绿色唱片封套语法承载本地 AI 配置、官方下载与网络恢复。
colors:
  canvas: "#d9f3f0"
  surface: "#f8fffd"
  surface-soft: "#e5f8f6"
  ink: "#080d14"
  ink-secondary: "#17363a"
  ink-muted: "#4d6a6d"
  border: "#8bbdb8"
  signal-teal: "#0b8a83"
  signal-bright: "#12cfc3"
  danger: "#a3313e"
typography:
  display:
    fontFamily: '"PingFang SC", "Microsoft YaHei UI", sans-serif'
    fontSize: "clamp(30px, 3.3vw, 51px)"
    fontWeight: 780
    letterSpacing: "-0.045em"
  body:
    fontFamily: '"PingFang SC", "Microsoft YaHei UI", sans-serif'
    fontSize: "12px"
    fontWeight: 560
    lineHeight: 1.55
  label:
    fontFamily: '"Instrument Sans Variable", sans-serif'
    fontSize: "9px"
    fontWeight: 700
    letterSpacing: "0.12em"
rounded:
  structural: "0"
  control: "0"
spacing:
  content-gutter: "26px"
  sleeve-gap: "0"
  action-gap: "20px"
components:
  button-primary:
    backgroundColor: "{colors.signal-bright}"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
    padding: "0 14px"
    height: "45px"
  button-quiet:
    backgroundColor: "transparent"
    textColor: "{colors.signal-teal}"
    rounded: "{rounded.control}"
    padding: "0"
    height: "31px"
  status-strip:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.structural}"
    padding: "12px 12px 12px 0"
  setup-drawer:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.structural}"
    padding: "20px"
---

# Design System: 哒哒助手

## Overview

**Creative North Star: “青色唱片封套”**

哒哒助手不再把自己表现成通用 SaaS 后台，而是一张可以工作的本地配置唱片：青绿色是整张封套的品牌场域，深墨色是唱片与文字的骨架，白色是承载真实状态的内页。原始 `src/assets/brand/dada-logo.svg` 是唯一品牌标记，不绘制新 Logo。

首屏用一个完整的封套构图把“检测 ChatGPT/Codex → 配置中文 → 验证 → 恢复原网络”串成闭合集合。软件、CLI、哒哒 API 服务链接属于封套的 B-side 目录；它们仍然真实、可用，但不抢走首屏主动作。

**Key Characteristics:**

- 青绿色平涂是识别场域，不是点缀；深墨色负责稳定和可读性。
- 唱片圆面、曲目索引、封套侧栏和反白当前曲目构成信息架构，不是装饰贴图。
- 页面主结构使用零圆角、细黑边和明确的平面切割，避免圆角卡片堆叠。
- 状态同时由文字、规则、位置和色彩表达；失败与恢复不能只靠颜色。
- 动效只有一个 authored moment：封套从右侧裁切进入；抽屉继续使用真实的受保护焦点和滑入反馈。

## Colors

调色板围绕原始 Logo 的深墨与哒哒青绿建立；浅薄荷负责纸张和辅助区，错误红只表示失败或不可用。

### Primary

- **哒哒青绿** (`#12cfc3`): 封套主场域、当前动作、焦点状态。
- **信号青深色** (`#0b8a83`): 正文中的青色动作、悬停和高对比品牌信号。

### Neutral

- **原始 Logo 黑** (`#080d14`): Logo、唱片、主要文字和结构边界。
- **画布青灰** (`#d9f3f0`): 应用外层背景。
- **纸张白** (`#f8fffd`): 封套内页、列表和抽屉。
- **薄荷浅层** (`#e5f8f6`): 辅助区、恢复提示和已确认状态。
- **细边青灰** (`#8bbdb8`): 分隔线与低优先级边界。
- **错误红** (`#a3313e`): 失败、不可用和需要重试的状态。

### Named Rules

**The Original Mark Rule.** 使用 `src/assets/brand/dada-logo.svg`；任何新页面都不能用生成图形替换它。

**The Signal-State Rule.** 青绿色标记动作和已确认状态，文字与结构必须同时承担语义。

## Typography

**Display Font:** `PingFang SC`, `Microsoft YaHei UI`, sans-serif

**Body Font:** `PingFang SC`, `Microsoft YaHei UI`, sans-serif

**Label/Mono Font:** `Instrument Sans Variable`, sans-serif，用于 DADA API 与唱片索引微标签。

**Character:** 中文正文保持系统优先的清晰度；英文微标签使用窄而有节奏的字面，形成唱片封套的编辑感，不用等宽字体装饰“技术感”。

### Hierarchy

- **Display** (780, `clamp(30px, 3.3vw, 51px)`, `0.98`): 首屏“让好模型，更好用。”主标题。
- **Title** (740–760, `14–22px`): 配置中文、软件分组和抽屉标题。
- **Body** (560, `12px`, `1.55`): 状态说明、恢复提示和来源信息，长段落保持短行宽。
- **Label** (700, `9–10px`, `0.12–0.16em`): 曲目索引、品牌微标签和状态标记。

### Named Rules

**The Closed-Set Type Rule.** 主标题、状态、曲目和步骤使用清晰的字重差，不靠无限放大的标题或大量 eyebrow 形成层级。

## Layout

应用顶栏保留原始 Logo、哒哒助手名称和本机工作区标记。首屏是三段式唱片封套：左侧深色唱片区，中间状态与配置区，右侧曲目索引。中间区域依次展示 ChatGPT/Codex 的真实本机状态、“配置中文”主动作、五步配置流程预览，以及后续的哒哒 API 外链与官方下载目录。

中文配置继续使用右侧抽屉，宽度 `min(560px, calc(100vw - 48px))`；抽屉内保留五步真实流程、错误反馈、主动作和手动恢复原网络步骤。

窗口较窄时，唱片区移动到顶部，曲目索引降为一行标签，状态和五步流程改为单列；软件与 CLI 目录改为单列清单。不得为了保持海报构图而挤压中文正文。

### Named Rules

**The Sleeve-First Layout Rule.** 第一视口先读成一张完整的工作唱片，再读到目录；同等权重的重复卡片不能成为页面骨架。

## Elevation & Depth

静止表面以平面色块和边界线分层。唱片封套使用柔和的偏移模糊阴影帮助它从青灰画布中脱离；右侧中文配置抽屉使用更深的柔和阴影和遮罩。禁止零偏移的彩色光晕和装饰性玻璃模糊。

### Shadow Vocabulary

- **Sleeve shadow** (`0 18px 36px rgb(8 13 20 / 12%)`): 首屏唱片封套与画布之间的结构分离。
- **Drawer shadow** (`-18px 0 42px rgb(8 13 20 / 18%)`): 中文配置抽屉打开时的浮层层级。
- **Backdrop veil** (`rgb(8 13 20 / 48%)`): 抽屉打开时降低主页面对比度。

### Named Rules

**The Flat-by-Default Rule.** 没有明确层级关系的区域不使用阴影；深度只服务于封套和受保护的抽屉。

## Shapes

唱片封套、标签牌、状态行和按钮使用零圆角和 1–1.5px 结构边界；唱片本体保留真实的圆形几何，作为世界语法中的唯一有机轮廓。状态不使用大量胶囊 pill，当前项通过反白、位置和规则表达。

## Components

### Buttons

- **Shape:** 零圆角；主动作最小高度 45px，下载/安装动作最小高度 31px。
- **Primary:** 深墨动作带中的哒哒青背景、深墨文字和明确箭头，执行“配置中文”或“恢复原网络”。
- **Hover / Focus:** 悬停切换到深信号青并轻微上移；焦点使用深墨 2px outline 与 3px offset。
- **Quiet:** 官方下载与外部链接使用无填充青色文字动作，不伪装成营销 CTA。

### Chips

状态标记只在需要快速扫描时使用方形标签；文字必须保留“检测中 / 未安装 / 已安装 / 运行中 / 检测失败”等真实状态，禁止用颜色单独表达结果。

### Cards / Containers

软件与 CLI 目录使用带规则的目录单元，零圆角、透明或纸张白背景，依靠分隔线和栏目位置组织。首屏不使用同尺寸卡片作为主要结构。

### Navigation

顶栏是品牌与工作区入口；首屏右侧曲目索引使用反白表达当前任务。哒哒 API 外链作为分段目录保留真实服务入口，悬停只切换青绿色平涂。

### Record Sleeve

左侧唱片区、中央主动作带和右侧曲目列表是页面签名组件。唱片只做工作区的视觉锚点，原始 Logo 始终通过实际品牌资源渲染在顶栏。

### Chinese Setup Drawer

抽屉是安全流程的签名组件：检测、路由确认、旧恢复记录、中文验证和恢复原网络五步始终可见。只有真实成功后才显示成功；恢复失败时保留记录并允许重试。

## Do's and Don'ts

### Do:

- **Do** 使用原始哒哒 Logo 和现有青绿色品牌信号。
- **Do** 让唱片、曲目、反白和闭合集合承担信息层级。
- **Do** 把真实状态、官方下载和网络恢复放在用户几秒内可找到的位置。
- **Do** 让失败原因和恢复动作使用产品自己的简体中文。

### Don't:

- **Don't** 回到白底圆角卡片加大量状态 pill 的泛化 SaaS 结构。
- **Don't** 用新图形替换 Logo，不要让生成的插画成为品牌标记。
- **Don't** 展示假指标、假进度、未实现模块、Coming Soon 或技术说明。
- **Don't** 隐藏网络恢复状态，也不要把中文激活成功与网络恢复成功混为一谈。
