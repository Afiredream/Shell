# CLAUDE.md

核心约定见 [AGENTS.md](AGENTS.md)，请一并遵循。要点：

- 全项目**中文命名**（目录、文件、类型、函数、变量、测试名、提交信息），例外仅 Rust 关键字、库 API、C 符号、外部命令、`main`、Cargo 包名。
- 许可证固定 **AGPL-3.0-or-later**，不得更改。
- 不引入 OOP；数组/对象只是数据。
- 只使用 MIT/Apache-2.0/BSD 等与 AGPL 兼容的开源库；新增依赖同步更新 `Cargo.toml` 注释与 README。
- 提交前 `make` 必须通过（clippy `-D warnings` + 单测 + release 构建）。
- 改语法/命令/依赖要同步 `README.md`、`文档/`、`内置.rs` 的 `帮助()`。
- 临时脚本放 `临时/`，不要提交 `构建/`。

@AGENTS.md
