//! 壳语言：一个几百行的极简 shell。
//!
//! 模块划分（全部在 [`mod@模块`] 下，文件名一律用中文）：
//! - [`模块::词法`]：引号、管道、重定向、变量、通配符；
//! - [`模块::释义`]：把「作业」翻译成可直接执行的「管道片段」；
//! - [`模块::执行`]：`fork` / `pipe` / `dup2` / `execvp` / `waitpid`；
//! - [`模块::内置`]：`cd`、`export`、`exit` 等 shell 自身命令；
//! - [`模块::shell`]：串起以上各层，维护变量与退出状态；
//! - [`模块::系统调用`]：手写的极简 libc 绑定（整个项目零第三方依赖）。

#[path = "模块/mod.rs"]
mod 模块;

use std::io::{BufRead, IsTerminal, Write};

use 模块::shell::壳;

fn main() {
    let 参数: Vec<String> = std::env::args().skip(1).collect();
    let mut 壳 = 壳::新建();

    match 参数.first().map(String::as_str) {
        // `shell-lite -c "命令"`：执行一条命令后退出，方便脚本与测试
        Some("-c") => {
            壳.加载启动文件();
            let 代码 = 参数.get(1).cloned().unwrap_or_default();
            let 状态 = 壳.运行一行(&代码);
            std::process::exit(if 壳.退出 { 壳.退出码 } else { 状态 });
        }
        Some("-h" | "--help") => {
            模块::内置::帮助();
            return;
        }
        None => {}
        Some(其它) => {
            eprintln!("shell-lite: 不认识的参数 {其它}（用法：shell-lite [-c 命令]）");
            std::process::exit(2);
        }
    }

    壳.问候();
    壳.加载启动文件();

    // 先让 SIGINT/SIGTERM 被忽略，再 fork 子进程：从 fork 到 exec 的窗口里
    // Ctrl-C 不会把刚建好的子进程打断；子进程 exec 前会恢复成默认处理。
    unsafe {
        use 模块::系统调用 as 系统;
        系统::os_signal(系统::SIGINT, 系统::SIG_IGN);
        系统::os_signal(系统::SIGTERM, 系统::SIG_IGN);
    }

    let 交互 = std::io::stdin().is_terminal();
    let stdin = std::io::stdin();
    let mut 行 = String::new();
    loop {
        if 交互 {
            print!("{}", 壳.提示符());
            let _ = std::io::stdout().flush();
        }
        行.clear();
        match stdin.lock().read_line(&mut 行) {
            Ok(0) => break, // EOF（Ctrl-D）
            Ok(_) => {}
            Err(错误) => {
                eprintln!("读取输入失败：{错误}");
                break;
            }
        }
        壳.运行一行(&行);
        if 壳.退出 {
            break;
        }
    }
    if 交互 {
        println!("再见。");
    }
    std::process::exit(if 壳.退出 { 壳.退出码 } else { 壳.最后状态 });
}
