# Release Incident Handling

正式发布采用单向推广。允许重试瞬时失败，不允许重写 `v*` 标签、删除最终 Release 或替换公开资产。

## Before GitHub Prerelease

- 预检、CI、用户态构建或 Draft 上传失败时，保留 Draft 和私有 artifact 以便诊断。
- 网络或 runner 瞬时错误可重跑失败 job。
- 代码、版本或资产错误必须通过新补丁版本修复。

## Public Channel Failure

- GitHub 四平台安装失败时不得同步 Gitee。
- Gitee 安装失败时 GitHub 保持 Prerelease，不得晋级 Final。
- `4xx`、SHA-256、格式、架构、Bundle ID、用户安装路径或启动失败不是可回退错误。
- 已公开资产不得替换；修复后发布新补丁版本。

### Gitee Upload Throughput

- 若日志显示已快速连接并收到 `HTTP 100 Continue`，但 `uploaded_bytes` 在 900 秒内仍小于资产大小，属于 runner 到 Gitee 的上传吞吐故障，不是令牌或资产格式错误。
- 正式 Gitee 上传固定使用 macOS runner。不要在同一网络路径上无限增加 Ubuntu runner 重试次数或关闭完整性门禁。
- 若标签工作流已在 Gitee 阶段停止，先合并传输修复，再对原 GitHub Prerelease 运行“修复 Gitee Release 发布”。不得移动标签、替换 GitHub 资产或跳过 Gitee 四平台安装。

## Repository Or Credential Incident

- Gitee Token 或路由构建配置泄露时立即轮换对应值并暂停创建标签。
- 仓库、标签或 Release 疑似被篡改时停止推荐安装命令，核对审计日志和两源资产。
- 未签名发行不具备 Apple/Windows 发布者认证；SHA-256 不能抵御仓库与校验文件同时被控制。

## User Installation

- macOS 安装失败时确认 `$HOME/Applications` 可写且用户拥有目标应用，不要建议使用 `sudo`。
- Windows 安装失败时确认 NSIS 保持 `currentUser`，不要改成机器级安装或全局 ExecutionPolicy。
- 企业管理策略可能拒绝未认证应用；该限制不能通过发布脚本保证绕过。

## User Network Recovery

发布安装脚本只负责应用安装和启动，不修改系统代理。若中文流程后无法恢复网络，让用户保持哒哒助手运行，通过第五步重试“恢复原网络”。不得建议删除恢复记录或先关闭本地代理。
