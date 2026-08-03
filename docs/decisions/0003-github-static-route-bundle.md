# 0003 - GitHub Static Route Bundle

## Status

Accepted.

## Decision

生产桌面客户端不依赖 SSH 后台、配置服务器或订阅中继。独立 `dadaapi-routes` 仓库通过 GitHub Actions 定时拉取上游订阅，并向 GitHub Raw 发布三个静态文件：

- `manifest.json`
- `routes.sig`
- `routes.enc`

发布端使用 XChaCha20-Poly1305 加密订阅正文，使用 Ed25519 签名清单原始字节，并在清单中使用 SHA-256 绑定密文。v2 路由包使用 `DADAR002`、`dadaapi-routes/v2` AAD 和 `schemaVersion: 2`；客户端严格按“HTTPS 下载 → 清单验签 → 字段与有效期验证 → 密文大小和哈希校验 → AEAD 解密”的顺序处理。

客户端缓存只保存签名清单、签名和密文。远程网络错误、超时或 `5xx` 时，可以回退到仍未过期且能完整重新验证的缓存；任何远程签名、格式、哈希或解密错误都不得触发缓存回退。

## Consequences

- 生产链路不再需要自建配置服务或订阅中继。
- ChatGPT/Codex 用户流量仍固定为本地代理直连选中的海外节点，不经过 GitHub 或哒哒 API 服务器。
- 解密 key 会进入官方开源客户端构建，只能降低公开仓库中节点被直接浏览的风险，不能被视为不可提取秘密。
- 签名私钥和原始上游订阅地址只保存在路由仓库的 GitHub Actions Secrets 中。
- v1 路由包和缓存不再受支持；发布新版桌面客户端前必须先发布 v2 路由包。
