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
use super::词法::{self, 语句};
use super::值::{self, 值 as 运行时值};
use super::释义::{释义, 释义器};

/// 控制流信号。用返回值传递，避免「忘记清除」的状态残留。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum 流 {
    正常(整数),
    跳出(整数),
    继续(整数),
    返回(整数),
}

/// 交互模式一行输入的反馈。
pub enum 运行反馈 {
    完成(整数),
    继续,
}

pub struct 壳 {
    pub 变量: HashMap<String, String>,
    /// 目录栈：`压栈目录` 推入、`弹栈目录` 取出
    pub 目录栈: Vec<std::path::PathBuf>,
    pub 最后状态: 整数,
    pub 退出: bool,
    /// 退出时使用的状态码
    pub 退出码: 整数,
    /// 交互模式下未完成的块缓冲
    pub 块缓冲: Vec<String>,
    /// 函数表：名称 → (参数名列表, 函数体)
    pub 函数表: HashMap<String, (Vec<String>, Vec<语句>)>,
    /// 最近一次函数返回的值（`$返回值` 取其文本形式）
    pub 返回值: 运行时值,
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
            目录栈: Vec::new(),
            最后状态: 0,
            退出: false,
            退出码: 0,
            块缓冲: Vec::new(),
            函数表: HashMap::new(),
            返回值: 运行时值::整数(0),
        }
    }

    /// 启动时读取 `~/.壳语言rc`（存在才读），用来设置变量、导出环境等。
    /// 启动文件本身出错不会让 shell 起不来。
    /// 启动时读取 `~/.壳语言rc`（存在才读），用来设置变量、导出环境等。
    /// 启动文件本身出错不会让 shell 起不来。
    pub fn 加载启动文件(&mut self) {
        let Some(家目录) = dirs::home_dir() else {
            return;
        };
        let 路径 = 家目录.join(".壳语言rc");
        if !路径.exists() {
            return;
        }
        self.执行脚本文件(&路径.display().to_string());
        // 启动文件的状态码不参与提示符显示
        self.最后状态 = 0;
    }

    /// `加载脚本` 的实现：在当前 shell 里逐行执行文件，变量、目录、目录栈都会保留。
    /// 空行与 `#` 注释行与交互输入完全一致地处理。
    pub fn 执行脚本文件(&mut self, 文件名: &str) -> 整数 {
        let 内容 = match std::fs::read_to_string(文件名) {
            Ok(内容) => 内容,
            Err(错误) => {
                use std::io::Write;
                let _ = writeln!(std::io::stderr(), "加载脚本: {文件名}: {错误}");
                return 1;
            }
        };
        let 语句列表 = match 词法::解析脚本(&内容) {
            Ok(列表) => 列表,
            Err(错误) => {
                eprintln!("语法错误：{错误}");
                return 2;
            }
        };
        let 结果 = self.执行语句列表(&语句列表);
        // 脚本里改过的变量（内置命令已同步写进进程环境）并回 shell 的变量表
        self.变量.extend(std::env::vars());
        match 结果 {
            流::正常(码) => 码,
            // 解析阶段已拦截循环外的跳出/继续；到顶层的按当前状态处理
            流::跳出(码) | 流::继续(码) | 流::返回(码) => 码,
        }
    }

    /// 给脚本设置位置参数：脚本路径是 `$0`，后面的参数依次是 `$1`、`$2`……
    /// 位置参数只在 shell 内部可见，不写进进程环境（与 bash 一致）。
    pub fn 设置位置参数(&mut self, 脚本路径: &str, 参数: &[String]) {
        self.变量.insert("0".into(), 脚本路径.into());
        for (序号, 值) in 参数.iter().enumerate() {
            self.变量.insert((序号 + 1).to_string(), 值.clone());
        }
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

    /// 执行一组作业。
    fn 执行作业列表(&mut self, 作业列表: &[词法::作业]) -> 整数 {
        for 待执行作业 in 作业列表 {
            if self.退出 {
                break;
            }
            let 释义结果 = {
                let 释义器 = 释义器 {
                    环境表: &self.变量,
                    最后状态: self.最后状态,
                    命令替换: &执行命令替换,
                };
                释义器.释义(待执行作业)
            };
            match 释义结果 {
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

    /// 执行语句序列；跳出/继续向上传播，由最近的循环消费。
    pub fn 执行语句列表(&mut self, 语句们: &[语句]) -> 流 {
        let mut 状态 = 0;
        for 语句 in 语句们 {
            if self.退出 {
                return 流::正常(self.退出码);
            }
            match self.执行语句(语句) {
                流::正常(码) => 状态 = 码,
                其它 => return 其它,
            }
        }
        流::正常(状态)
    }

    fn 执行语句(&mut self, 语句: &语句) -> 流 {
        match 语句 {
            语句::作业列表(作业列表) => 流::正常(self.执行作业列表(作业列表)),
            语句::如果 { 分支, 否则体 } => {
                for (条件, 体) in 分支 {
                    if self.执行作业列表(std::slice::from_ref(条件)) == 0 {
                        return self.执行语句列表(体);
                    }
                }
                if let Some(体) = 否则体 {
                    return self.执行语句列表(体);
                }
                流::正常(0)
            }
            语句::对于 { 变量, 取值, 体 } => {
                let mut 状态 = 0;
                for 字 in 取值 {
                    let 值列表 = {
                        let 释义器 = 释义器 {
                            环境表: &self.变量,
                            最后状态: self.最后状态,
                            命令替换: &执行命令替换,
                        };
                        释义器.展开(字)
                    };
                    for 值 in 值列表 {
                        // 只更新 shell 变量表；不每次写进程环境（循环变量由本 shell 展开）
                        self.变量.insert(变量.clone(), 值);
                        match self.执行语句列表(体) {
                            流::正常(码) => 状态 = 码,
                            流::返回(码) => return 流::返回(码),
                            流::跳出(码) => return 流::正常(码),
                            流::继续(码) => {
                                状态 = 码;
                                continue;
                            }
                        }
                        if self.退出 {
                            return 流::正常(self.退出码);
                        }
                    }
                }
                流::正常(状态)
            }
            语句::每当 { 条件, 体 } => {
                let mut 状态 = 0;
                loop {
                    if self.退出 {
                        return 流::正常(self.退出码);
                    }
                    if self.执行作业列表(std::slice::from_ref(条件)) != 0 {
                        break;
                    }
                    match self.执行语句列表(体) {
                        流::正常(码) => 状态 = 码,
                        流::返回(码) => return 流::返回(码),
                        流::跳出(码) => {
                            状态 = 码;
                            break;
                        }
                        流::继续(码) => 状态 = 码,
                    }
                }
                流::正常(状态)
            }
            语句::跳出 => 流::跳出(self.最后状态),
            语句::继续 => 流::继续(self.最后状态),
            语句::函数 { 名称, 参数, 体 } => {
                self.函数表.insert(名称.clone(), (参数.clone(), 体.clone()));
                流::正常(0)
            }
            语句::返回(表达式) => {
                let 返回值 = match 表达式 {
                    Some(表达式) => 值::求值(表达式, &|名| {
                        self.变量
                            .get(名)
                            .cloned()
                            .or_else(|| std::env::var(名).ok())
                    }),
                    None => 运行时值::整数(self.最后状态 as i64),
                };
                let 退出码 = 返回值.转退出码() as 整数;
                self.返回值 = 返回值.clone();
                self.变量.insert("返回值".into(), 返回值.转文本());
                self.变量.insert("返回值JSON".into(), 返回值.转JSON());
                // `$参数对象` 常用于验证解析结果：它能再还原成对象（数组、选项、剩余）
                if let Some(对象) = 值::值::读JSON(&返回值.转JSON()) {
                    if let 运行时值::对象(_) = 对象 {
                        self.变量.insert("返回值对象".into(), 对象.转文本());
                    }
                }
                self.最后状态 = 退出码;
                流::返回(退出码)
            }
        }
    }

    /// 交互模式：缩进块用空行结束；单条语句立即执行。
    pub fn 运行输入(&mut self, 行: &str) -> 运行反馈 {
        if 行.trim().is_empty() {
            if self.块缓冲.is_empty() {
                return 运行反馈::完成(0);
            }
            let 文本 = std::mem::take(&mut self.块缓冲).join("\n");
            return self.执行缓冲文本(&文本);
        }
        let 多行 = {
            let 首行 = self.块缓冲.first().map(String::as_str).unwrap_or(行);
            let 首词 = 首行.split_whitespace().next().unwrap_or("");
            matches!(首词, "如果" | "或者" | "否则" | "对于" | "每当" | "函数")
        };
        self.块缓冲.push(行.to_string());
        if 多行 {
            运行反馈::继续
        } else {
            let 文本 = std::mem::take(&mut self.块缓冲).join("\n");
            self.执行缓冲文本(&文本)
        }
    }

    fn 执行缓冲文本(&mut self, 文本: &str) -> 运行反馈 {
        match 词法::解析脚本(文本) {
            Ok(语句列表) => {
                let 状态 = match self.执行语句列表(&语句列表) {
                    流::正常(码) => 码,
                    流::跳出(码) | 流::继续(码) | 流::返回(码) => 码,
                };
                运行反馈::完成(状态)
            }
            Err(错误) => {
                eprintln!("语法错误：{错误}");
                运行反馈::完成(2)
            }
        }
    }

    /// 执行一段脚本文本（`-c` 用）；文本里的换行可以表达块。
    pub fn 运行脚本文本(&mut self, 文本: &str) -> 整数 {
        let 语句列表 = match 词法::解析脚本(文本) {
            Ok(列表) => 列表,
            Err(错误) => {
                eprintln!("语法错误：{错误}");
                return 2;
            }
        };
        match self.执行语句列表(&语句列表) {
            流::正常(码) => 码,
            流::跳出(码) | 流::继续(码) | 流::返回(码) => 码,
        }
    }

    fn 执行管道(&mut self, 片段: 管道片段) -> 整数 {
        // 0) 函数调用：函数优先于内置命令与外部命令
        if 片段.命令.len() == 1 && !片段.后台 {
            let 程序名 = 片段.命令[0].程序名();
            let 定义 = self.函数表.get(&程序名).cloned();
            if let Some((参数名, 体)) = 定义 {
                let 实参 = 片段.命令[0].参数列表[1..].to_vec();
                return self.执行函数调用(&程序名, &参数名, &实参, &体, &片段.重定向);
            }
        }

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
            // 只读内置命令：不克隆变量表、不合并进程环境，直接用空上下文跑。
            // 这些命令不读写 shell 变量/目录栈，省掉每命令一次的整表克隆与 vars() 收集。
            if matches!(
                内置命令,
                内置::内置命令::输出文本
                    | 内置::内置命令::输出
                    | 内置::内置命令::替换运行
                    | 内置::内置命令::显示帮助
                    | 内置::内置命令::真
                    | 内置::内置命令::假
            ) {
                let mut 空环境: Vec<(String, String)> = Vec::new();
                let mut 空栈: Vec<std::path::PathBuf> = Vec::new();
                let 状态 = {
                    let 壳本身 = &mut *self;
                    let mut 上下文 = 内置::上下文 {
                        环境: &mut 空环境,
                        最后状态: 壳本身.最后状态,
                        退出: false,
                        目录栈: &mut 空栈,
                        执行脚本文件: &mut |文件| 壳本身.执行脚本文件(文件),
                    };
                    let 参数 = &片段.命令[0].参数列表[1..];
                    let 状态 = 内置命令.运行(参数, &mut 上下文);
                    if 上下文.退出 {
                        壳本身.退出 = true;
                        壳本身.退出码 = 状态;
                    }
                    状态
                };
                return 状态;
            }

            let mut 环境表 = self.变量快照();
            let mut 目录栈 = std::mem::take(&mut self.目录栈);
            let 状态;
            {
                // 拆开借用：执行脚本的闭包需要 `&mut self`，环境表与目录栈则作为参数借出
                let 壳本身 = &mut *self;
                let mut 上下文 = 内置::上下文 {
                    环境: &mut 环境表,
                    最后状态: 壳本身.最后状态,
                    退出: false,
                    目录栈: &mut 目录栈,
                    执行脚本文件: &mut |文件| 壳本身.执行脚本文件(文件),
                };
                let 参数 = &片段.命令[0].参数列表[1..];
                状态 = 内置命令.运行(参数, &mut 上下文);
                if 上下文.退出 {
                    壳本身.退出 = true;
                    壳本身.退出码 = 状态;
                }
            }
            self.目录栈 = 目录栈;
            self.变量 = self.合并内置修改(环境表);
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

    /// 执行函数调用：命名参数与位置参数只在调用期间生效，返回后恢复。
    fn 执行函数调用(
        &mut self,
        名称: &str,
        参数名: &[String],
        实参: &[String],
        体: &[语句],
        重定向: &执行::重定向,
    ) -> 整数 {
        let _守护 = if 重定向.是否设置() || 重定向.合并错误 {
            match 重定向守护::应用(重定向) {
                Ok(守护) => Some(守护),
                Err(错误) => {
                    eprintln!("壳语言：{错误}");
                    return 1;
                }
            }
        } else {
            None
        };

        let 原位置参数 = self.位置参数快照();
        let 原命名参数: Vec<(String, Option<String>)> = 参数名
            .iter()
            .map(|名字| (名字.clone(), self.变量.get(名字).cloned()))
            .collect();

        self.变量.insert("0".into(), 名称.to_string());
        for (序号, 名字) in 参数名.iter().enumerate() {
            let 值 = 实参.get(序号).cloned().unwrap_or_default();
            self.变量.insert(名字.clone(), 值);
        }
        for (序号, 值) in 实参.iter().enumerate() {
            self.变量.insert((序号 + 1).to_string(), 值.clone());
        }

        let 结果 = self.执行语句列表(体);
        let 码 = match 结果 {
            流::正常(码) | 流::返回(码) => 码,
            // 解析阶段已拦截循环外的跳出/继续；函数体内它们也不越过函数边界
            流::跳出(码) | 流::继续(码) => 码,
        };

        self.恢复位置参数(原位置参数);
        for (名字, 原值) in 原命名参数 {
            if let Some(值) = 原值 {
                self.变量.insert(名字, 值);
            } else {
                self.变量.remove(&名字);
            }
        }
        码
    }

    fn 位置参数快照(&self) -> Vec<(String, Option<String>)> {
        self.变量
            .keys()
            .filter(|键| !键.is_empty() && 键.chars().all(|字符| 字符.is_ascii_digit()))
            .map(|键| (键.clone(), self.变量.get(键).cloned()))
            .collect()
    }

    fn 恢复位置参数(&mut self, 快照: Vec<(String, Option<String>)>) {
        let 数字键: Vec<String> = self
            .变量
            .keys()
            .filter(|键| !键.is_empty() && 键.chars().all(|字符| 字符.is_ascii_digit()))
            .cloned()
            .collect();
        for 键 in 数字键 {
            self.变量.remove(&键);
        }
        for (键, 值) in 快照 {
            if let Some(值) = 值 {
                self.变量.insert(键, 值);
            }
        }
    }

    /// 内置命令执行完后合并变量。
    ///
    /// 内置命令改变量时总会同步写 `set_var` / `remove_var`，所以以**进程环境**为准；
    /// 只是这样会漏掉「表里有、但进程环境里没有」的普通赋值（赋值本身也写 `set_var`，
    /// 正常不会出现），为了稳妥，原表里进程环境没有的键保留。
    fn 合并内置修改(&self, 原表: Vec<(String, String)>) -> HashMap<String, String> {
        let 进程环境: HashMap<String, String> = std::env::vars().collect();
        let mut 结果: HashMap<String, String> = 原表.into_iter().collect();
        for (键, 值) in 进程环境 {
            结果.insert(键, 值);
        }
        结果
    }

    fn 变量快照(&self) -> Vec<(String, String)> {
        self.变量
            .iter()
            .map(|(键, 值)| (键.clone(), 值.clone()))
            .collect()
    }
}

/// `$(命令)` 的执行器：交给 `/bin/sh` 运行，取标准输出并去掉结尾换行。
/// 用系统 shell 是为了支持命令里的管道/重定向；它不认识本 shell 的中文内置命令。
fn 执行命令替换(命令: &str, 查变量: &dyn Fn(&str) -> Option<String>) -> String {
    let 展开后 = 展开命令文本(命令, 查变量);
    match std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(展开后)
        .output()
    {
        Ok(输出) => String::from_utf8_lossy(&输出.stdout)
            .trim_end_matches('\n')
            .to_string(),
        Err(_) => String::new(),
    }
}

/// 交给 `/bin/sh` 前，先用本 shell 的变量表展开 `$(...)` 里的 `$变量`/`${变量}`：
/// sh 不认中文变量名。单引号内的内容原样保留（留给内层程序，如 `awk '{print $1}'`），
/// 变量不存在时也原样保留交给 sh。
fn 展开命令文本(文本: &str, 查变量: &dyn Fn(&str) -> Option<String>) -> String {
    let 字符: Vec<char> = 文本.chars().collect();
    let mut 结果 = String::with_capacity(文本.len());
    let mut i = 0;
    let mut 单引号 = false;
    while i < 字符.len() {
        let c = 字符[i];
        if c == '\'' {
            单引号 = !单引号;
            结果.push(c);
            i += 1;
            continue;
        }
        if !单引号 && c == '$' {
            match 字符.get(i + 1) {
                Some('{') => {
                    if let Some(偏移) = 字符[i + 2..].iter().position(|&x| x == '}') {
                        let 名: String = 字符[i + 2..i + 2 + 偏移].iter().collect();
                        match 查变量(&名) {
                            Some(值) => 结果.push_str(&值),
                            None => {
                                for 字 in &字符[i..i + 3 + 偏移] {
                                    结果.push(*字);
                                }
                            }
                        }
                        i = i + 3 + 偏移;
                        continue;
                    }
                }
                Some(&首) if 首.is_alphanumeric() || 首 == '_' => {
                    let 起点 = i + 1;
                    let mut 终点 = 起点;
                    while 终点 < 字符.len() && (字符[终点].is_alphanumeric() || 字符[终点] == '_') {
                        终点 += 1;
                    }
                    let 名: String = 字符[起点..终点].iter().collect();
                    match 查变量(&名) {
                        Some(值) => 结果.push_str(&值),
                        None => {
                            for 字 in &字符[i..终点] {
                                结果.push(*字);
                            }
                        }
                    }
                    i = 终点;
                    continue;
                }
                _ => {}
            }
        }
        结果.push(c);
        i += 1;
    }
    结果
}

/// 提示符里的目录：家目录显示成 `~`。
fn 短目录() -> String {
    let 家目录 = dirs::home_dir()
        .map(|路径| 路径.display().to_string())
        .unwrap_or_default();
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

#[cfg(test)]
mod 测试 {
    use super::*;

    #[test]
    fn 脚本位置参数() {
        let mut 实例 = 壳::新建();
        let 原有一 = std::env::var_os("1");
        实例.设置位置参数("脚本.壳", &["甲".into(), "乙".into()]);
        assert_eq!(实例.变量.get("0").map(String::as_str), Some("脚本.壳"));
        assert_eq!(实例.变量.get("1").map(String::as_str), Some("甲"));
        assert_eq!(实例.变量.get("2").map(String::as_str), Some("乙"));
        assert!(!实例.变量.contains_key("3"));
        // 位置参数只在 shell 内部，不写进进程环境
        assert_eq!(std::env::var_os("1"), 原有一);
    }

    #[test]
    fn 默认参数变量() {
        let mut 实例 = 壳::新建();
        // 没有参数
        实例.设置位置参数("脚本", &[]);
        实例.运行脚本文本("个数=$参数个数; 列表=[$参数列表]");
        assert_eq!(实例.变量.get("个数").map(String::as_str), Some("0"));
        assert_eq!(实例.变量.get("列表").map(String::as_str), Some("[]"));

        // 选项、= 数据、-- 分隔、剩余
        实例.设置位置参数(
            "脚本",
            &[
                "构建".into(),
                "--release".into(),
                "--输出".into(),
                "json".into(),
                "--file=数据.txt".into(),
                "--".into(),
                "剩余一".into(),
            ],
        );
        实例.运行脚本文本("对象=$参数对象; 个数=$参数个数");
        assert_eq!(实例.变量.get("个数").map(String::as_str), Some("7"));
        assert_eq!(
            实例.变量.get("对象").map(String::as_str),
            Some(
                r#"{"命令行":["构建","json"],"选项":{"release":"true","输出":"true","file":"数据.txt"},"分隔符":"--","剩余":["剩余一"]}"#
            )
        );
    }

    #[test]
    fn 控制流执行() {
        let mut 实例 = 壳::新建();

        // 如果 / 或者 / 否则：只有命中的分支改变量
        let 状态 = 实例.运行脚本文本(
            "分支=无\n如果 错误\n    分支=一\n或者 正常\n    分支=二\n否则\n    分支=三",
        );
        assert_eq!(状态, 0);
        assert_eq!(实例.变量.get("分支").map(String::as_str), Some("二"));

        // 对于：逐值赋予循环变量；块内改动块外可见
        let 状态 = 实例.运行脚本文本("结果=\n对于 项 甲 乙 丙\n    结果=${结果}${项}");
        assert_eq!(状态, 0);
        assert_eq!(实例.变量.get("结果").map(String::as_str), Some("甲乙丙"));

        // 每当：条件错误后停止
        let 状态 = 实例.运行脚本文本("次数=\n每当 test \"$次数\" != \"aa\"\n    次数=a$次数");
        assert_eq!(状态, 0);
        assert_eq!(实例.变量.get("次数").map(String::as_str), Some("aa"));

        // 继续 / 跳出
        let 状态 = 实例.运行脚本文本(
            "收=\n对于 项 甲 乙 丙\n    如果 test \"$项\" = \"乙\"\n        继续\n    收=${收}${项}",
        );
        assert_eq!(状态, 0);
        assert_eq!(实例.变量.get("收").map(String::as_str), Some("甲丙"));

        let 状态 = 实例.运行脚本文本(
            "收=\n对于 项 甲 乙 丙\n    如果 test \"$项\" = \"乙\"\n        跳出\n    收=${收}${项}",
        );
        assert_eq!(状态, 0);
        assert_eq!(实例.变量.get("收").map(String::as_str), Some("甲"));
    }

    #[test]
    fn 函数执行() {
        let mut 实例 = 壳::新建();

        // 用独特变量名，避免与其它并行测试写进进程环境的同名变量串扰
        // 定义 + 命名参数
        let 状态 = 实例.运行脚本文本("函数 问候 名字\n    函果甲=你好$名字\n问候 世界");
        assert_eq!(状态, 0);
        assert_eq!(实例.变量.get("函果甲").map(String::as_str), Some("你好世界"));

        // 返回码
        let 状态 = 实例.运行脚本文本("函数 给码\n    返回 7\n给码");
        assert_eq!(状态, 7);

        // 命名参数与位置参数在函数返回后恢复
        实例.设置位置参数("脚本", &["全局".into()]);
        let 状态 = 实例.运行脚本文本(
            "函数 设置 函参甲\n    函果甲=内$函参甲$1\n函参甲=外\n设置 传\n函果乙=外$函参甲$1",
        );
        assert_eq!(状态, 0);
        assert_eq!(实例.变量.get("函果甲").map(String::as_str), Some("内传传"));
        assert_eq!(实例.变量.get("函果乙").map(String::as_str), Some("外外全局"));
    }

    #[test]
    fn 特殊变量() {
        let mut 实例 = 壳::新建();

        // $? 取上一条命令的状态
        实例.运行脚本文本("错误; 结果=$?");
        assert_eq!(实例.变量.get("结果").map(String::as_str), Some("1"));

        // $# $@ $* 来自位置参数
        实例.设置位置参数("脚本", &["甲".into(), "乙".into()]);
        实例.运行脚本文本("个数=$#; 全部=$@; 星=$*");
        assert_eq!(实例.变量.get("个数").map(String::as_str), Some("2"));
        assert_eq!(实例.变量.get("全部").map(String::as_str), Some("甲 乙"));
        assert_eq!(实例.变量.get("星").map(String::as_str), Some("甲 乙"));

        // $$ 是当前进程号
        实例.运行脚本文本("进程=$$");
        assert!(实例
            .变量
            .get("进程")
            .map(|值| 值.parse::<u32>().is_ok())
            .unwrap_or(false));
    }

    #[test]
    fn 命令替换展开变量() {
        // $(...) 里先用本 shell 的变量表展开，再交给 /bin/sh（sh 不认中文变量名）
        let mut 实例 = 壳::新建();
        let 状态 = 实例.运行脚本文本("甲=你好; 结果=\"$(echo ${甲})\"");
        assert_eq!(状态, 0);
        assert_eq!(实例.变量.get("结果").map(String::as_str), Some("你好"));
    }

    #[test]
    fn 返回值类型() {
        use super::super::值::值 as 运行时值;
        let mut 实例 = 壳::新建();

        // 数组
        let 状态 = 实例.运行脚本文本("函数 数组例\n    返回 (1 2 三)\n数组例");
        assert_eq!(状态, 0);
        assert_eq!(
            实例.返回值,
            运行时值::数组(vec![
                运行时值::整数(1),
                运行时值::整数(2),
                运行时值::文本("三".into()),
            ])
        );
        assert_eq!(
            实例.变量.get("返回值").map(String::as_str),
            Some("(1 2 三)")
        );

        // 对象（含嵌套数组），且对象只是数据，没有方法/类
        let 状态 = 实例.运行脚本文本("函数 对象例\n    返回 {甲=(2 3)}\n对象例");
        assert_eq!(状态, 0);
        assert_eq!(
            实例.返回值,
            运行时值::对象(vec![(
                "甲".into(),
                运行时值::数组(vec![运行时值::整数(2), 运行时值::整数(3)])
            )])
        );
    }
}
