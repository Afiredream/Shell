//! 壳语言：一个几百行的极简 shell。
//!
//! 模块划分（全部在 [`模块`] 下，目录与文件名一律用中文）：
//! - [`模块::词法`]：引号、管道、重定向、变量、通配符；
//! - [`模块::释义`]：把「作业」翻译成可直接执行的「管道片段」；
//! - [`模块::执行`]：分叉进程、管道、接描述符、换程序、等待进程；
//! - [`模块::内置`]：`cd`、`export`、`exit` 等 shell 自身命令；
//! - [`模块::壳`]：串起以上各层，维护变量与退出状态；
//! - [`模块::系统调用`]：手写的极简 C 库绑定（整个项目零第三方依赖）。

#[path = "模块/mod.rs"]
mod 模块;

use std::io::{BufRead, IsTerminal, Write};

use 模块::壳::壳;

/// 程序入口。Rust 要求入口函数叫 `main`，名字不可改。
fn main() {
    let 命令行参数: Vec<String> = std::env::args().skip(1).collect();
    let mut 当前壳 = 壳::新建();

    match 命令行参数.first().map(String::as_str) {
        // `壳 -c "命令"`：执行一条命令后退出，方便脚本与测试
        Some("-c") => {
            当前壳.加载启动文件();
            let 命令文本 = 命令行参数.get(1).cloned().unwrap_or_default();
            let 状态 = 当前壳.运行一行(&命令文本);
            std::process::exit(if 当前壳.退出 { 当前壳.退出码 } else { 状态 });
        }
        Some("-h" | "--help") => {
            模块::内置::帮助();
            return;
        }
        None => {}
        Some(其它参数) => {
            eprintln!("壳: 不认识的参数 {其它参数}（用法：壳 [-c 命令]）");
            std::process::exit(2);
        }
    }

    当前壳.问候();
    当前壳.加载启动文件();

    // 先让 SIGINT/SIGTERM 被忽略，再分叉子进程：从分叉到换程序的窗口里
    // Ctrl-C 不会把刚建好的子进程打断；子进程换程序前会恢复成默认处理。
    use 模块::系统调用 as 系统;
    系统::设置信号处理(系统::中断信号, 系统::忽略信号);
    系统::设置信号处理(系统::终止信号, 系统::忽略信号);

    let 是交互终端 = std::io::stdin().is_terminal();
    let 标准输入 = std::io::stdin();
    let mut 当前行 = String::new();
    loop {
        if 是交互终端 {
            print!("{}", 当前壳.提示符());
            let _ = std::io::stdout().flush();
        }
        当前行.clear();
        match 标准输入.lock().read_line(&mut 当前行) {
            Ok(0) => break, // 输入结束（Ctrl-D）
            Ok(_) => {}
            Err(错误) => {
                eprintln!("读取输入失败：{错误}");
                break;
            }
        }
        当前壳.运行一行(&当前行);
        if 当前壳.退出 {
            break;
        }
    }
    if 是交互终端 {
        println!("再见。");
    }
    std::process::exit(if 当前壳.退出 { 当前壳.退出码 } else { 当前壳.最后状态 });
}
