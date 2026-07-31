# ADR 0002: Embedded Proxy Protocol Implementations

## Status

Accepted

## Decision

哒哒助手的节点数据面在客户端 Rust 进程内实现，不启动 Mihomo、sing-box、Xray 或 Go Sidecar。

- Hysteria2 使用 MIT 许可的 `rsteria2` 库，启用 Salamander、端口跳跃和连接复用。
- VLESS TCP/TLS 使用仓库内的最小协议实现，包含请求头、响应头和双向流。
- Reality/Vision 在没有完成兼容实现和真实互操作测试前保持明确拒绝，不允许退化成普通 TLS 或伪造成功。
- GPL 协议库不得在项目许可证未对齐前引入发布客户端。
- 无明确许可证的 Reality 实现不得作为依赖或复制来源。

## Reasons

- 保持最终安装包只有一个桌面进程和一个 Rust 运行时。
- 避免外部代理进程的启动、更新、权限和崩溃恢复问题。
- 确保节点测速与正式激活使用同一协议实现。
- 避免协议实现的许可证反向决定整个开源项目许可证。

## Consequences

- 当前可用节点为 Hysteria2、VLESS TCP 和 VLESS TLS。
- VLESS WebSocket、gRPC、Reality 和 Vision 节点会在预检阶段被跳过。
- Reality/Vision 完成后必须增加与真实 Xray 服务端的互操作测试，再允许进入节点候选集。
