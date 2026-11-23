//! 壳语言：一个中文命名的 shell。
//!
//! 模块划分（全部在 [`模块`] 下，目录与文件名一律用中文）：
//! - [`模块::词法`]：引号、管道、重定向、变量、通配符、控制流与函数；
//! - [`模块::释义`]：把「作业」翻译成可直接执行的「管道片段」；
//! - [`模块::执行`]：分叉进程、管道、接描述符、换程序、等待进程；
//! - [`模块::内置`]：shell 自身命令；
//! - [`模块::壳`]：串起以上各层，维护变量与退出状态；
//! - [`模块::系统`]：基于 libc 的 C 库封装；
//! - [`模块::值`]：整数/文本/数组/对象与表达式求值。
//!
//! 命令行用 clap 解析；交互模式用 rustyline 提供行编辑与历史（非终端回退到普通读取）。

#[path = "模块/模块.rs"]
mod 模块;

use std::io::{BufRead, IsTerminal};

use clap::Parser;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use 模块::壳::壳;

#[derive(Parser)]
#[command(
    name = "壳",
    version,
    about = "壳语言：中文命名的 shell",
    disable_help_flag = true
)]
struct 参数 {
    /// 执行一段命令后退出（可含换行写控制流）
    #[arg(short = 'c', value_name = "命令")]
    命令: Option<String>,

    /// 显示帮助
    #[arg(short = 'h', long = "help", action = clap::ArgAction::SetTrue)]
    帮助: bool,

    /// 要运行的脚本文件
    #[arg(value_name = "脚本")]
    脚本: Option<String>,

    /// 传给脚本的参数（$1、$2……）
    #[arg(value_name = "参数", trailing_var_arg = true)]
    脚本参数: Vec<String>,
}

/// 程序入口。Rust 要求入口函数叫 `main`，名字不可改。
fn main() {
    let 参数 = 参数::parse();
    let mut 当前壳 = 壳::新建();

    if 参数.帮助 {
        模块::内置::帮助();
        return;
    }

    // `壳 -c "命令"`：执行一段命令后退出
    if let Some(命令文本) = 参数.命令 {
        当前壳.加载启动文件();
        let 状态 = 当前壳.运行脚本文本(&命令文本);
        std::process::exit(if 当前壳.退出 {
            当前壳.退出码
        } else {
            状态
        });
    }

    // `壳 脚本文件 [参数...]`：直接运行脚本
    if let Some(文件路径) = 参数.脚本 {
        当前壳.加载启动文件();
        当前壳.设置位置参数(&文件路径, &参数.脚本参数);
        let 状态 = 当前壳.执行脚本文件(&文件路径);
        std::process::exit(if 当前壳.退出 {
            当前壳.退出码
        } else {
            状态
        });
    }

    // 交互模式
    当前壳.问候();
    当前壳.加载启动文件();

    // 先让 SIGINT/SIGTERM 被忽略，再分叉子进程：从分叉到换程序的窗口里
    // Ctrl-C 不会把刚建好的子进程打断；子进程换程序前会恢复成默认处理。
    use 模块::系统;
    系统::设置信号处理(系统::中断信号, 系统::忽略信号);
    系统::设置信号处理(系统::终止信号, 系统::忽略信号);

    let 是交互终端 = std::io::stdin().is_terminal();
    let mut 编辑器 = if 是交互终端 {
        match DefaultEditor::new() {
            Ok(编辑器) => Some(编辑器),
            Err(错误) => {
                eprintln!("行编辑器初始化失败：{错误}，改用普通输入");
                None
            }
        }
    } else {
        None
    };

    let 标准输入 = std::io::stdin();
    let mut 读取器 = 标准输入.lock();
    let mut 当前行 = String::new();
    let mut 续行 = false;

    loop {
        let 行 = if let Some(编辑器) = 编辑器.as_mut() {
            let 提示符 = if 续行 {
                "> ".to_string()
            } else {
                当前壳.提示符()
            };
            match 编辑器.readline(&提示符) {
                Ok(行) => {
                    let _ = 编辑器.add_history_entry(&行);
                    行
                }
                Err(ReadlineError::Interrupted) => {
                    // Ctrl-C：放弃当前未完成的块
                    当前壳.块缓冲.clear();
                    续行 = false;
                    当前壳.最后状态 = 130;
                    continue;
                }
                Err(ReadlineError::Eof) => break,
                Err(错误) => {
                    eprintln!("读取输入失败：{错误}");
                    break;
                }
            }
        } else {
            当前行.clear();
            match 读取器.read_line(&mut 当前行) {
                Ok(0) => break, // 输入结束（Ctrl-D）
                Ok(_) => 当前行.clone(),
                Err(错误) => {
                    eprintln!("读取输入失败：{错误}");
                    break;
                }
            }
        };

        match 当前壳.运行输入(&行) {
            模块::壳::运行反馈::完成(码) => {
                当前壳.最后状态 = 码;
                续行 = false;
            }
            模块::壳::运行反馈::继续 => 续行 = true,
        }
        if 当前壳.退出 {
            break;
        }
    }

    // Ctrl-D：块没写完要报错并丢弃缓冲，避免污染后续输入
    if !当前壳.块缓冲.is_empty() {
        eprintln!("语法错误：块未结束");
        当前壳.块缓冲.clear();
        当前壳.最后状态 = 2;
    }

    if 是交互终端 {
        println!("再见。");
    }
    std::process::exit(if 当前壳.退出 {
        当前壳.退出码
    } else {
        当前壳.最后状态
    });
}
