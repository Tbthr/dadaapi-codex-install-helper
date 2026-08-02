# Production Release Runbook

本文档用于首发 `v1.0.0` 及后续补丁版本。任何步骤失败都不得跳过门禁、重写标签或替换已经公开的资产。

## 1. One-Time Repository Governance

在 GitHub 中完成并复核：

- `main` 要求“必需 CI / 完整质量门禁”、macOS 安装脚本契约和两项 Windows 安装脚本契约，通过后才允许合并。
- `main` 至少需要一次非作者 PR 审核；`CODEOWNERS` 为 `.github/workflows/`、`scripts/` 和 `release/` 指定责任人。
- `v*` 标签规则禁止删除、更新和 force push，只允许维护者创建。
- 在 Gitee 仓库设置中为 `v*` 启用等价的保护标签规则，禁止删除和更新，并保存设置截图或审计记录。当前发布自动化只会拒绝主动覆盖已有标签，不能替代服务端标签保护；未确认该规则前不得创建首发标签。
- 启用 Dependabot security updates、Secret Scanning 和 Push Protection。
- 创建无审批保护规则的 `production-release` Environment。不要让普通 PR job 引用该 Environment。

## 2. Configure Public Trust Identities

从实际生产证书读取并交叉复核以下公开值，不得凭名称猜测：

- Apple Developer ID 证书的 10 位 Team ID。
- macOS Bundle ID，必须保持 `com.dadaapi.assistant`。
- Windows 代码签名证书完整 `Subject` 字符串，包含字段顺序和空格。

将实际值同时写入：

- `release/trust-identities.json`
- `scripts/install.sh` 的 `expected_apple_team_id`
- `scripts/install.ps1` 的 `ExpectedWindowsPublisherSubject`

保留任意 `SET_BEFORE_V1_0_0` 时，候选构建、标签发布和用户安装命令都会 fail closed。

## 3. Configure Production Environment

在 `production-release` Environment 设置：

Secrets:

- `GITEE_TOKEN`
- `DADAAPI_ROUTE_PUBLIC_KEY_PEM`
- `DADAAPI_ROUTE_KEY_B64`
- `APPLE_CERTIFICATE`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_API_KEY`
- `APPLE_API_ISSUER`
- `APPLE_API_KEY_BASE64`
- `WINDOWS_CERTIFICATE`
- `WINDOWS_CERTIFICATE_PASSWORD`

Variables:

- `GITEE_REPOSITORY=lyq_power/dadaapi-codex-install-helper`
- `GITEE_USERNAME=lyq_power`
- `WINDOWS_TIMESTAMP_URL`，必须是不含凭据的 HTTPS RFC 3161 时间戳地址
- `RELEASE_ACCEPTANCE_RECORD_JSON`，仅在真实设备验收完成后设置

迁移 `GITEE_TOKEN` 后删除同名 Repository Secret，避免扩大普通工作流可见范围。Actions 不允许读取已有 Secret 明文，因此迁移必须由持有者重新录入。

## 4. Build Private Candidate

1. 确认目标提交已经合并到受保护 `main`，所有必需 CI 为绿。
2. 确认 GitHub 与 Gitee 的 `v*` 服务端保护规则都已启用并留存审计记录。
3. 记录完整 40 位提交 SHA。
4. 手动运行“生成私有签名候选包”，输入该 SHA。
5. 工作流会拒绝非当前 `main` 的提交，使用共享生产签名构建，并生成 7 天有效的三个私有安装包与 `candidate-manifest`。
6. 不要从候选流程创建标签或公开 Release。

## 5. Real-Device Acceptance

在相互隔离的 Intel Mac、Apple Silicon Mac、Windows x64 和 Windows ARM64 上分别使用候选包验证：

- 安装成功，旧版本替换和失败清理符合预期。
- 应用可启动，平台签名、公证或 Authenticode 状态有效。
- 新版 ChatGPT 中文设置得到真实中文结果。
- 旧版 Codex 中文设置得到真实中文结果。
- 中文完成后代理仍保持，只有用户点击第五步才恢复网络。
- 手动恢复先恢复原网络配置，再关闭本地代理；失败时恢复记录仍存在且可重试。

不得在日常开发设备上为验收修改系统代理。保留设备、系统版本、候选 workflow run URL、测试时间和验收人记录。

验收全部通过后，在 `production-release` Environment 写入以下结构；`candidateRunId` 必须指向同一提交的成功候选工作流：

```json
{
  "schemaVersion": 1,
  "version": "v1.0.0",
  "commit": "40-character-commit-sha",
  "candidateRunId": 123456789,
  "acceptedBy": "maintainer-name",
  "acceptedAt": "2026-08-02T12:00:00Z",
  "devices": {
    "macosIntel": {
      "install": true,
      "launch": true,
      "chatgptChinese": true,
      "legacyCodexChinese": true,
      "manualNetworkRecovery": true
    },
    "macosAppleSilicon": {
      "install": true,
      "launch": true,
      "chatgptChinese": true,
      "legacyCodexChinese": true,
      "manualNetworkRecovery": true
    },
    "windowsX64": {
      "install": true,
      "launch": true,
      "chatgptChinese": true,
      "legacyCodexChinese": true,
      "manualNetworkRecovery": true
    },
    "windowsArm64": {
      "install": true,
      "launch": true,
      "chatgptChinese": true,
      "legacyCodexChinese": true,
      "manualNetworkRecovery": true
    }
  }
}
```

## 6. Create The Release Tag

在验收提交仍是当前 `main` 时创建 GitHub 可验证的签名注释标签：

```bash
git fetch origin main --tags
git switch main
git pull --ff-only origin main
git tag -s v1.0.0 -m "Release v1.0.0"
git verify-tag v1.0.0
git push origin refs/tags/v1.0.0
```

禁止创建 lightweight tag，禁止在验收后修改提交，禁止 force push 标签。

## 7. Observe Automatic Promotion

发布工作流必须按以下顺序全部通过：

1. 标签签名、版本、生产配置、候选 run 和真实设备验收记录。
2. Rust、前端、安装脚本和许可证完整检查。
3. 三平台共享签名构建与证书身份验证。
4. 创建四资产 GitHub Draft，再公开为 Prerelease。
5. GitHub 标签脚本在 Intel/Apple Silicon/x64/ARM64 完成安装和启动。
6. 同步相同标签和逐字节相同的四资产，公开 Gitee Final。
7. Gitee 标签脚本在四个平台完成安装和启动。
8. GitHub 转为 Final。
9. GitHub `latest` 下载端点与 Gitee `releases/latest` API 均指向本次标签，三个安装包和 `checksums.txt` 逐字节一致。

最终 Release 必须恰好包含三个安装包和 `checksums.txt`。安装脚本固定标签内容必须与仓库标签内容一致。

## 8. Final Checks

- 从 README 分别运行 Gitee 默认命令和 GitHub 备用命令。
- 确认缺失版本返回非零状态，不产生“安装成功”输出。
- 确认 `latest` 指向本次标签。
- 确认最终 Release 页面没有 updater JSON、额外压缩包或未签名资产。
- 记录完整 workflow URL 和四平台成功结果。

发生故障时转到 `docs/RELEASE_INCIDENTS.md`。
