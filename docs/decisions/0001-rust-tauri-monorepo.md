# ADR 0001: Rust And Tauri Monorepo

## Status

Accepted

## Decision

哒哒助手使用 Tauri v2、Vue 3、TypeScript 和 Rust Workspace。桌面客户端和配置服务器均使用 Rust，旧 Go 项目仅作为迁移参考。

## Reasons

- 消除 Go Sidecar 和进程间通信
- 统一构建、签名和发布工具链
- 核心逻辑可以独立于 Tauri 测试
- 安装包小于 Electron 方案
- Vue 提供足够成熟的桌面界面开发能力

## Consequences

- 需要重写现有 Go 系统逻辑
- Rust 平台 API 和代理恢复需要重点测试
- 桌面 UI 仍通过系统 WebView 渲染
- Windows 依赖 WebView2，macOS 依赖系统 WebKit
