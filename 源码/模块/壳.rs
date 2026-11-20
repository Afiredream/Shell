//! 把各层串成「壳」：持有变量与退出状态，负责一行一行地调度。
//!
//! 分工：
//! - 变量同时存在于 [`壳::变量`] 与进程环境里——外部程序（如 `env`）都要能看到赋值的效果；
//! - 单独出现的内置命令在 shell 进程里执行（所以 `cd`、`export` 能改变 shell 自己）；
//! - 出现在管道或后台里的内置命令由执行层分叉出子进程来跑，不会回头污染父 shell。

use std::collections::HashMap;

use super::内置;
use super::执行::{self, 管道片段, 重定向守护};
use super::系统::整数;
use super::词法;
use super::释义::{释义, 释义器};

pub struct 壳 {
    pub 变量: HashMap<String, String>,
    pub 最后状态: 整数,
    pub 退出: bool,
    /// 退出时使用的状态码
    pub 退出码: 整数,
}

impl 壳 {
    pub fn 新建() -> Self {
        let mut 变量: HashMap<String, String> = std::env::vars().collect();
        变量.entry("PWD".into()).or_insert_with(|| {
            std::env::current_dir()
                .map(|路径| 路径.display().to_string())
                .unwrap_or_default()
        });
        Self {
            变量,
            最后状态: 0,
            退出: false,
            退出码: 0,
        }
    }

    /// 启动时读取 `~/.壳语言rc`（存在才读），用来设置变量、导出环境等。
    /// 启动文件本身出错不会让 shell 起不来。
    pub fn 加载启动文件(&mut self) {
        let Some(家目录) = std::env::var_os("HOME") else {
            return;
        };
        let 路径 = std::path::PathBuf::from(家目录).join(".壳语言rc");
        let Ok(内容) = std::fs::read_to_string(&路径) else {
            return;
        };
        for 每行 in 内容.lines() {
            if self.退出 {
                break;
            }
            self.运行一行(每行);
        }
        // 启动文件的状态码不参与提示符显示
        self.最后状态 = 0;
    }

    pub fn 问候(&self) {
        println!(
            "壳 v{}（壳语言）—— 输入 help 查看用法，Ctrl-D 退出。",
            env!("CARGO_PKG_VERSION")
        );
    }

    /// 提示符：`用户@主机:目录$ `。上一条命令失败时 `$` 变 `#`。
    pub fn 提示符(&self) -> String {
        let 用户 = self
            .变量
            .get("USER")
            .or_else(|| self.变量.get("LOGNAME"))
            .cloned()
            .unwrap_or_else(|| "?".into());
        let 主机 = std::fs::read_to_string("/etc/hostname")
            .map(|文本| 文本.trim().to_string())
            .ok()
            .filter(|文本| !文本.is_empty())
            .or_else(|| self.变量.get("HOSTNAME").cloned())
            .unwrap_or_else(|| "localhost".into());
        let 符号 = if self.最后状态 == 0 { "$" } else { "#" };
        format!("{用户}@{主机}:{}{符号} ", 短目录())
    }

    /// 执行一整行。返回最后一条作业的状态。
    pub fn 运行一行(&mut self, 输入行: &str) -> 整数 {
        let 作业列表 = match 词法::解析命令(输入行) {
            Ok(列表) => 列表,
            Err(语法错误) => {
                eprintln!("语法错误：{语法错误}");
                self.最后状态 = 2;
                return 2;
            }
        };
        for 待执行作业 in &作业列表 {
            if self.退出 {
                break;
            }
            let 环境快照 = self.变量快照();
            let 释义器 = 释义器 {
                环境表: &环境快照,
                最后状态: self.最后状态,
            };
            match 释义器.释义(待执行作业) {
                Err(消息) => {
                    eprintln!("壳语言：{消息}");
                    self.最后状态 = 2;
                }
                Ok(释义::空) => {}
                Ok(释义::赋值(赋值列表)) => {
                    for (名, 值) in 赋值列表 {
                        // 赋值之后同样写进进程环境，`A=1 env` 能看到；
                        // 与 `export` 的差别只在语义表达上
                        std::env::set_var(&名, &值);
                        self.变量.insert(名, 值);
                    }
                    self.最后状态 = 0;
                }
                Ok(释义::管道(片段)) => {
                    self.最后状态 = self.执行管道(片段);
                }
            }
        }
        self.最后状态
    }

    fn 执行管道(&mut self, 片段: 管道片段) -> 整数 {
        // 1) 单独的内置命令：在 shell 自身进程里执行，输出跟着（临时）重定向走
        if let Some(内置命令) = 片段.内置命令() {
            let _守护 = if 片段.重定向.是否设置() || 片段.重定向.合并错误 {
                match 重定向守护::应用(&片段.重定向) {
                    Ok(守护) => Some(守护),
                    Err(错误) => {
                        eprintln!("壳语言：{错误}");
                        return 1;
                    }
                }
            } else {
                None
            };
            let mut 上下文 = 内置::上下文 {
                环境: &mut self.变量快照(),
                最后状态: self.最后状态,
                退出: false,
            };
            let 参数 = &片段.命令[0].参数列表[1..];
            let 状态 = 内置命令.运行(参数, &mut 上下文);
            self.变量 = 上下文.环境.drain(..).collect();
            // 把内核里的环境变量也同步成新表，`cd`/`export` 之后再起的子进程才看得到
            self.重设进程环境();
            if 上下文.退出 {
                self.退出 = true;
                self.退出码 = 状态;
            }
            return 状态;
        }

        // 2) 外部命令：最简单的情况直接用标准库，其余走分叉/管道
        let 单条命令 = &片段.命令[0];
        let 执行结果 = if 片段.命令.len() == 1
            && !片段.重定向.是否设置()
            && !片段.重定向.合并错误
            && !片段.后台
            && 单条命令.环境.is_empty()
        {
            执行::运行单个(单条命令)
        } else {
            执行::运行管道(&片段)
        };
        match 执行结果 {
            Ok(状态) => 状态,
            Err(错误) => {
                eprintln!("壳语言：{错误}");
                1
            }
        }
    }

    fn 变量快照(&self) -> Vec<(String, String)> {
        self.变量
            .iter()
            .map(|(键, 值)| (键.clone(), 值.clone()))
            .collect()
    }

    /// 用当前变量表重建进程环境（`unset` 掉的名字要真的消失）。
    fn 重设进程环境(&self) {
        for 旧键 in std::env::vars_os().map(|(键, _)| 键) {
            if !self.变量.contains_key(旧键.to_string_lossy().as_ref()) {
                std::env::remove_var(&旧键);
            }
        }
        for (键, 值) in &self.变量 {
            std::env::set_var(键, 值);
        }
    }
}

/// 提示符里的目录：家目录显示成 `~`。
fn 短目录() -> String {
    let 家目录 = std::env::var("HOME").unwrap_or_default();
    let 当前目录 = std::env::current_dir()
        .map(|路径| 路径.display().to_string())
        .unwrap_or_else(|_| "?".into());
    if !家目录.is_empty() {
        if 当前目录 == 家目录 {
            return "~".into();
        }
        if let Some(余下) = 当前目录.strip_prefix(&format!("{家目录}/")) {
            return format!("~/{余下}");
        }
    }
    当前目录
}
