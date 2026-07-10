# AI Development Guide

AI 工具开始工作前必须读取根目录 `AGENTS.md`。本目录提供可复制的任务提示词，用于统一实现、修复、迁移、审查和发布质量。

使用原则：

- 每个任务先写清目标、非目标和验收条件。
- 要求 AI 先阅读现有代码，不按记忆重新设计。
- 涉及代理和进程控制时，明确要求验证恢复路径。
- 不允许 AI 把测试跳过描述成测试通过。
- 不允许为了让流程成功而吞掉真实错误。

提示词模板：

- `prompts/feature.md`
- `prompts/bugfix.md`
- `prompts/migrate-go.md`
- `prompts/review.md`
- `prompts/release.md`
