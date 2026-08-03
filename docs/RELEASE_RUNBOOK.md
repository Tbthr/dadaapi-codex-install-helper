# User-Scoped Release Runbook

本文档用于正式 Release `v1.0.10` 及后续补丁版本。`v1.0.0` 至 `v1.0.9` 仅保留不可变标签；发行包不使用 Apple Developer ID、公证或 Windows Authenticode，安装命令通过用户目录安装实现无管理员密码安装，系统安全提示由用户确认。

## 1. Repository Governance

- `main` 必须通过完整质量门禁以及 macOS、Windows x64、Windows ARM64 安装脚本契约。
- 单人仓库允许零批准合并，但仍必须走 PR；管理员同样不得绕过必需 CI。
- GitHub 与 Gitee 的 `v*` 标签禁止更新和删除。
- Secret Scanning、Push Protection 和 Dependabot security updates 保持启用。

## 2. Production Environment

在 `production-release` Environment 设置：

Secrets:

- `GITEE_TOKEN`
- `DADAAPI_ROUTE_PUBLIC_KEY_PEM`
- `DADAAPI_ROUTE_KEY_B64`

Variables:

- `GITEE_REPOSITORY=lyq_power/dadaapi-codex-install-helper`
- `GITEE_USERNAME=lyq_power`

不需要 Apple 证书、Apple API Key、Windows PFX、时间戳服务、Team ID、Publisher Subject 或候选验收 JSON。迁移 `GITEE_TOKEN` 后删除同名 Repository Secret。

## 3. Optional Private Candidate

需要提前查看产物时，在受保护 `main` 上运行“生成私有用户态候选包”，输入当前 `main` 的完整提交 SHA。工作流生成三个 7 天有效的 Actions artifact 和候选清单，不创建标签或公开 Release。

候选检查重点：

- macOS Intel 和 Apple Silicon 均安装到 `~/Applications/哒哒助手.app`，不要求密码，可启动且保留系统 quarantine 状态。
- Windows x64 和 ARM64 均安装到当前用户目录，不触发管理员提升，可启动。
- 新版 ChatGPT、旧版 Codex、中文结果和手动网络恢复正常。

候选检查是可选的人工补充；标签发布仍会执行四平台远程安装冒烟。

## 4. Create The Release Tag

确认目标提交仍是当前 `main`，创建不可变标签：

```bash
git fetch origin main --tags
git switch main
git pull --ff-only origin main
git tag -a v1.0.10 -m "Release v1.0.10"
git push origin refs/tags/v1.0.10
```

不得移动、删除或重新创建任何已推送标签。发布失败时修复根因、统一增加补丁版本，并从新的当前 `main` 创建新标签。

## 5. Automatic Promotion

发布工作流必须依次通过：

1. 标签、版本、当前 `main`、Gitee 和路由构建配置校验。
2. Rust、前端、安装脚本和许可证完整检查。
3. macOS Universal ad-hoc DMG、Windows x64/ARM64 currentUser 未签名安装器构建。
4. GitHub Draft 创建并转为 Prerelease。
5. GitHub 标签脚本在 Intel、Apple Silicon、Windows x64 和 Windows ARM64 安装并启动。
6. Gitee Final 通过 macOS runner 同步后，在相同四平台再次安装并启动。
7. GitHub 转为 Final，并验证两端 `latest` 与四个资产逐字节一致。
8. 两源验证通过后删除旧 GitHub/Gitee Release 记录，仅保留当前 Final；所有历史 `v*` 标签继续保留且不得改写或删除。

任何步骤失败都不得继续后续渠道、移动标签或替换已经公开的资产。

若 GitHub Prerelease 和四平台安装已通过，但 Gitee 上传因 runner 网络吞吐失败，修复代码合并到 `main` 后运行“修复 Gitee Release 发布”，输入原不可变标签。该工作流只接受仍为 GitHub Prerelease 且标签提交仍在 `main` 历史中的版本，并重新执行 Gitee 资产验真、四平台 Gitee 安装、GitHub Final 和两源 latest 验证。

如需清理发布前已存在的旧 Release 记录，在 Actions 手动运行“仅保留最新 Release”并输入要保留的 Final 标签。工作流会先验证标签属于 `main`、两端均为 Final 且四个正式资产齐全，再删除其他 Release 记录；它不会调用标签删除接口。

## 6. Final Checks

- macOS 安装路径只能是 `~/Applications/哒哒助手.app`，安装和启动过程不出现密码请求。
- Windows 安装路径必须位于当前用户目录，不出现 UAC 提升。
- Gitee 默认命令和 GitHub 备用命令均可在新系统运行。
- 缺失版本返回非零状态，不输出伪成功。
- 最终 Release 恰好包含三个安装包和 `checksums.txt`。
- README 和 Release 说明必须明确安装包未经过 Apple/Windows 发布者认证。

发生故障时转到 `docs/RELEASE_INCIDENTS.md`。
