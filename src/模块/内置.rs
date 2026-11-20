//! 内置命令：必须在 shell 自己进程里完成的命令。
//!
//! 内置命令直接读写标准输入/输出/错误。带重定向的内置命令会由执行层先用
//! 「重定向守护」把真实的 0/1/2 指到目标上，执行完再恢复 —— 这样内置命令
//! 完全不需要关心自己是在终端、管道还是文件里。
//!
//! `内置命令` 的变体用中文命名；`全部` 表里的字符串才是用户在命令行里敲的名字。

use std::io::{self, Write};

use super::系统调用::整数;

/// 内置命令执行时能看到的上下文。
pub struct 上下文<'a> {
    pub 环境: &'a mut Vec<(String, String)>,
    /// 上一条命令的退出状态（`$?`）
    pub 最后状态: 整数,
    /// 内置命令要求退出 shell 时置位
    pub 退出: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 内置命令 {
    切换目录,
    打印目录,
    退出,
    导出变量,
    取消变量,
    列出环境,
    回显,
    显示帮助,
    真,
    假,
}

const 全部: &[(&str, 内置命令)] = &[
    ("cd", 内置命令::切换目录),
    ("pwd", 内置命令::打印目录),
    ("exit", 内置命令::退出),
    ("export", 内置命令::导出变量),
    ("unset", 内置命令::取消变量),
    ("env", 内置命令::列出环境),
    ("echo", 内置命令::回显),
    ("help", 内置命令::显示帮助),
    ("true", 内置命令::真),
    ("false", 内置命令::假),
];

/// 按命令行里敲的名字查内置命令。
pub fn 按名称查找(名称字符串: &str) -> Option<&'static 内置命令> {
    全部
        .iter()
        .find(|(名字, _)| *名字 == 名称字符串)
        .map(|(_, 命令)| 命令)
}

impl 内置命令 {
    /// 执行，返回退出状态。
    pub fn 运行(&self, 参数: &[String], 上下文: &mut 上下文) -> 整数 {
        match self {
            Self::切换目录 => Self::执行切换目录(参数),
            Self::打印目录 => match std::env::current_dir() {
                Ok(路径) => {
                    let _ = writeln!(io::stdout(), "{}", 路径.display());
                    0
                }
                Err(错误) => {
                    let _ = writeln!(io::stderr(), "pwd: {错误}");
                    1
                }
            },
            Self::退出 => {
                上下文.退出 = true;
                let 参数里指定的码: Option<整数> = 参数.first().and_then(|文本| 文本.parse().ok());
                参数里指定的码.unwrap_or(上下文.最后状态)
            }
            Self::导出变量 => Self::导出(参数, 上下文),
            Self::取消变量 => {
                上下文.环境.retain(|(键, _)| !参数.contains(键));
                for 键 in 参数 {
                    std::env::remove_var(键);
                }
                0
            }
            Self::列出环境 => {
                let mut 输出端 = io::stdout().lock();
                for (键, 值) in 上下文.环境.iter() {
                    let _ = writeln!(输出端, "{键}={值}");
                }
                0
            }
            Self::回显 => {
                回显(参数);
                0
            }
            Self::显示帮助 => {
                帮助();
                0
            }
            Self::真 => 0,
            Self::假 => 1,
        }
    }

    fn 执行切换目录(参数: &[String]) -> 整数 {
        let 目标 = match 参数.first().map(String::as_str) {
            None | Some("") => match std::env::var("HOME") {
                Ok(家目录) => 家目录,
                Err(_) => return 0,
            },
            Some("-") => std::env::var("OLDPWD").unwrap_or_else(|_| ".".into()),
            Some(其它) => 其它.to_string(),
        };
        let 旧目录 = std::env::current_dir().ok();
        match std::env::set_current_dir(&目标) {
            Ok(()) => {
                if 参数.first().map(String::as_str) == Some("-") {
                    if let Ok(现在目录) = std::env::current_dir() {
                        println!("{}", 现在目录.display());
                    }
                }
                if let Some(旧) = 旧目录 {
                    std::env::set_var("OLDPWD", 旧);
                }
                0
            }
            Err(错误) => {
                let _ = writeln!(io::stderr(), "cd: {目标}: {错误}");
                1
            }
        }
    }

    fn 导出(参数: &[String], 上下文: &mut 上下文) -> 整数 {
        if 参数.is_empty() {
            let mut 列表: Vec<String> = 上下文
                .环境
                .iter()
                .map(|(键, 值)| format!("export {键}={值}"))
                .collect();
            列表.sort();
            for 每条 in 列表 {
                println!("{每条}");
            }
            return 0;
        }
        let mut 状态 = 0;
        for 项 in 参数 {
            match 项.split_once('=') {
                Some((键, 值)) => {
                    match 上下文.环境.iter_mut().find(|(已有键, _)| 已有键 == 键) {
                        Some((_, 旧值)) => *旧值 = 值.to_string(),
                        None => 上下文.环境.push((键.to_string(), 值.to_string())),
                    }
                    std::env::set_var(键, 值);
                }
                None => {
                    // `export 名称`：不存在的变量只警告（与 bash 一致）
                    if !上下文.环境.iter().any(|(键, _)| 键 == 项) {
                        let _ = writeln!(io::stderr(), "export: {项}: 变量不存在");
                        状态 = 1;
                    }
                }
            }
        }
        状态
    }
}

/// 回显命令：支持 `-n`（结尾不换行）与 `-e`（解释 `\n` `\t` 等转义）。
fn 回显(参数: &[String]) {
    let mut 要换行 = true;
    let mut 要转义 = false;
    let mut 起点 = 0;
    while let Some(选项) = 参数.get(起点) {
        match 选项.as_str() {
            "-n" => 要换行 = false,
            "-e" => 要转义 = true,
            "-ne" | "-en" => {
                要换行 = false;
                要转义 = true;
            }
            _ => break,
        }
        起点 += 1;
    }
    let mut 文本 = 参数[起点..].join(" ");
    if 要转义 {
        文本 = 解释转义(&文本);
    }
    if 要换行 {
        文本.push('\n');
    }
    let 标准输出 = io::stdout();
    let mut 输出端 = 标准输出.lock();
    let _ = 输出端.write_all(文本.as_bytes());
    let _ = 输出端.flush();
}

fn 解释转义(原文: &str) -> String {
    let mut 结果 = String::with_capacity(原文.len());
    let mut 字符迭代 = 原文.chars();
    while let Some(字符) = 字符迭代.next() {
        if 字符 != '\\' {
            结果.push(字符);
            continue;
        }
        match 字符迭代.next() {
            Some('n') => 结果.push('\n'),
            Some('t') => 结果.push('\t'),
            Some('r') => 结果.push('\r'),
            Some('\\') => 结果.push('\\'),
            Some('e') => 结果.push('\x1b'),
            Some(其它) => {
                结果.push('\\');
                结果.push(其它);
            }
            None => 结果.push('\\'),
        }
    }
    结果
}

pub fn 帮助() {
    println!(
        r##"壳 —— 一个几百行的极简 shell（壳语言）

用法： 壳            进入交互模式
       壳 -c "命令"  执行一条命令后退出

语法：
  管道      cmd1 | cmd2          把前一个命令的标准输出接到后一个
            cmd1 2>&1 | cmd2     标准错误也一起进入管道（同 |&）
  重定向    cmd > 文件  >> 文件   覆写 / 追加标准输出
            cmd 2> 文件          重定向标准错误
            cmd < 文件           标准输入来自文件
            cmd &> 文件          标准输出与标准错误一起重定向
  后台      cmd &               放到后台执行
  分隔      cmd1 ; cmd2          依次执行（换行同理）
  引号      单引号内原样；双引号内仅 $ 与 \ 可转义
  展开      $名称 与 ${{名称}}、~/路径、* ? [abc]、# 注释

内置命令：
  cd [目录]        切换目录（cd 回家，cd - 回上一次）
  pwd              打印当前目录
  export [名=值]   设置并导出环境变量（无参数时列出）
  unset 名...      删除环境变量
  env              列出环境变量
  echo [-n] [-e]   输出文本；-n 不换行，-e 解释 \n \t 等转义
  help             显示本帮助
  true / false     退出码 0 / 1
  exit [码]        退出 shell

启动文件： ~/.壳语言rc 会在启动时逐行执行。
其它：     Ctrl-C 中断当前行，Ctrl-D 退出。"##
    );
}
