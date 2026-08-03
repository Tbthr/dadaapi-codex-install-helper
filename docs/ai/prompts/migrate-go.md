# Go To Rust Migration Prompt

需要把旧 Go 实现迁移到哒哒助手 Rust 模块。

Go 参考位置：

```text
<填写旧仓库文件和函数>
```

Rust 目标位置：

```text
<填写 crate 和模块>
```

迁移要求：

1. 将 Go 代码视为行为参考，不做逐行翻译。
2. 提取输入、输出、平台差异、错误和恢复语义。
3. 先创建 fixture 和兼容测试，再实现 Rust 逻辑。
4. 不把 Go 的 PowerShell 字符串直接散落到多个 Rust 模块。
5. 保持 ChatGPT/Codex 双名称兼容。
6. 迁移完成后列出已覆盖和未覆盖的旧行为。
