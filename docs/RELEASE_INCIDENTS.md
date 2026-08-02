# Release Incident Handling

正式发布采用单向推广。允许重试瞬时失败，不允许重写 `v*` 标签、删除最终 Release、替换公开资产或降低签名与校验门禁。

## Before GitHub Prerelease

- 预检、CI、签名构建或 Draft 上传失败时，先保留 Draft 和私有 artifact。
- 网络或服务瞬时错误可从 GitHub Actions 重新运行失败 job；复用 Draft 时工作流会逐文件比对，内容不同会停止。
- 代码、信任常量、版本或资产错误不能在同一标签上修复。创建新的补丁版本并重新走候选验收。

## GitHub Prerelease Is Public

- GitHub 四平台安装失败时不要继续同步 Gitee。
- 瞬时下载或 runner 故障可重试失败 job。
- 脚本、签名或应用缺陷必须发布新补丁版本；不要替换 Prerelease 资产或移动原标签。

## Gitee Final Is Public

- Gitee 安装冒烟失败时 GitHub 必须保持 Prerelease，不能晋级 Final。
- 若只是 Gitee CDN 短时不可用，可重试冒烟；`4xx`、哈希、格式或签名失败不是可回退事件。
- 资产或脚本错误通过新补丁版本修复。已经公开的 Gitee Final 保持原样，发布说明可指向补丁版本。

## GitHub Final Or latest Verification Fails

- GitHub Final API 瞬时失败可重试；工作流会确认四资产名称和大小未变化。
- GitHub `latest` 下载内容、Gitee `releases/latest` API 或两源资产不一致时，停止对外推荐 `latest`，使用明确版本命令定位问题。
- 不手动上传或覆盖同名文件。恢复方式是修复渠道状态或发布新补丁版本。

## Credential Or Signing Incident

- 立即撤销受影响的 Gitee Token、Apple API Key、证书或 Windows PFX，并从 `production-release` Environment 删除。
- 暂停创建新标签。不要通过关闭签名检查维持发布。
- 如果公开信任身份发生合法轮换，先更新 `release/trust-identities.json` 和两个安装脚本，完成新的候选及四平台真实设备验收，再发布补丁版本。
- 检查 Actions 日志和 artifact；日志不得包含证书、密码、路由解密 key 或完整私有路径。

## User Network Recovery

发布安装脚本只负责应用安装和启动，不会修改系统代理。若用户在应用中文流程后无法恢复网络，应让用户保持哒哒助手运行，通过第五步重试“恢复原网络”。不得建议删除恢复记录或先关闭本地代理。
