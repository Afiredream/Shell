# 壳语言 · 壳

一个用 Rust 写的极简 shell，源码数千行（不含测试），
管道、重定向、后台、控制流、函数、引号、变量、通配符全都自己实现，目录与文件、类型与函数名一律中文。
编译出来的可执行文件就叫 **`壳`**（`cargo build --release` 后位于 `构建/release/壳`）。

```text
$ cargo run
壳语言 壳 v0.1.0 —— 输入 help 查看用法，Ctrl-D 退出。
user@host:~$ 回显 你好 | tr -d '\n'; 回显 世界
你好世界
user@host:~$ ls /不存在的 2>&1 | wc -l
1
user@host:~$ 异步 回显 追加 >> 笔记.txt
[4242] 已放入后台
```

> **文档**：[文档索引](文档/README.md) · [使用手册](文档/使用手册.md) · [语法参考](文档/语法参考.md) · [实现说明](文档/实现说明.md) · [测试与排错](文档/测试与排错.md)

## 支持的功能

| 类别 | 语法 |
| --- | --- |
| 管道 | `a \| b \| c`、`a \|& b`（stderr 也进管道）、`a 2>&1 \| b` |
| 重定向 | `>` `>>` `<`、`2>` `2>>`、`2>&1`、`&>` `&>>`（stdout+stderr 一起） |
| 后台 | `异步 cmd 参数`（旧写法 `cmd &` 仍可用；自成进程组，退出 shell 后不会被挂起） |
| 控制流 | `如果`/`或者`/`否则`、`对于 变量 ...`、`每当 条件`、`跳出`、`继续`（块体用缩进，可嵌套） |
| 函数 | `函数 名称 参数...` 定义（块体用缩进）；`返回 [表达式]` 可返回整数/文本/数组 `(a b)`/对象 `{键=值}`（可嵌套，非 OOP），用 `$返回值` 取回 |
| 值类型 | 整数、文本、数组 `()`、对象 `{}`；对象只是键值对数据，没有方法/类 |
| 分隔 | `;`、换行、行尾 `\` 续行 |
| 引号 | `'...'` 原样；`"..."` 里 `$变量` 照常展开、空白不切分；`\` 转义 |
| 默认变量 | `$参数`、`$参数列表`、`$参数个数`、`$参数对象`（参数解析后的 JSON：命令行 / 选项 / 分隔符 / 剩余）；`$当前目录`、`$脚本路径`、`$脚本目录`；`读取文本`/`读取文件` 读文件、`覆写文件` 写文件（JSON/INI/文本），解析结果放入 `$返回值对象` |
| 展开 | `$变量`、`${变量}`、`$(命令)`（命令替换；先用本 shell 展开变量，再交给 `/bin/sh`）、`~`、`~/路径`、`*`、`?`、`[abc]`、`[!abc]` |
| 特殊变量 | `$?` `$$` `$!` `$#` `$@` `$*` `$0` `$1`… |
| 赋值 | `A=1`（作用于 shell）、`A=1 命令`（只给这条命令） |
| 注释 | `#` 到行尾 |
| 启动文件 | `~/.壳语言rc` 逐行执行 |

内置命令：`进入`（支持 `进入 -`）、`压栈`、`弹栈`、`导出`、`删除`、
`导入`、`输出`、`回显 [-n] [-e]`、`运行 程序 [参数]`、`帮助`、`正常`、`错误`、`退出 [码]`。全部为中文名。

直接运行脚本：`壳 脚本文件 参数...` 逐行执行文件，参数可用 `$1`、`$2`…… 取到（`$0` 是脚本名）。

## 目录约定

| 目录 | 用途 | 是否入库 |
| --- | --- | --- |
| `源码/` | 全部源代码 | 是 |
| `文档/` | 使用手册、语法参考、实现说明等 | 是 |
| `构建/` | cargo 的构建产物（由 `.cargo/config.toml` 的 `target-dir` 指定） | 否 |
| `临时/` | 临时测试、草稿文件，可随时清空 | 否 |
| `测试临时/` | 旧名字，同样被忽略 | 否 |

AI 编程助手的项目约定见 [`AGENTS.md`](AGENTS.md)；另提供 `CLAUDE.md`、`CODEBUDDY.md`、`GEMINI.md`、
`.github/copilot-instructions.md`、`.cursor/rules/项目约定.mdc` 供各工具识别。

## 源码命名

```
源码/
├── 主体.rs            # 入口（Rust 强制入口函数叫 main）
└── 模块/
    ├── 模块.rs        # 模块索引
    ├── 词法.rs        # 分词、语法、引号、展开、通配匹配、控制流/函数语句层
    ├── 释义.rs        # 作业 → 管道片段
    ├── 执行.rs        # 分叉、管道、重定向、等待
    ├── 内置.rs        # 内置命令
    ├── 壳.rs          # 调度器（控制流与函数执行）
    ├── 值.rs          # 整数/文本/数组/对象与表达式求值
    └── 系统.rs        # 基于 libc 的 C 库封装
```

## 实现思路

```text
输入行 ──► 词法（解析器）──► 作业列表 ──► 释义（展开 + 判定内置）──► 管道片段 ──► 执行（fork/pipe/exec）
                │                                                       │
                └── 引号 / 管道 / 重定向 / 后台 / 赋值                    └── 内置命令在 shell 自身进程执行
```

| 文件 | 职责 |
| --- | --- |
| [`源码/模块/词法.rs`](源码/模块/词法.rs) | 把一行拆成「作业、管道、重定向」，记录引号状态；`~`/变量/通配符展开 |
| [`源码/模块/释义.rs`](源码/模块/释义.rs) | 词 → 实参列表；区分「改 shell 的赋值」「前置赋值的环境覆盖」「内置命令」 |
| [`源码/模块/执行.rs`](源码/模块/执行.rs) | `fork` 每段管道、`dup2` 接好 fd、`execvp` 换进程，`waitpid` 回收 |
| [`源码/模块/内置.rs`](源码/模块/内置.rs) | 必须由 shell 亲自动手的命令 |
| [`源码/模块/壳.rs`](源码/模块/壳.rs) | 变量表（同时同步到进程环境）、提示符、一行一行的调度 |
| [`源码/模块/系统.rs`](源码/模块/系统.rs) | 基于 `libc` 的 C 库调用 + 中文安全封装 |

几个值得注意的点：

- **基于 libc**：C 库调用统一走 `libc` crate，不再手写 `extern "C"` 声明。
  `系统.rs` 保留中文安全封装（如 `分叉进程`、`重定向描述符`），其余模块只用中文层。
- **中文命名**：目录、文件、类型、函数、变量、闭包参数、模块标签一律中文；
  不翻译的只有语言关键字与标准库接口、外部命令与命令行选项、C 库 ABI 常量（`O_CREAT` 等），
  以及 Rust 强制要求的入口名 `main`。中文模块名通过 `#[path = "中文文件名.rs"]` 接线。
- **子进程接线的顺序**：先关掉用不到的管道端，再 `dup2`，最后 `execvp`——
  顺序反了会把本进程要用的 fd 先关掉（写这个 shell 时踩过这个坑）。
- **SIGPIPE**：`yes | head -1` 里 `yes` 会被 SIGPIPE 杀掉。某一段被信号终止时，
  直接转入收尾：等完下游、把上游残留的子进程收掉，shell 不会卡住。
- **后台作业**：子进程 `setpgid` 自成一组，否则父 shell 退出后它们读终端会被
  SIGTTIN 挂起。
- **信号与终端**：shell 先把 SIGINT 设为忽略再 fork，避免 `Command` 的 fork→exec
  窗口被 Ctrl-C 打断；子进程 exec 前恢复默认处理。
- **内置命令也能进管道**：单独出现时在 shell 进程执行（`进入`、`导出` 生效）；
  出现在管道/后台时由子进程执行，不会污染父 shell。

## 构建与测试

```sh
cargo build --release          # 产物：构建/release/壳
cargo test                     # 词法、展开、引号、控制流、函数、值的单元测试（33 项）
cargo clippy                   # 静态检查（0 警告）
cargo run                      # 交互模式（提示符需要真实终端）
cargo run -- -c 'ls | wc -l'   # 执行一条命令
```

也可以用 `make`（见 [Makefile](Makefile)）：

```sh
make                           # 等价于 make 检查 测试 构建
make 安装                      # 构建后把「壳」装到 ~/bin/壳
make 卸载                      # 删除 ~/bin/壳
make 安装 安装目录=/别的/目录   # 换安装位置
```

`make install` / `make uninstall` 是 `make 安装` / `make 卸载` 的英文别名。

## 一个完整的会话示例

```text
user@host:~/demo$ printf '苹果\n香蕉\n苹果\n' | sort | uniq -c
      2 苹果
      1 香蕉
user@host:~/demo$ ls /nope 2> 错误.txt; 回显 状态=$?
状态=2
user@host:~/demo$ X=外层; X=内层 env | grep ^X=
X=内层
user@host:~/demo$ 回显 外层=$X
外层=外层
user@host:~/demo$ touch 甲.rs 乙.rs; 回显 *.rs
乙.rs 甲.rs
user@host:~/demo$ 回显 "*.rs"
*.rs
```

## 已知边界

- 只面向 Linux（Arch/glibc 下验证）；`O_*` 常量按 Linux 取值。
- 不做作业控制：没有 `jobs`/`fg`/`bg`，没有 `&&`、`||`、子 shell。
  控制流用中文的 `如果`/`或者`/`否则`、`对于`、`每当` 实现（块体用缩进，不用 `结束`）；函数用 `函数`/`返回`，都支持嵌套。
- 后台作业退出后由 init 回收，shell 不打印完成通知（没有 SIGCHLD 处理）。
- 交互模式用 rustyline 提供行编辑与历史（方向键、历史记录可用）。
- 展开顺序与 bash 略有不同：`$变量` 展开出来的内容不会再被当作通配符解析；
  一个词里只要出现单引号就整词原样（单引号内 `$` 不展开），双引号内只展开变量。

## 依赖与许可证

项目本身采用 AGPL-3.0-or-later。以下第三方库均为 **MIT 或 Apache-2.0** 许可，
与 AGPL-3.0-or-later 兼容；分发时请保留各自的版权与许可声明（见各 crate 自带的 `LICENSE`）。

| 库 | 许可 | 用途 |
| --- | --- | --- |
| `libc` | MIT / Apache-2.0 | C 库调用（替代手写 `extern "C"`） |
| `clap` | MIT / Apache-2.0 | 命令行参数解析 |
| `rustyline` | MIT | 交互行编辑与历史 |
| `thiserror` | MIT / Apache-2.0 | 错误类型派生 |
| `dirs` | MIT / Apache-2.0 | 跨平台家目录 |
| `glob` | MIT / Apache-2.0 | shell 风格通配匹配（替代手写匹配器） |

JSON 读写与值类型序列化已改为手写（`源码/模块/值.rs` 的 `读JSON` / `写JSON`），
不再依赖 `serde`、`serde_json`。

## 许可

本项目采用 **GNU Affero 通用公共许可证第 3 版或任何更高版本**（AGPL-3.0-or-later）授权。

```
Copyright (C) 2026 Afiredream

This program is free software: you can redistribute it and/or modify it under
the terms of the GNU Affero General Public License as published by the Free
Software Foundation, either version 3 of the License, or (at your option) any
later version.

This program is distributed in the hope that it will be useful, but WITHOUT
ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
FOR A PARTICULAR PURPOSE.  See the GNU Affero General Public License for more
details.

You should have received a copy of the GNU Affero General Public License along
with this program.  If not, see <https://www.gnu.org/licenses/>.
```

完整条文见 [LICENSE](LICENSE)。简单说：可以自由使用、修改、分发，但**分发时必须以 AGPL 开源**；
第 13 条还要求，如果把修改后的版本作为网络服务提供给他人使用，也要向他们提供对应的源码。

## 发布与下载

[`.github/workflows/自动发布.yml`](.github/workflows/自动发布.yml) 负责编译 Linux 版本：

- **发布 Release 时自动触发**：在 GitHub 上发布（Publish，草稿转正式）一个 Release 后，
  工作流自动编译并把可执行文件附加到该 Release；
- **版本号一律读取 `Cargo.toml`**：自动与手动触发都一样，发布标签为 `v<版本号>`，无需手工填写；
  发版只需改 `Cargo.toml` 里的 `version`；
- **可手动触发**：Actions →「自动发布」→ Run workflow，只需在 `create_release` 勾选框里选择是否顺手创建（或更新）对应的 Release；
- 只发布 **x86_64 的单个可执行文件**，不打包压缩：
  `shell-<版本>-linux-x86_64` 与同名 `.sha256`（文件名用 ASCII，便于 `curl`/`wget` 直接下载；程序本身仍叫「壳」）；
- 编译出的可执行文件会在运行器上实际跑一遍（`-c` 与 `-h`）作为自检；
- 手动触发时选「不创建 Release」，产物只保留在 Actions 运行页的 Artifacts 里，适合先试构建。

> 注：工作流脚本里的 shell 变量一律用英文名。Runner 上的 bash 只认 ASCII 变量名，
> 写成中文会变成「命令未找到」；工作流名、任务名、步骤名与输入名称则是中文。

下载后安装（重命名为「壳」）：

```sh
# 0.1.0 换成实际版本号
base=https://github.com/Afiredream/Shell/releases/download/v0.1.0
curl -fLO "$base/shell-0.1.0-linux-x86_64"
curl -fsLO "$base/shell-0.1.0-linux-x86_64.sha256"
sha256sum -c shell-0.1.0-linux-x86_64.sha256
sudo install -m 755 shell-0.1.0-linux-x86_64 /usr/local/bin/壳
壳 -c '回显 你好 | tr a-z A-Z'
```

本地源码构建则不用下载：`make 安装` 会把「壳」装到 `~/bin/壳`。

发版步骤（本地）：

```sh
# 1) 改 Cargo.toml 里的 version
# 2) 提交并推送
git commit -am "release：0.2.0" && git push
# 3) 打标签并推送
git tag v0.2.0 && git push origin v0.2.0
# 4) 在 GitHub 上到 Releases → Draft a new release → 选 v0.2.0 → Publish
#    （Publish 的瞬间本工作流自动触发；也可以跳过 3、4，直接在 Actions 里手动运行，
#     若标签 v0.2.0 还不存在，手动运行会自动按 Cargo.toml 的版本创建 Release）
```


