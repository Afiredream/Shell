//! 内置命令：必须在 shell 自己进程里完成的命令。
//!
//! 内置命令直接读写标准输入/输出/错误。带重定向的内置命令会由执行层先用
//! 「重定向守护」把真实的 0/1/2 指到目标上，执行完再恢复 —— 这样内置命令
//! 完全不需要关心自己是在终端、管道还是文件里。
//!
//! `内置命令` 的变体用中文命名；`全部` 表里的字符串才是用户在命令行里敲的名字。

use std::io::{self, Write};

use super::系统::整数;

/// 内置命令执行时能看到的上下文。
pub struct 上下文<'a> {
    pub 环境: &'a mut Vec<(String, String)>,
    /// 上一条命令的退出状态（`$?`）
    pub 最后状态: 整数,
    /// 内置命令要求退出 shell 时置位
    pub 退出: bool,
}

impl 上下文<'_> {
    /// 设置（或新增）一个变量：变量表与进程环境同步更新。
    fn 设置变量(&mut self, 名称: &str, 数据: &str) {
        match self.环境.iter_mut().find(|(已有名称, _)| 已有名称 == 名称) {
            Some((_, 旧值)) => *旧值 = 数据.to_string(),
            None => self.环境.push((名称.to_string(), 数据.to_string())),
        }
        std::env::set_var(名称, 数据);
    }

    fn 删除变量(&mut self, 名称: &str) {
        self.环境.retain(|(已有名称, _)| 已有名称 != 名称);
        std::env::remove_var(名称);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 内置命令 {
    切换目录,
    打印目录,
    退出,
    读取变量,
    删除变量,
    回显,
    显示帮助,
    真,
    假,
    处理环境变量,
}

/// 关键字 → 内置命令。`环境变量` 是正式名字，`export`/`env`/`unset` 保留为旧名，
/// 老脚本和习惯 bash 的人不用改。
const 全部: &[(&str, 内置命令)] = &[
    ("cd", 内置命令::切换目录),
    ("pwd", 内置命令::打印目录),
    ("exit", 内置命令::退出),
    ("环境变量", 内置命令::处理环境变量),
    ("export", 内置命令::处理环境变量), // 旧名保留，便于迁移
    ("env", 内置命令::读取变量),
    ("unset", 内置命令::删除变量),
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
            // `环境变量` 既负责列出，也负责设置
            Self::处理环境变量 => Self::执行环境变量(参数, 上下文),
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
            Self::删除变量 => {
                for 键 in 参数 {
                    上下文.删除变量(键);
                }
                0
            }
            Self::读取变量 => {
                列出全部变量(上下文);
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

    /// `环境变量` 关键字：
    /// - `环境变量`：列出当前 shell 的全部环境变量
    /// - `环境变量 名称=数据`：设置并导出；数据可带引号也可不带，
    ///   带引号是因为有空格，单引号内是纯文本
    fn 执行环境变量(参数: &[String], 上下文: &mut 上下文) -> 整数 {
        if 参数.is_empty() {
            列出全部变量(上下文);
            return 0;
        }
        let mut 状态 = 0;
        for 项 in 参数 {
            let Some(等号位置) = 项.find('=') else {
                let _ = writeln!(io::stderr(), "环境变量: {项}: 缺少 =数据");
                状态 = 1;
                continue;
            };
            let 名称 = &项[..等号位置];
            let 数据 = &项[等号位置 + 1..];
            if !是合法变量名(名称) {
                let _ = writeln!(io::stderr(), "环境变量: {名称}: 不是合法的变量名");
                状态 = 1;
                continue;
            }
            上下文.设置变量(名称, 数据);
        }
        状态
    }
}

/// 变量名：首字符为字母或 `_`，其后为字母、数字、`_`（与赋值规则一致）。
fn 是合法变量名(名称: &str) -> bool {
    let mut 字符迭代 = 名称.chars();
    字符迭代
        .next()
        .is_some_and(|首字| 首字.is_alphabetic() || 首字 == '_')
        && 字符迭代.all(|字符| 字符.is_alphanumeric() || 字符 == '_')
}

/// 列出全部变量，每行 `名称=数据`，按名称排序。
fn 列出全部变量(上下文: &mut 上下文) {
    let mut 列表: Vec<&(String, String)> = 上下文.环境.iter().collect();
    列表.sort_by(|甲, 乙| 甲.0.cmp(&乙.0));
    let mut 输出端 = io::stdout().lock();
    for (键, 值) in 列表 {
        let _ = writeln!(输出端, "{键}={值}");
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
  环境变量         列出当前 shell 的全部环境变量
  环境变量 名=数据  设置并导出环境变量；数据可带引号（有空格时），单引号内是纯文本
  unset 名...      删除环境变量
  export / env     旧名：「export 名=数据」同「环境变量 名=数据」，「env」同「环境变量」
  echo [-n] [-e]   输出文本；-n 不换行，-e 解释 \n \t 等转义
  help             显示本帮助
  true / false     退出码 0 / 1
  exit [码]        退出 shell

启动文件： ~/.壳语言rc 会在启动时逐行执行。
其它：     Ctrl-C 中断当前行，Ctrl-D 退出。"##
    );
}

#[cfg(test)]
mod 测试 {
    use super::*;

    fn 新上下文(环境: &mut Vec<(String, String)>) -> 上下文<'_> {
        上下文 {
            环境,
            最后状态: 0,
            退出: false,
        }
    }

    #[test]
    fn 环境变量列出与设置() {
        let mut 环境: Vec<(String, String)> =
            vec![("乙".into(), "2".into()), ("甲".into(), "1".into())];
        let 命令 = 内置命令::处理环境变量;
        // 无参数：列出，不动内容
        assert_eq!(命令.运行(&[], &mut 新上下文(&mut 环境)), 0);
        // 设置：新增与覆盖
        let 参数 = vec!["甲=覆盖".to_string(), "丙=新".to_string()];
        assert_eq!(命令.运行(&参数, &mut 新上下文(&mut 环境)), 0);
        assert_eq!(环境.iter().find(|(键, _)| 键 == "甲").unwrap().1, "覆盖");
        assert_eq!(环境.iter().find(|(键, _)| 键 == "丙").unwrap().1, "新");
        // 参数展开后不含引号，空格原样保存在数据里
        let 参数 = vec!["带空格的=你好 世界".to_string()];
        assert_eq!(命令.运行(&参数, &mut 新上下文(&mut 环境)), 0);
        assert_eq!(
            环境.iter().find(|(键, _)| 键 == "带空格的").unwrap().1,
            "你好 世界"
        );
        // 非法参数
        assert_eq!(
            命令.运行(&["没有等号".to_string()], &mut 新上下文(&mut 环境)),
            1
        );
        assert_eq!(
            命令.运行(&["1坏名=x".to_string()], &mut 新上下文(&mut 环境)),
            1
        );
    }

    #[test]
    fn 合法变量名() {
        assert!(是合法变量名("名字"));
        assert!(是合法变量名("_x1"));
        assert!(!是合法变量名(""));
        assert!(!是合法变量名("1名"));
        assert!(!是合法变量名("名 字"));
    }
}
