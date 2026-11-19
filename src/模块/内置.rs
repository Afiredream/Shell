//! 内置命令：必须在 shell 自己进程里完成的命令。
//!
//! 内置命令直接读写 `std::io::stdin/stdout/stderr`。凡是带重定向或管道的内置命令，
//! 释义层会先 `dup2` 把真实的 0/1/2 指到目标上，执行完再恢复 —— 这样内置命令
//! 完全不需要关心自己是在终端、管道还是文件里。

use std::io::{self, Write};

pub struct 上下文<'a> {
    pub 环境: &'a mut Vec<(String, String)>,
    /// 上一条命令的退出状态（`$?`）
    pub 最后状态: i32,
    /// 内置命令要求退出 shell 时置位
    pub 退出: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 内置命令 {
    Cd,
    Pwd,
    Exit,
    Export,
    Unset,
    Env,
    Echo,
    Help,
    True,
    False,
}

const 全部: &[(&str, 内置命令)] = &[
    ("cd", 内置命令::Cd),
    ("pwd", 内置命令::Pwd),
    ("exit", 内置命令::Exit),
    ("export", 内置命令::Export),
    ("unset", 内置命令::Unset),
    ("env", 内置命令::Env),
    ("echo", 内置命令::Echo),
    ("help", 内置命令::Help),
    ("true", 内置命令::True),
    ("false", 内置命令::False),
];

/// 按名字查内置命令。
pub fn 查找(名称: &str) -> Option<&'static 内置命令> {
    全部.iter().find(|(名, _)| *名 == 名称).map(|(_, 命令)| 命令)
}

impl 内置命令 {
    /// 执行，返回退出状态。
    pub fn 运行(&self, 参数: &[String], 上下文: &mut 上下文) -> i32 {
        match self {
            Self::Cd => Self::切换目录(参数),
            Self::Pwd => match std::env::current_dir() {
                Ok(路径) => {
                    let _ = writeln!(io::stdout(), "{}", 路径.display());
                    0
                }
                Err(错误) => {
                    let _ = writeln!(io::stderr(), "pwd: {错误}");
                    1
                }
            },
            Self::Exit => {
                上下文.退出 = true;
                参数.first().and_then(|s| s.parse().ok()).unwrap_or(上下文.最后状态)
            }
            Self::Export => Self::导出(参数, 上下文),
            Self::Unset => {
                上下文.环境.retain(|(k, _)| !参数.contains(k));
                for 键 in 参数 {
                    std::env::remove_var(键);
                }
                0
            }
            Self::Env => {
                let mut 输出 = io::stdout().lock();
                for (键, 值) in 上下文.环境.iter() {
                    let _ = writeln!(输出, "{键}={值}");
                }
                0
            }
            Self::Echo => {
                输出(参数);
                0
            }
            Self::Help => {
                帮助();
                0
            }
            Self::True => 0,
            Self::False => 1,
        }
    }

    fn 切换目录(参数: &[String]) -> i32 {
        let 目标 = match 参数.first().map(String::as_str) {
            None | Some("") => match std::env::var("HOME") {
                Ok(家) => 家,
                Err(_) => return 0,
            },
            Some("-") => std::env::var("OLDPWD").unwrap_or_else(|_| ".".into()),
            Some(其他) => 其他.to_string(),
        };
        let 旧目录 = std::env::current_dir().ok();
        match std::env::set_current_dir(&目标) {
            Ok(()) => {
                if 参数.first().map(String::as_str) == Some("-") {
                    if let Ok(现在) = std::env::current_dir() {
                        println!("{}", 现在.display());
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

    fn 导出(参数: &[String], 上下文: &mut 上下文) -> i32 {
        if 参数.is_empty() {
            let mut 列表: Vec<String> = 上下文.环境.iter().map(|(k, v)| format!("export {k}={v}")).collect();
            列表.sort();
            for 行 in 列表 {
                println!("{行}");
            }
            return 0;
        }
        let mut 状态 = 0;
        for 项 in 参数 {
            match 项.split_once('=') {
                Some((键, 值)) => {
                    match 上下文.环境.iter_mut().find(|(k, _)| k == 键) {
                        Some((_, 旧值)) => *旧值 = 值.to_string(),
                        None => 上下文.环境.push((键.to_string(), 值.to_string())),
                    }
                    std::env::set_var(键, 值);
                }
                None => {
                    // `export NAME`：不存在的变量只警告（与 bash 一致）
                    if !上下文.环境.iter().any(|(k, _)| k == 项) {
                        let _ = writeln!(io::stderr(), "export: {项}: 变量不存在");
                        状态 = 1;
                    }
                }
            }
        }
        状态
    }
}

/// `echo` 支持 `-n`（结尾不换行）与 `-e`（解释 `\n` `\t` 等转义）。
fn 输出(参数: &[String]) {
    let mut 换行 = true;
    let mut 转义 = false;
    let mut 起点 = 0;
    while let Some(选项) = 参数.get(起点) {
        match 选项.as_str() {
            "-n" => 换行 = false,
            "-e" => 转义 = true,
            "-ne" | "-en" => {
                换行 = false;
                转义 = true;
            }
            _ => break,
        }
        起点 += 1;
    }
    let mut 文本 = 参数[起点..].join(" ");
    if 转义 {
        文本 = 解释转义(&文本);
    }
    if 换行 {
        文本.push('\n');
    }
    let 标准输出 = io::stdout();
    let mut 输出 = 标准输出.lock();
    let _ = 输出.write_all(文本.as_bytes());
    let _ = 输出.flush();
}

fn 解释转义(s: &str) -> String {
    let mut 结果 = String::with_capacity(s.len());
    let mut 字符 = s.chars();
    while let Some(c) = 字符.next() {
        if c != '\\' {
            结果.push(c);
            continue;
        }
        match 字符.next() {
            Some('n') => 结果.push('\n'),
            Some('t') => 结果.push('\t'),
            Some('r') => 结果.push('\r'),
            Some('\\') => 结果.push('\\'),
            Some('e') => 结果.push('\x1b'),
            Some(其他) => {
                结果.push('\\');
                结果.push(其他);
            }
            None => 结果.push('\\'),
        }
    }
    结果
}

pub fn 帮助() {
    println!(
        r##"壳语言 shell-lite —— 一个几百行的极简 shell

用法： shell-lite           进入交互模式
       shell-lite -c "命令"  执行一条命令后退出

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
  展开      $名称 ${{名称}} ~/路径 * ? [abc] # 注释

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
