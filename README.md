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

## 安装发布版本

用户态 Release 仅分发 Windows/macOS 安装包和 `checksums.txt`，客户端不会检查或下载在线更新。GitHub 与 Gitee 必须各自包含且仅包含 `Dada-Assistant_<version>_x64-setup.exe`、`Dada-Assistant_<version>_arm64-setup.exe`、`Dada-Assistant_<version>_universal.dmg` 和 `checksums.txt`。没有 Release 时，安装命令会返回失败，不会伪造成功。

macOS 应用使用 ad-hoc 签名并安装到当前用户的 `~/Applications/哒哒助手.app`；首次打开时 macOS 可能显示未验证开发者提示，需要用户在系统提示中确认。Windows 安装器不含 Authenticode，使用 `currentUser` 模式；首次运行时 Windows 可能显示 SmartScreen 提示，需要用户确认运行。两端安装均不调用 `sudo`、不请求 UAC 提升，也不清除 quarantine、解除下载区块或修改系统全局执行策略。SHA-256 用于确认下载资产与 Release 清单一致，但不等同于 Apple 或 Windows 的发布者认证。

生产 Gitee 镜像为 `lyq_power/dadaapi-codex-install-helper`。GitHub Actions 使用仓库级 `GITEE_TOKEN` Secret，以及 `production-release` Environment 中的 `GITEE_REPOSITORY=lyq_power/dadaapi-codex-install-helper` 和 `GITEE_USERNAME=lyq_power` 变量，同步 `main`、`v1.0.0` 标签与四个正式 Release 资产。Microsoft Store 短期下载元数据由定时工作流写入 Gitee Final Release 的说明字段，不创建额外分支或 Release 资产。令牌不写入仓库、本地配置或安装脚本，普通 PR 工作流也不引用它。

下列入口脚本固定来自不可变的 `v1.0.0` 标签；脚本默认安装两端最新正式版本。Windows 命令使用系统 `curl.exe` 限制 HTTPS 和重定向，将脚本完整下载到当前用户临时目录，确认真实退出码并按严格 UTF-8 读取到内存后执行，最后清理：

Windows（Gitee 默认入口，严格 Gitee 优先）：

```powershell
$u = "https://gitee.com/lyq_power/dadaapi-codex-install-helper/raw/v1.0.0/scripts/install.ps1"
$d = Join-Path ([IO.Path]::GetTempPath()) ("DadaBootstrap-" + [Guid]::NewGuid().ToString("N"))
$p = Join-Path $d "install.ps1"
try {
  [void](New-Item -ItemType Directory -Path $d)
  & curl.exe --fail --silent --show-error --location --max-redirs 5 --proto "=https" --proto-redir "=https" --tlsv1.2 --connect-timeout 10 --max-time 60 --max-filesize 1048576 --output $p $u
  $downloadExit = $LASTEXITCODE
  if ($downloadExit -ne 0 -or -not (Test-Path -LiteralPath $p)) { throw "安装脚本下载失败，curl.exe 退出代码：$downloadExit" }
  $scriptText = [IO.File]::ReadAllText($p, [Text.UTF8Encoding]::new($false, $true))
  if ([string]::IsNullOrWhiteSpace($scriptText)) { throw "安装脚本为空。" }
  & ([ScriptBlock]::Create($scriptText))
} finally { Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue }
```

macOS（Gitee 默认入口，严格 Gitee 优先）：

```sh
umask 077
installer_file=$(mktemp "${TMPDIR:-/tmp}/dada-assistant-bootstrap.XXXXXX") || exit 1
trap 'rm -f "$installer_file"' EXIT HUP INT TERM
curl -fsSL --proto '=https' --proto-redir '=https' --max-redirs 5 --connect-timeout 10 --max-time 60 \
  --max-filesize 1048576 \
  https://gitee.com/lyq_power/dadaapi-codex-install-helper/raw/v1.0.0/scripts/install.sh \
  -o "$installer_file" || exit $?
/bin/sh "$installer_file"
```

仅当 Gitee 发生网络错误、超时或 `5xx` 时，默认命令才回退 GitHub。`4xx`、校验和格式、重复资产、版本或 SHA-256 错误会直接停止。GitHub 明确备用入口如下：

```powershell
$u = "https://raw.githubusercontent.com/Tbthr/dadaapi-codex-install-helper/v1.0.0/scripts/install.ps1"
$d = Join-Path ([IO.Path]::GetTempPath()) ("DadaBootstrap-" + [Guid]::NewGuid().ToString("N"))
$p = Join-Path $d "install.ps1"
$previousSource = [Environment]::GetEnvironmentVariable("DADA_ASSISTANT_INSTALL_SOURCE", "Process")
try {
  [void](New-Item -ItemType Directory -Path $d)
  & curl.exe --fail --silent --show-error --location --max-redirs 5 --proto "=https" --proto-redir "=https" --tlsv1.2 --connect-timeout 10 --max-time 60 --max-filesize 1048576 --output $p $u
  $downloadExit = $LASTEXITCODE
  if ($downloadExit -ne 0 -or -not (Test-Path -LiteralPath $p)) { throw "安装脚本下载失败，curl.exe 退出代码：$downloadExit" }
  $scriptText = [IO.File]::ReadAllText($p, [Text.UTF8Encoding]::new($false, $true))
  if ([string]::IsNullOrWhiteSpace($scriptText)) { throw "安装脚本为空。" }
  $env:DADA_ASSISTANT_INSTALL_SOURCE = "github"
  & ([ScriptBlock]::Create($scriptText))
} finally {
  [Environment]::SetEnvironmentVariable("DADA_ASSISTANT_INSTALL_SOURCE", $previousSource, "Process")
  Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue
}
```

```sh
umask 077
installer_file=$(mktemp "${TMPDIR:-/tmp}/dada-assistant-bootstrap.XXXXXX") || exit 1
trap 'rm -f "$installer_file"' EXIT HUP INT TERM
curl -fsSL --proto '=https' --proto-redir '=https' --max-redirs 5 --connect-timeout 10 --max-time 60 \
  --max-filesize 1048576 \
  https://raw.githubusercontent.com/Tbthr/dadaapi-codex-install-helper/v1.0.0/scripts/install.sh \
  -o "$installer_file" || exit $?
DADA_ASSISTANT_INSTALL_SOURCE=github /bin/sh "$installer_file"
```

安装器稳定参数为 `DADA_ASSISTANT_INSTALL_VERSION=latest|vN.N.N` 与 `DADA_ASSISTANT_INSTALL_SOURCE=auto|gitee|github`。例如在执行下载后的脚本前设置 `DADA_ASSISTANT_INSTALL_VERSION=v1.0.0`，即可固定安装当前正式版本 `v1.0.0`。当前 GitHub 与 Gitee 均只保留 `main`、单个根提交、`v1.0.0` 标签和 `v1.0.0` Final Release；Release 页面只展示这一个正式版本。脚本只安装并启动哒哒助手，不写入中文配置或系统代理。

## 配置中文

1. 启动 ChatGPT 或旧版 Codex，再打开哒哒助手。
2. 在软件页点击“配置中文”，等待前四步完成并确认应用以 `zh-CN` 重启。
3. 中文验证成功后，本地临时代理会继续运行。需要恢复原网络时，在第五步点击“恢复原网络”。
4. 恢复失败时不要手动清空系统代理；保留哒哒助手运行并在第五步重试。恢复成功后恢复记录会删除，本地代理随之关闭。

首次点击“配置中文”没有遗留恢复记录时，不会修改现有系统代理；只有中文流程已保存恢复记录后，才会在流程中恢复该记录。

## 发布、镜像与 E2E

- 已配置的生产 Gitee 仓库是 [lyq_power/dadaapi-codex-install-helper](https://gitee.com/lyq_power/dadaapi-codex-install-helper)。向 GitHub `main` 推送后会自动运行“同步哒哒助手 v1.0 代码至 Gitee”。需要手动重试时，在 GitHub Actions 中运行该工作流；目标始终来自受保护的 `GITEE_REPOSITORY` Actions Variable。
- 可选地在正式发布前运行“生成私有用户态候选包”，输入当时受保护 `main` 的完整提交 SHA。候选包只保存在 7 天 Actions artifact 中，不创建公开 Release。
- Intel Mac、Apple Silicon Mac、Windows x64 与 Windows ARM64 的远程安装和启动由标签发布流水线强制执行；人工中文结果和网络恢复验收可按 [发布 Runbook](docs/RELEASE_RUNBOOK.md) 作为额外检查。
- 当前 `main` 提交上的 `v*` 标签会自动执行：完整 CI -> 共享用户态构建 -> GitHub Draft -> GitHub Prerelease -> GitHub 四平台版本化命令安装 -> Gitee Final -> Gitee 四平台安装 -> GitHub Final -> 两端 `latest` 校验。任何失败都会阻止后续渠道。
- `production-release` Environment 只保存 Gitee 令牌和路由构建配置。Apple、Windows 平台证书、时间戳和公开发布者身份均不再需要。
- 正式 Release 和标签均不可覆盖。发布后故障必须增加补丁版本，不能重写标签或替换资产。处理步骤见 [发布故障手册](docs/RELEASE_INCIDENTS.md)。
- Windows x64 中文全链路测试由“Windows locale E2E”在相关 Pull Request 上执行，也可在 Actions 中手动运行。失败时下载 `windows-locale-e2e-diagnostics` 查看截图和 WebDriver 日志。Windows ARM64 继续由“Desktop package smoke”和“哒哒助手 v1.0 发布冒烟”覆盖安装及启动。

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