# 哒哒助手

哒哒助手是哒哒 API 品牌下的开源跨平台 AI 桌面工具。首个版本面向 Windows 和 macOS，提供 ChatGPT/Codex 中文配置、安全路由更新、中文流程内的手动网络恢复和官方软件下载能力。

当前仓库已完成桌面激活核心链路：检测新版 ChatGPT/旧版 Codex、优先从 Gitee 并备用从 GitHub Raw 下载加密路由包、验证 Ed25519 签名和 SHA-256、使用 XChaCha20-Poly1305 解密、筛选和测试海外节点、启动本地代理、安全修改并由用户手动恢复系统网络、写入中文配置、重启应用并验证 Renderer 运行语言。生产激活 Command 已稳定注册，未注入完整构建配置时保持只读模式。

## 技术栈

- Tauri v2
- Vue 3 + TypeScript
- Rust Workspace
- Tokio
- pnpm

## 工程结构

- `apps/desktop`：Tauri 桌面客户端
- `crates`：可测试、可复用的 Rust 核心模块
- `crates/route-bundle`：静态路由包下载、验签、校验、解密和密文缓存
- `tools/route-e2e`：Gitee 优先、GitHub 备用的生产路由真实联调工具
- `docs`：需求、架构、规范、计划和 AI 开发提示词

## 环境要求

- Rust stable，包含 `rustfmt` 和 `clippy`
- Node.js 22+
- pnpm 10+
- macOS Command Line Tools，或 Windows 对应的 MSVC/WebView2 环境

## 开发命令

```bash
pnpm install
pnpm dev
```

完整验证：

```bash
pnpm verify
```

本地打包：

```bash
pnpm --dir apps/desktop tauri build
```

## 安装哒哒助手 v1.0

`v1.0.0` Release 仅用于分发 Windows/macOS 安装包和 `checksums.txt`，客户端不会检查或下载在线更新。Release 必须包含唯一的 `*_x64-setup.exe`、`*_arm64-setup.exe` 和 `*_universal.dmg` 资产；Gitee 发布会将 macOS DMG 包装为 ZIP，安装脚本会自动处理。

Windows（Gitee 脚本入口）：

```powershell
irm https://gitee.com/lyq_power/dadaapi-codex-install-helper/raw/main/scripts/install.ps1 | iex
```

macOS（Gitee 脚本入口）：

```sh
curl -fsSL https://gitee.com/lyq_power/dadaapi-codex-install-helper/raw/main/scripts/install.sh | sh
```

脚本优先从 Gitee 获取校验和和安装包，失败时回退 GitHub；也可直接使用 GitHub 脚本入口：

```powershell
irm https://raw.githubusercontent.com/Tbthr/dadaapi-codex-install-helper/main/scripts/install.ps1 | iex
```

```sh
curl -fsSL https://raw.githubusercontent.com/Tbthr/dadaapi-codex-install-helper/main/scripts/install.sh | sh
```

静态路由真实联调（Gitee 优先、GitHub 备用）：

```bash
cargo run -p route-e2e -- --quick
cargo run -p route-e2e
```

工具默认读取当前用户配置目录下的 `dada-assistant/routes/route-signing-public.pem` 和 `route-encryption-key.bin`。也可通过以下环境变量覆盖文件位置，但不得把密钥写入仓库或命令行参数：

- `DADAAPI_ROUTE_PUBLIC_KEY_FILE`
- `DADAAPI_ROUTE_KEY_FILE`
- `DADAAPI_ROUTE_MANIFEST_URLS`
- `DADAAPI_ROUTE_KEY_ID`

桌面激活运行时通过构建环境注入四项配置：

- `DADAAPI_ROUTE_MANIFEST_URLS`：按优先级排列、逗号分隔且无凭据、无查询参数的 HTTPS `manifest.json` 地址；兼容单地址变量 `DADAAPI_ROUTE_MANIFEST_URL`。
- `DADAAPI_ROUTE_PUBLIC_KEY_PEM`：验证路由清单签名的 Ed25519 公钥 PEM，可使用真实换行或转义的 `\n`。
- `DADAAPI_ROUTE_KEY_B64`：32 字节 XChaCha20-Poly1305 解密 key 的 Base64。它会进入官方客户端，只能用于避免直接浏览，不能视为不可提取秘密。
- `DADAAPI_ROUTE_KEY_ID`：必须与签名清单中的 `keyId` 一致。

四项都未提供时，桌面端只装载只读检测能力；仅缺任一项或内容无效时，应用拒绝启动，避免半配置运行。签名私钥和原始上游订阅地址不得进入客户端构建。

## 当前范围

- 不使用 Go Sidecar
- 不使用卡密、激活码、IP 绑定或次数限制
- 不使用七牛或自建软件下载镜像
- 软件安装包通过用户本地网络从官方地址下载
- 中文订阅由独立仓库定时发布为签名加密路由包并同步到 Gitee，客户端优先访问 Gitee、备用访问 GitHub，在本机验签、解密、解析和筛选
- ChatGPT/Codex 代理流量由客户端直接连接选中的订阅节点，不经过哒哒 API 服务器

## 当前可用能力

- 识别 macOS 新版 ChatGPT 与旧版 Codex
- Windows ChatGPT/Codex 常规安装和 Microsoft Store 路径检测
- 读取 `config.toml` 与全局状态中的语言设置
- 写入前备份原文件，写入失败时恢复现场
- 幂等写入 `localeOverride = "zh-CN"`
- 恢复原语言配置
- 配置完成后停止并重新打开目标桌面应用
- Gitee 优先、GitHub Raw 备用的静态路由下载、Ed25519 验签、SHA-256 校验和 XChaCha20-Poly1305 解密
- 仅保存签名清单、签名和密文的私有原子缓存及严格安全回退
- VLESS TCP/TLS、Hysteria2 本地协议连接
- ChatGPT/OpenAI 多目标节点选优及脱敏结果缓存
- 网络恢复优先的完整激活协调器
- 中文配置与 Renderer `--lang=zh-CN` 双重验证
- 可信 ChatGPT 官方安装包目录与当前系统/架构匹配
- `.part` + ETag/Last-Modified 安全断点续传、取消、重试和 SHA-256 校验
- 持久化下载任务、真实进度事件和官方下载目录打开能力
- 下载完成后通过系统默认图形界面直接打开 DMG 或 EXE 安装包
- 路由配置不可用时仍可检测并由用户手动恢复遗留系统代理
- 中文配置跨文件事务撤销、部分成功结果和恢复状态聚合 Command
- 中文验证成功后在第五步手动恢复原网络，并在失败时保留状态以便重试

详细需求见 [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md)。
