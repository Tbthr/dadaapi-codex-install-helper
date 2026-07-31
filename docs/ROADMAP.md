# Development Roadmap

## M0 - Development Environment

- Rust Workspace
- Tauri v2 + Vue 3 桌面壳
- Axum 配置服务壳
- 统一依赖、格式化、测试和 AI 开发规范
- macOS 本地构建验证

## M1 - Desktop Detection And Locale

- [x] 从旧 Go 项目提取 Windows/macOS 行为与测试样例
- [x] 新版 ChatGPT/旧版 Codex 检测
- [x] TOML 与全局状态中文配置幂等写入
- [x] 原配置备份与恢复
- [x] 应用停止、启动和配置状态验证
- [x] macOS 真实应用只读联调
- [x] Windows x64/ARM64 核心模块交叉编译检查
- [ ] Windows 真实机器冒烟测试

## M2 - Proxy Configuration And Recovery

- [x] 本地测试订阅解析及香港/国内节点排除
- [x] ChatGPT/OpenAI 多目标稳定性选优
- [x] 已选节点与两个备用节点的脱敏元数据缓存
- [x] `recovery.json` 原子持久化与失败保留
- [x] Tauri 启动阶段检测遗留网络状态并提供手动恢复
- [x] Windows/macOS 系统代理保存、应用和恢复实现
- [x] 纯 Rust 本地代理生命周期接口、就绪检查与失败清理
- [x] 本地 Rust HTTP CONNECT 监听与双向转发
- [x] 本地 HTTP CONNECT 代理与节点连接器边界
- [x] 客户端 Ed25519 配置签名验证与原子缓存
- [x] 配置服务 Ed25519 签名发布与不可用错误边界
- [x] 安全上游订阅拉取、匿名本地 Route ID 与内存路由目录
- [x] 固定证书 TLS 订阅中继与客户端拉取
- [x] 客户端 VLESS TCP/TLS 基础连接器
- [ ] 客户端 VLESS Reality/Vision 连接器
- [x] 客户端 Hysteria2 节点连接器（Salamander、端口跳跃、连接复用）
- [x] 客户端本地节点预检、临时代理测速和 ChatGPT/OpenAI 综合选优
- [x] 完整激活协调器、失败清理顺序和网络恢复记录保留
- [x] 配置状态与 Renderer `--lang=zh-CN` 双重中文效果验证
- [x] Tauri 生产激活运行时工厂与构建配置校验
- [x] 内存密钥本地端到端服务链路与真实节点筛选工具
- [x] 激活 Tauri Command、真实进度事件和构建配置可用性控制
- [x] 客户端真实节点严格筛选联调
- [x] 独立 GitHub 路由仓库、定时加密发布和 Ed25519 清单签名
- [x] 客户端静态路由下载、验签、SHA-256 校验、解密和私有密文缓存
- [x] 桌面生产运行时切换到 GitHub 静态路由包
- [x] GitHub 生产路由快速与严格节点筛选联调
- [ ] 客户端真实节点完整激活联调
- [ ] Windows/macOS 真实机器系统代理恢复冒烟测试
- [ ] 崩溃恢复测试

## M3 - Official Download Center

- [x] 可信 ChatGPT 官方产品目录
- [x] Claude Desktop、CC Switch、Node.js LTS 动态官方下载目录
- [x] Codex CLI、Claude Code CLI 检测与官方 npm 安装
- [x] Windows/macOS 与 x64/ARM64 主机识别
- [x] 下载进度、取消、重试和 ETag/Last-Modified 安全断点续传
- [x] 下载任务持久化与 Tauri 进度事件
- [x] 完成任务打开所在目录与打开官方下载页
- [x] 下载完成后使用系统默认方式直接打开安装包
- [x] 可信官方下载链接获取
- [x] 下载中心与 CLI 安装前端交互

## M4 - Desktop Productization

- [x] 哒哒助手浅色品牌界面、完整桌面导航和状态反馈
- [x] 中文配置事务恢复与部分成功结果
- [x] 路由配置不可用时的遗留网络手动恢复
- [x] 私有滚动日志、双重脱敏诊断摘要与 ZIP 导出后端
- [x] 修复工具和诊断导出前端交互
- [ ] Windows/macOS 签名与安装包
- [x] 移除客户端在线更新检查、签名下载与重启安装流程
- [x] Windows/macOS 多架构 GitHub Actions 发布工作流
- [ ] 开源许可证与贡献指南确认
