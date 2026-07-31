# Architecture

## Components

```text
Vue Desktop UI
    ↓ Tauri invoke/events
Tauri Command Layer
    ↓
Rust Application Services
    ↓
Discovery | Platform | Proxy | Locale | Route Bundle | Downloader | Diagnostics
                                             | CLI Tools

GitHub Actions Route Publisher
    ↓ HTTPS upstream subscription
XChaCha20-Poly1305 + Ed25519 + SHA-256
    ↓ commits static artifacts
Gitee Raw (primary) / GitHub Raw (fallback): manifest.json | routes.sig | routes.enc
    ↓ HTTPS download, verify, decrypt in memory
Desktop Client

ChatGPT/Codex
    ↓ localhost HTTP CONNECT
Wocao Hub Rust Local Proxy
    ↓ VLESS / Hysteria2
Selected Overseas Node
    ↓
ChatGPT/OpenAI Services
```

## Rules

- `apps/desktop/src` 不包含系统业务逻辑。
- `apps/desktop/src-tauri` 负责依赖装配、Tauri Commands 和事件转发。
- `crates` 不依赖 Tauri，能够独立测试。
- `crates/route-bundle` 负责静态路由包协议，不能依赖 Tauri UI。
- `services/config-server` 和 `services/subscription-relay` 只保留为旧链路兼容测试代码，不进入生产桌面运行时。

## Activation State Machine

```text
Idle → DetectingApp → FetchingProxyConfig → FilteringProxyNodes
→ TestingProxyNodes → SelectingProxyNode → StartingLocalProxy
→ SavingNetworkState → WritingLocale → StoppingDesktopApp
→ LaunchingDesktopApp → Verifying → Succeeded | Failed

PendingManualRecovery → RestoringNetwork → StoppingLocalProxy → Idle
```

节点测速使用各自的临时代理，不修改系统网络。只有正式最优节点的本地代理启动并通过就绪检查后，才进入 `SavingNetworkState` 并创建 `recovery.json`。只有确认网络恢复后才能删除该文件。

- `recovery.json` 使用同目录临时文件替换写入；macOS 文件权限固定为仅当前用户可读写。
- 应用临时代理失败时必须立即尝试回滚可能发生的部分修改；回滚失败时保留 `recovery.json`。
- 激活完成或验证失败后保留 `recovery.json` 和本地代理会话，等待用户点击“恢复原网络”。
- 客户端启动时只检查遗留的 `recovery.json`，不得自动恢复；存在遗留状态时阻止新的激活并展示手动恢复入口。
- 系统代理只允许指向回环地址和非零端口，静态路由包不能直接指定任意系统代理地址。
- macOS 检测到认证代理时停止激活，因为系统接口无法读取原密码，继续修改无法保证完整恢复。

### Proxy Node Selection

- 订阅解析后先按节点名称、地区元数据和出口定位排除香港及国内节点。
- 节点检测必须通过代理请求 ChatGPT/OpenAI 激活依赖地址，普通 Ping 只能作为辅助指标。
- 候选节点复检必须覆盖 ChatGPT 网页、OpenAI 登录域名和 OpenAI API；目标覆盖率、连续成功率优先于延迟。
- 候选节点必须并行检测，单节点先执行最多 3 秒的出口和目标预检，预检失败立即淘汰；通过预检的节点再并行完成稳定性复检。
- 整个节点选优阶段必须设置 15 秒硬截止时间；截止时使用已经完整验证成功的最优节点，没有成功节点则返回真实失败。
- OpenAI API 在未携带凭据时返回 `401` 代表链路已到达服务，`403` 不得作为可用结果。
- 每个候选节点至少记录连接结果、TLS 建连时间、首字节延迟和连续失败次数。
- 选择当前成功且综合延迟最低的节点，并保留次优可用节点作为自动回退。
- 上一次成功节点可以缓存，但每次激活前仍需做快速复检。
- `proxy-cache.json` 只保存节点名称、协议、地区、覆盖率和性能元数据，最多保存两个备用节点；不得保存节点 URI、服务器地址、密码或订阅内容。
- 真实节点选优成功后更新 `proxy-cache.json`；缓存写入失败只降低下次启动性能，不得把本次已验证节点判为失败。
- 代理、订阅凭据和完整节点配置不得写入日志。

### Subscription And Local Route Catalog

- 上游订阅 URL 只存在于独立路由仓库的 GitHub Actions Secrets，客户端和公开仓库均不包含该 URL 或 Token。
- 路由发布器拉取上游时只允许 HTTPS，最多跟随 5 次 HTTPS 重定向，并限制超时和 8MB 响应体。
- 客户端只接受无内嵌凭据、无查询参数、无片段且文件名为 `manifest.json` 的 HTTPS 地址。
- 客户端按“清单原始字节验签 → 清单字段与有效期验证 → 密文大小和 SHA-256 校验 → XChaCha20-Poly1305 解密”顺序处理。
- 订阅内容由客户端在内存中解析；香港、国内和元数据节点在测速前排除。
- 解析后的节点使用完整节点 URI 的 SHA-256 摘要生成本地稳定 Route ID；完整节点 URI 不写入缓存或日志。
- 订阅节点凭据会在客户端运行内存中短暂存在。开源、无登录客户端无法把本机正在使用的凭据视为不可提取秘密。

### Local Proxy Lifecycle

- 具体代理协议实现必须通过 Rust `LocalProxyEngine` 边界接入，最终客户端不得依赖 Mihomo、Go Sidecar 或外部常驻进程。
- 本地代理只允许返回回环地址和非零端口；启动后必须在限定时间内通过就绪检查，之后才能修改系统代理。
- 就绪检查失败时先尝试优雅关闭，关闭失败再强制中止；仍在运行的会话被提前释放时必须同步触发中止信号。
- 用户触发系统代理恢复且确认完成后才能关闭本地代理，避免恢复过程中出现新的断网窗口。
- 如果系统代理恢复失败，协调器保留 `recovery.json` 和当前本地代理会话，不得在系统仍可能指向该回环端口时主动关闭代理。

### Static Route Bundle

- `routes.enc` 格式固定为 8 字节 `WCRTE001`、24 字节 XChaCha nonce 和带认证标签的密文，AAD 固定为 `wocao-hub-routes/v1`。
- `routes.sig` 是对 `manifest.json` 原始字节的 Ed25519 Base64 签名；清单中的 `routeSha256` 和 `routeSize` 绑定 `routes.enc`。
- 清单、签名和密文分别限制为 64KB、1KB 和约 8MB，超限响应在完整读取前拒绝。
- 只有网络错误、超时或 `5xx` 可以使用本地缓存。远程签名、格式、哈希、有效期、`keyId` 或解密错误必须直接失败。
- 缓存使用仅当前用户可访问的原子版本文件，只保存清单、签名和密文；读取缓存时重新执行完整验证。
- GitHub 和 wocao.ai 均不接收 ChatGPT/Codex 的代理流量。

### Direct Node Transport

- 本地代理只接受 HTTP `CONNECT`，将目标主机和端口作为结构化数据交给本地节点协议连接器。
- 节点协议连接器在客户端进程内实现 VLESS 和 Hysteria2，不启动 Go、Mihomo、sing-box 或其他 Sidecar。
- 节点测速与正式激活使用同一协议连接器，禁止用普通 TCP 直连结果冒充节点可用。
- 当前连接器支持 Hysteria2、VLESS TCP 和 VLESS TLS；Reality/Vision 在真实协议实现完成前必须在预检阶段跳过。
- ChatGPT/Codex 流量路径固定为 `应用 → 127.0.0.1 本地代理 → 选中订阅节点 → OpenAI`，不经过 wocao.ai 服务器。

## Locale Configuration Component

语言文件写入可以独立测试，但不能单独宣称激活成功：

```text
WritingLocale → ReadingLocale → Configured | Failed
```

- 新版 macOS 应用显示名和可执行文件为 `ChatGPT`，当前 Bundle Identifier 仍为 `com.openai.codex`。
- 旧版应用继续兼容 `Codex`、`OpenAI Codex` 和对应 Windows 可执行文件名。
- 中文设置同时写入 `~/.codex/config.toml` 的 `desktop.localeOverride` 与全局状态的 `localeOverride`。
- 完整协调器通过 `ChineseEffectVerifier` 注入应用内验证；生产 `RuntimeChineseEffectVerifier` 必须同时确认配置值和目标 App Renderer 的 `--lang=zh-CN` 运行参数，文件配置检查不能单独判定成功。
- 激活 Tauri Command 始终保持稳定注册，但只有构建同时注入路由清单地址、验签公钥、解密 key 和 `keyId` 时才装载激活运行时并在界面展示“一键中文”；未配置构建保持只读检测能力，Command 返回稳定的不可用错误。
- 激活进度由核心协调器的真实状态转换产生，经 Tauri 事件转发到前端；前端不得使用计时器伪造阶段或成功状态。
- 首次修改前创建相邻备份；原文件不存在时记录缺失状态，以便恢复时删除由 Wocao Hub 创建的文件。
- 配置更新使用临时文件和替换写入，保留原文件权限，不覆盖无效 TOML 或 JSON。
- Tauri Command 必须重新检测所选应用，不接受前端提供的任意可执行文件路径。

## Platform Boundary

平台 crate 暴露统一 trait，Windows 和 macOS 分别实现：

- 应用发现
- 进程停止和启动
- 系统代理保存、应用和恢复
- 应用数据目录
- 安装器启动

下载任务只接受可信产品目录中的固定官方链接，目标路径由后端计算且不写入任务状态文件。下载完成后进入 `Ready`，用户触发安装时由系统默认图形界面直接打开 DMG 或 EXE；前端不能传入任意 URL、本地路径、命令或参数。

CLI 安装只接受共享类型中的 `CodexCli` 与 `ClaudeCodeCli`，核心服务将其映射到固定 npm 包并使用官方 npm registry。Node/npm 缺失时返回稳定错误，由界面引导用户先安装可信目录中的 Node.js LTS。Windows 子进程必须隐藏控制台窗口。

## Local Data

```text
settings.json       用户设置
proxy-cache.json    上一次验证成功的代理配置
route-bundles/      已签名且仍为密文的路由缓存
recovery.json       未完成的网络恢复记录
downloads.json      下载任务状态
logs/app.log        脱敏日志
diagnostics-exports/ 仅包含白名单摘要和二次脱敏日志的诊断包
```

## Public Route Artifacts

```text
https://gitee.com/lyq_power/dadaapi-routes/raw/main/public/manifest.json
https://gitee.com/lyq_power/dadaapi-routes/raw/main/public/routes.sig
https://gitee.com/lyq_power/dadaapi-routes/raw/main/public/routes.enc

https://raw.githubusercontent.com/Tbthr/dadaapi-routes/main/public/manifest.json
https://raw.githubusercontent.com/Tbthr/dadaapi-routes/main/public/routes.sig
https://raw.githubusercontent.com/Tbthr/dadaapi-routes/main/public/routes.enc
```

公开文件不得包含上游订阅地址、Token、签名私钥或明文节点。GitHub Actions 每四小时更新一次并同步到 Gitee，默认有效期为 72 小时。客户端仅在 Gitee 网络错误、超时或 `5xx` 时切换 GitHub；签名、格式、哈希或解密错误必须直接失败。

## Route End-To-End Harness

- `tools/route-e2e` 默认按 Gitee、GitHub 顺序连接正式清单，并从当前用户私有配置文件读取验签公钥和解密 key。
- `--quick` 使用 4 个候选、2 次尝试和较短超时；默认模式使用 8 个候选、3 次尝试和生产超时。
- 工具必须完成下载、验签、有效期验证、SHA-256 校验、解密、订阅解析、香港及国内排除和真实 ChatGPT/OpenAI 多目标测速。
- 输出只包含节点名称、协议、出口地区、覆盖率和延迟等脱敏指标，不输出路由 URI、密码、订阅正文或解密 key。
- 旧 `tools/local-e2e` 仅用于回归历史配置服务器和固定证书中继链路，不代表生产架构。

### Desktop Build Configuration

- `DADAAPI_ROUTE_MANIFEST_URLS`：按优先级排列、逗号分隔的固定 HTTPS `manifest.json` 地址；兼容单地址变量 `DADAAPI_ROUTE_MANIFEST_URL`。
- `DADAAPI_ROUTE_PUBLIC_KEY_PEM`：Ed25519 公钥 PEM。
- `DADAAPI_ROUTE_KEY_B64`：32 字节 XChaCha20-Poly1305 key 的 Base64。
- `DADAAPI_ROUTE_KEY_ID`：预期密钥标识。
- 旧的 `WOCAO_HUB_ROUTE_*` 构建变量不再读取。
- 四项全部缺失时只装载只读桌面能力；仅缺一项或任何一项格式错误时启动失败。
- 公钥、清单 URL 和 `keyId` 是公开配置。解密 key 会进入官方客户端，只能作为混淆手段；签名私钥和原始上游订阅地址不得进入客户端构建。

### Desktop Updates

- 客户端通过 Tauri Updater 优先检查 Gitee Raw 中的 `release/latest.json`，失败后检查 GitHub Releases 中的 `latest.json`，不依赖自建版本服务器。
- 应用启动后后台检查一次；发现新版本后只提示，不自动安装或强制重启。
- 更新包下载完成并通过 Tauri 更新签名验证后，由用户确认重启。
- 更新签名私钥与密码只存在于 GitHub Actions Secrets 和发布者私有环境；客户端仅内置更新公钥。
- GitHub Actions 对 macOS Intel/Apple Silicon 与 Windows x64/ARM64 分别构建，并将同一批更新资产同步到 Gitee Release。
