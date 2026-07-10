# Wocao Hub Agent Guidance

本文件适用于整个仓库，是 Codex、Claude 和其他 AI 编程工具的长期开发约束。

## Source Of Truth

开始任务前按需阅读：

1. `docs/REQUIREMENTS.md`
2. `docs/ARCHITECTURE.md`
3. `docs/DEVELOPMENT_STANDARDS.md`
4. `docs/SECURITY.md`
5. `docs/ROADMAP.md`

如果实现与文档冲突，先更新设计并说明原因，不要默默改变产品范围。

## Product Invariants

- 最终客户端是 Tauri v2 + Rust，不包含 Go 二进制或 Sidecar。
- 必须同时兼容新版 ChatGPT 和旧版 Codex。
- 用户不需要卡密、登录、IP 绑定或使用次数验证。
- 代理配置从 GitHub Raw 静态路由包获取，必须验签、校验、解密并仅缓存密文。
- 软件安装包必须走用户本地网络和官方地址，不接入软件下载镜像。
- 修改系统代理前必须持久化恢复状态。
- 成功、失败、崩溃和强制退出都必须尽力恢复用户原网络配置。
- 不得把订阅地址、代理密码、签名私钥或生产凭据提交到仓库。

## Architecture Boundaries

- Vue 组件只负责展示、输入和调用 Tauri Commands。
- Tauri Commands 必须薄，不包含核心业务流程。
- 核心逻辑放入 `crates/`，不得依赖 Vue 或 Tauri UI。
- 平台差异通过 trait 和 `cfg(target_os)` 隔离。
- 远程服务和客户端共享的 JSON 类型放在 `shared-types`。
- 网络、文件和进程操作必须返回结构化错误，不允许伪造成功状态。
- 不要为尚未出现的复杂度提前创建抽象层。

## Frontend Content Rule

- 前端页面只实现已经确认的产品界面和真实交互，不得把页面当作需求说明、开发文档或项目看板。
- 禁止在产品界面添加占位模块、功能规划、技术栈、开发状态、后续里程碑、`Coming Soon`、`TODO` 等非产品内容。
- 尚未确认或尚未实现的功能应留在 `docs/`、Issue 或代码任务中，不得为了填满页面而提前展示。
- 没有明确 UI 需求时，保持现有产品界面不变；新工程可以保留空的应用挂载根节点，等待正式设计。

## Implementation Rules

- Rust 标识符和代码注释使用英文；用户界面和用户错误信息使用简体中文。
- 核心模块避免 `unwrap()`、`expect()`、`panic!()`；入口启动失败可以带上下文终止。
- 异步 I/O 使用 Tokio，不在异步任务中执行长时间阻塞操作。
- 配置文件修改必须保持幂等，多次运行结果一致。
- 下载必须支持取消和真实进度，不把 HEAD 成功等同于完整下载成功。
- 日志不得记录完整代理密码、订阅内容、Token 或隐私路径。
- Windows 和 macOS 行为必须分别测试，不能用一个平台的路径规则推测另一个平台。

## Required Verification

提交代码前至少执行：

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm --dir apps/desktop check
pnpm --dir apps/desktop build
```

涉及系统代理、进程控制或配置写入时，必须增加对应单元测试或 fixture。

## Git Scope

- 不提交 `.env`、证书、签名私钥、构建产物、安装包和生产配置。
- 不修改与当前任务无关的文件。
- 不使用破坏性 Git 操作，除非用户明确授权。
- 提交信息使用简短英文祈使句，例如 `Add signed config cache`。
