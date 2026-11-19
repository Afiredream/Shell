# 壳语言 · shell-lite

一个用 Rust 写的极简 shell：**零第三方依赖**，源码不到一千行（不含测试），
管道、重定向、后台、引号、变量、通配符全都自己实现。

```text
$ cargo run
壳语言 shell-lite v0.1.0 —— 输入 help 查看用法，Ctrl-D 退出。
user@host:~$ echo 你好 | tr -d '\n'; echo 世界
你好世界
user@host:~$ ls /不存在的 2>&1 | wc -l
1
user@host:~$ echo 追加 >> 笔记.txt &
[4242] 已放入后台
```

## 支持的功能

| 类别 | 语法 |
| --- | --- |
| 管道 | `a \| b \| c`、`a \|& b`（stderr 也进管道）、`a 2>&1 \| b` |
| 重定向 | `>` `>>` `<`、`2>` `2>>`、`2>&1`、`&>` `&>>`（stdout+stderr 一起） |
| 后台 | `cmd &`（自成进程组，退出 shell 后不会被挂起） |
| 分隔 | `;`、换行、行尾 `\` 续行 |
| 引号 | `'...'` 原样；`"..."` 里 `$变量` 照常展开、空白不切分；`\` 转义 |
| 展开 | `$变量`、`${变量}`、`$?`、`~`、`~/路径`、`*`、`?`、`[abc]`、`[!abc]` |
| 赋值 | `A=1`（作用于 shell）、`A=1 命令`（只给这条命令） |
| 注释 | `#` 到行尾 |
| 启动文件 | `~/.壳语言rc` 逐行执行 |

内置命令：`cd`（支持 `cd -`）、`pwd`、`export`、`unset`、`env`、`echo [-n] [-e]`、
`help`、`true`、`false`、`exit [码]`。

## 实现思路

```text
输入行 ──► 词法（解析器）──► 作业列表 ──► 释义（展开 + 判定内置）──► 管道片段 ──► 执行（fork/pipe/exec）
                │                                                       │
                └── 引号 / 管道 / 重定向 / 后台 / 赋值                    └── 内置命令在 shell 自身进程执行
```

| 文件 | 职责 |
| --- | --- |
| [`src/模块/词法.rs`](src/模块/词法.rs) | 把一行拆成「作业、管道、重定向」，记录引号状态；`~`/变量/通配符展开 |
| [`src/模块/释义.rs`](src/模块/释义.rs) | 词 → 实参列表；区分「改 shell 的赋值」「前置赋值的环境覆盖」「内置命令」 |
| [`src/模块/执行.rs`](src/模块/执行.rs) | `fork` 每段管道、`dup2` 接好 fd、`execvp` 换进程，`waitpid` 回收 |
| [`src/模块/内置.rs`](src/模块/内置.rs) | 必须由 shell 亲自动手的命令 |
| [`src/模块/shell.rs`](src/模块/shell.rs) | 变量表（同时同步到进程环境）、提示符、一行一行的调度 |
| [`src/模块/系统调用.rs`](src/模块/系统调用.rs) | 手写的 libc 绑定：只要 `extern "C"` 声明，不引 `libc` crate |

几个值得注意的点：

- **零依赖**：直接用 std 对 `libc` 做 `extern "C"` 声明。`execvp`/`open`/`write`
  与 std 内部符号冲突，因此 Rust 侧带 `os_` 前缀，再用 `#[cfg_attr(..., link_name = "...")]`
  指向真正的 libc 符号。
- **子进程接线的顺序**：先关掉用不到的管道端，再 `dup2`，最后 `execvp`——
  顺序反了会把本进程要用的 fd 先关掉（写这个 shell 时踩过这个坑）。
- **SIGPIPE**：`yes | head -1` 里 `yes` 会被 SIGPIPE 杀掉。某一段被信号终止时，
  直接转入收尾：等完下游、把上游残留的子进程收掉，shell 不会卡住。
- **后台作业**：子进程 `setpgid` 自成一组，否则父 shell 退出后它们读终端会被
  SIGTTIN 挂起。
- **信号与终端**：shell 先把 SIGINT 设为忽略再 fork，避免 `Command` 的 fork→exec
  窗口被 Ctrl-C 打断；子进程 exec 前恢复默认处理。
- **内置命令也能进管道**：单独出现时在 shell 进程执行（`cd`、`export` 生效）；
  出现在管道/后台时由子进程执行，不会污染父 shell。

## 构建与测试

```sh
cargo build --release          # 产物：target/release/shell-lite
cargo test                     # 词法、展开、引号的单元测试
cargo run                      # 交互模式（提示符需要真实终端）
cargo run -- -c 'ls | wc -l'   # 执行一条命令
```

## 一个完整的会话示例

```text
user@host:~/demo$ printf '苹果\n香蕉\n苹果\n' | sort | uniq -c
      2 苹果
      1 香蕉
user@host:~/demo$ ls /nope 2> 错误.txt; echo 状态=$?
状态=2
user@host:~/demo$ X=外层; X=内层 env | grep ^X=
X=内层
user@host:~/demo$ echo 外层=$X
外层=外层
user@host:~/demo$ touch 甲.rs 乙.rs; echo *.rs
乙.rs 甲.rs
user@host:~/demo$ echo "*.rs"
*.rs
```

## 已知边界

- 只面向 Linux（Arch/glibc 下验证）；`O_*` 常量按 Linux 取值。
- 不做作业控制：没有 `jobs`/`fg`/`bg`，没有 `&&`、`||`、`if`、循环、函数、命令替换、
  子 shell。
- 后台作业退出后由 init 回收，shell 不打印完成通知（没有 SIGCHLD 处理）。
- 交互模式没有行编辑（没有 readline），方向键/历史记录不可用。
- 展开顺序与 bash 略有不同：`$变量` 展开出来的内容不会再被当作通配符解析；
  含单引号/双引号的整词不再做变量展开（引号内容按原样交给程序）。
