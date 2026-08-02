# Development Standards

## Rust

- 使用 stable Rust 和 2021 edition。
- 错误使用 `thiserror`，应用边界使用 `anyhow::Context`。
- I/O 接口返回 `Result`，不隐藏错误。
- 核心模块避免全局可变状态。
- 跨平台能力以 trait 表达，平台实现放在独立模块。
- 序列化类型使用 `camelCase` 与前端协议保持一致。

## Tauri And Vue

- Tauri Command 只做参数校验和服务调用。
- 长任务使用事件报告进度，不阻塞界面。
- Vue 使用 Composition API 和 TypeScript。
- 状态集中放在 Pinia，平台调用集中放在 `services/`。
- 页面不得直接拼接 shell 命令。
- 用户界面不展示代理密码、订阅全文或内部错误堆栈。
- 前端只展示已确认的真实产品内容，不展示需求说明、技术栈、开发状态或后续规划。
- 不创建占位卡片、`Coming Soon`、空功能入口或用于解释工程进度的页面内容。
- 未实现功能记录在文档或任务系统中，不通过产品页面向开发者说明。
- 桌面端固定使用哒哒 API 浅色品牌系统，不重新引入主题切换或客户端在线更新入口。

## Static Route Bundle

- 路由只通过固定 HTTPS 静态文件分发，客户端不得依赖动态配置 API。
- `manifest.json`、`routes.sig` 和 `routes.enc` 的协议版本、签名、超时、回退和缓存策略必须显式定义并测试。
- Tauri Command 的错误返回稳定的 `code`、`message` 和可选 `details`。

## Testing

- 纯函数和配置转换使用单元测试。
- Windows 路径、AppX 和 macOS plist 使用 fixture 测试。
- 网络请求使用本地 mock server。
- 系统代理测试不得修改开发机器真实代理。
- 发布前在真实 Windows 和 macOS 环境执行冒烟测试。
- 安装脚本的纯解析和回退策略必须通过本地 fixture 测试；正式发布还必须从两端不可变标签下载脚本，在 Intel/Apple Silicon/x64/ARM64 完成真实安装和启动。
- 锁定依赖必须通过 `scripts/check-licenses.sh`，用户可见第三方资产必须同步 `THIRD_PARTY_NOTICES.md`。

## Definition Of Done

- 验收条件已满足。
- 失败路径已处理。
- 格式化、静态检查和测试通过。
- 没有新增敏感信息。
- 相关文档同步更新。
