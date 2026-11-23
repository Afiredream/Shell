# CODEBUDDY.md

核心约定见 [AGENTS.md](AGENTS.md)。核心要求：

- **中文命名**：目录、文件、类型、函数、变量、测试名、Makefile 目标、提交信息；例外仅 Rust 关键字、标准库/第三方库 API、C 符号、外部命令、`main`、Cargo 包名。
- **许可证**：AGPL-3.0-or-later，不改。
- **依赖**：仅 MIT/Apache-2.0/BSD 等宽松开源许可；不引入专有/反开源组件；优先用开源库替代手写通用逻辑。
- **不做 OOP**：数组 `()`、对象 `{}` 只是数据。
- **错误**用 `thiserror`，信息用中文；`unsafe` 限于 `系统.rs` 的中文封装。
- **验证**：提交前 `make` 通过（clippy `-D warnings` + 33 项单测 + release 构建）。
- **文档同步**：改语法/命令/依赖后更新 `README.md`、`文档/`、`内置.rs` 的 `帮助()`。
- **临时文件**放 `临时/`（已忽略），不要提交 `构建/`。

## 常用命令

```sh
make            # 检查 + 测试 + 构建
make 测试
make 检查
cargo run -- -c '输出文本 你好'
```
