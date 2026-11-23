# 参与贡献

欢迎提 issue 和 PR。本项目采用 AGPL-3.0-or-later，贡献代码即表示同意以同一许可发布。

## 开发环境

只需要 Rust 稳定版工具链（含 `clippy`），无第三方依赖。

```sh
rustup toolchain install stable --component clippy
```

## 构建与检查

```sh
cargo build --release          # 产物：构建/release/壳
cargo test                     # 词法、展开、引号的单元测试
cargo clippy                   # 静态检查（要求 0 警告）
cargo run                      # 交互模式（提示符需要真实终端）
cargo run -- -c 'ls | wc -l'   # 执行一条命令
```

`构建/` 是 cargo 的输出目录（由 `.cargo/config.toml` 的 `target-dir` 指定），
不要提交里面的产物；临时草稿放 `临时/`，同样不入库。

## 代码约定

- **中文命名**：目录、文件、类型、函数、变量、闭包参数一律中文。
  不翻译的只有：Rust/标准库关键字、外部命令与命令行选项、C 库 ABI 常量（`O_CREAT` 等），
  以及 Rust 强制要求的入口 `main`。中文模块名通过 `#[path = "中文文件名.rs"]` 接线。
- **C 库绑定**：不引 `libc` crate。在 `源码/模块/系统.rs` 里分两层——
  内层是 `extern "C"` 的 C 符号声明，外层是同名的中文安全封装，其他模块只用中文层。
- **零依赖**：尽量不引入第三方 crate；确实需要时先在 issue 里说明理由。
- **注释**：解释「为什么」而不是「做了什么」，尤其是进程/信号/fd 接线这类容易踩坑的地方。

## 提交 PR

1. 在 issue 里先说明要改什么（大改动尤其如此），避免重复劳动。
2. 一个 PR 只做一件事；提交信息用中文，简短说明动机。
3. 按上面的命令跑通编译、测试、clippy，再在 PR 描述里确认检查清单。
4. 如改动了语法或行为，同步更新 `文档/` 下对应文档；如改动了版本，同步更新 `Cargo.toml`。

## 发布流程

版本号一律读 `Cargo.toml`，发版只需改 `version`。改完提交并打 `v<版本号>` 标签，
在 GitHub 上 Publish 该版本的 Release 即可触发 `.github/workflows/自动发布.yml`。
详见 [README.md](../README.md) 的「发布与下载」。
