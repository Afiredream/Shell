# GEMINI.md

核心约定见 [AGENTS.md](AGENTS.md)。要点：全项目中文命名（例外仅 Rust 关键字、库 API、C 符号、外部命令、`main`、Cargo 包名）；
许可证 AGPL-3.0-or-later 不改；只用 MIT/Apache-2.0/BSD 等开源依赖；不做 OOP；
提交前 `make` 通过（clippy `-D warnings` + 单测 + release 构建）；改功能同步 `README.md`、`文档/`、`内置.rs` 的 `帮助()`。
