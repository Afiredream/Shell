//! 释义层：把 [`super::词法::作业`] 翻译成可以直接交给执行层的 [`super::执行::管道片段`]。
//!
//! 这里负责三件事：
//! 1. 变量 / `~` / 通配 展开；
//! 2. 前置赋值 `A=1 cmd` 拆分为「只给子进程的环境变量」；
//! 3. 标出哪些命令是内置命令（单独出现时由 shell 亲自动手执行）。

use super::内置;
use super::执行::{简易命令, 管道片段, 重定向};
use super::系统::整数;
use super::词法::{作业, 展开词, 词, 连接方式};

/// 一条输入行释义后的结果。
pub enum 释义 {
    /// 空输入（只有注释或空白）
    空,
    /// 只是赋值：`A=1 B=2`，作用于 shell 自身
    赋值(Vec<(String, String)>),
    /// 一段可执行的管道
    管道(管道片段),
}

pub struct 释义器<'a> {
    /// shell 自己的变量表（优先于进程环境）
    pub 环境表: &'a std::collections::HashMap<String, String>,
    /// 上一条命令的退出状态，供 `$?` 使用
    pub 最后状态: 整数,
    /// `$(命令)` 的执行器：返回命令的标准输出
    #[allow(clippy::type_complexity)]
    pub 命令替换: &'a dyn Fn(&str, &dyn Fn(&str) -> Option<String>) -> String,
}

impl<'a> 释义器<'a> {
    fn 查变量(&self, 名称: &str) -> Option<String> {
        match 名称 {
            "?" => return Some(self.最后状态.to_string()),
            "$" => return Some(std::process::id().to_string()),
            "!" => return Some(super::执行::最后后台进程().to_string()),
            "#" => return Some(self.参数个数().to_string()),
            "@" | "*" => return Some(self.参数列表().join(" ")),
            // 壳语言默认变量：位置参数按 `--`/`-` 分隔后再取值
            "参数" => return Some(self.参数列表().into_iter().next().unwrap_or_default()),
            "参数列表" => return Some(self.参数列表().join(" ")),
            "参数个数" => return Some(self.参数个数().to_string()),
            "参数对象" => return Some(self.参数对象().写JSON()),
            // 运行时再取的全局变量：目录会随 `进入` 变化，脚本位置随 `导入` 嵌套变化
            "当前目录" => {
                return Some(match std::env::current_dir() {
                    Ok(目录) => 目录.display().to_string(),
                    Err(_) => self.环境表.get("PWD").cloned().unwrap_or_default(),
                })
            }
            "脚本路径" => return Some(self.环境表.get("脚本路径").cloned().unwrap_or_default()),
            "脚本目录" => return Some(self.环境表.get("脚本目录").cloned().unwrap_or_default()),
            _ => {}
        }
        if let Some(值) = self.环境表.get(名称) {
            return Some(值.clone());
        }
        std::env::var(名称).ok()
    }

    /// 位置参数：按序号排好的 `$1`、`$2`……（给 `$@`、`$#`、`$参数列表` 用）。
    fn 参数列表(&self) -> Vec<String> {
        self.位置参数()
    }

    /// `$#` 与 `$参数个数` 共用。
    fn 参数个数(&self) -> usize {
        self.位置参数().len()
    }

    /// `$参数对象`：标准参数解析的结果，形如
    /// `{"命令行":["a"],"选项":{"v":"true","n":"3"},"分隔符":"--","剩余":["x"]}`。
    ///
    /// - `选项.名称` 存去掉全部前导 `-` 后的选项名，有 `=数据` 时取数据，否则为 `"true"`；
    /// - `-` 是「标准输入」的惯用写法，记为 `选项.-  = "true"`；
    /// - 第一个 `--` 起的内容进 `剩余`，`分隔符` 记为 `"--"`；
    /// - 单独的 `--选项` 后面紧跟一个不以 `-` 开头、且不是 `--` 的词时，把该词当作它的数据。
    fn 参数对象(&self) -> super::值::值 {
        use super::值::值 as 运行值;
        let 文本 = |项: &str| 运行值::文本(项.to_string());

        let 参数 = self.位置参数();
        let mut 命令行 = Vec::new();
        let mut 选项: Vec<(String, 运行值)> = Vec::new();
        let mut 分隔符 = String::new();
        let mut 剩余 = Vec::new();

        let mut 序 = 0;
        while 序 < 参数.len() {
            let 某项 = 参数[序].as_str();
            序 += 1;
            if 某项 == "--" {
                分隔符 = "--".to_string();
                剩余.extend(参数[序..].iter().map(|项| 文本(项)));
                break;
            }
            if 某项 == "-" {
                选项.push(("-".to_string(), 文本("true")));
                continue;
            }
            let Some(体) = 某项.strip_prefix("--").or_else(|| 某项.strip_prefix('-')) else {
                命令行.push(文本(某项));
                continue;
            };
            match 体.split_once('=') {
                Some((名, 数据)) => 选项.push((名.to_string(), 文本(数据))),
                None if 体.is_empty() => 命令行.push(文本(某项)),
                None => {
                    let 纯选项 = !某项.starts_with("--");
                    let 下个是本选项的数据 = 纯选项
                        && 参数.get(序).is_some_and(|下个| !下个.starts_with('-'));
                    let 选项数据 = if 下个是本选项的数据 {
                        序 += 1;
                        参数[序 - 1].clone()
                    } else {
                        "true".to_string()
                    };
                    选项.push((体.to_string(), 文本(&选项数据)));
                }
            }
        }

        运行值::对象(vec![
            ("命令行".to_string(), 运行值::数组(命令行)),
            ("选项".to_string(), 运行值::对象(选项)),
            ("分隔符".to_string(), 文本(&分隔符)),
            ("剩余".to_string(), 运行值::数组(剩余)),
        ])
    }

    /// 数字键（除 `0`）按序收集的位置参数。
    fn 位置参数(&self) -> Vec<String> {
        let mut 项: Vec<(u32, String)> = self
            .环境表
            .iter()
            .filter_map(|(键, 值)| {
                if 键 != "0" && !键.is_empty() && 键.bytes().all(|字| 字.is_ascii_digit()) {
                    键.parse::<u32>().ok().map(|序| (序, 值.clone()))
                } else {
                    None
                }
            })
            .collect();
        项.sort_by_key(|(序, _)| *序);
        项.into_iter().map(|(_, 值)| 值).collect()
    }

    /// 展开一个词为若干实参（变量 / `~` / 通配）。
    pub fn 展开(&self, 字: &词) -> Vec<String> {
        let 查变量 = |名称: &str| self.查变量(名称);
        展开词(字, &查变量, self.命令替换)
    }

    /// 变量赋值 / 重定向目标只取一个值：空展开不能被丢掉，否则 `> $未定义` 会变成无目标。
    fn 展开拼接(&self, 字: &词) -> String {
        self.展开(字).join(" ")
    }

    /// 把一条简单命令展开成实参列表（每个词可能展开成多个文件）。
    fn 实参列表(&self, 词列表: &[词]) -> Vec<String> {
        词列表.iter().flat_map(|字| self.展开(字)).collect()
    }

    pub fn 释义(&self, 待释义作业: &作业) -> Result<释义, String> {
        let 命令们 = &待释义作业.管道.命令;
        if 命令们.is_empty() {
            return Ok(释义::空);
        }

        // 只有赋值、没有命令：修改 shell 自身环境
        if 命令们.len() == 1 && 命令们[0].词列表.is_empty() {
            let 赋值列表 = 命令们[0]
                .赋值
                .iter()
                .map(|(名, 值)| (名.clone(), self.展开拼接(值)))
                .collect();
            return Ok(释义::赋值(赋值列表));
        }

        let mut 命令列表: Vec<简易命令> = Vec::with_capacity(命令们.len());
        let mut 错误进管道: Vec<bool> = Vec::with_capacity(命令们.len());

        for (序号, 简单命令) in 命令们.iter().enumerate() {
            let 参数列表 = self.实参列表(&简单命令.词列表);
            if 参数列表.is_empty() {
                return Err("展开后没有可执行的命令".into());
            }
            // 内置命令用在管道/后台里时，由执行层起子进程跑内置实现
            let 名称 = 简单命令.名称().unwrap_or_default().to_string();
            let 会进子进程 = 命令们.len() > 1 || 待释义作业.后台;
            let 是内置命令 = 会进子进程 && 内置::按名称查找(&名称).is_some();

            // 前置赋值 `A=1 cmd`：只进入子进程的「环境覆盖」
            let 环境 = 简单命令
                .赋值
                .iter()
                .map(|(名, 值)| (名.clone(), self.展开拼接(值)))
                .collect();

            命令列表.push(简易命令 {
                参数列表,
                环境,
                内置: 是内置命令,
            });

            if 序号 + 1 < 命令们.len() {
                let 链 = 待释义作业
                    .管道
                    .连接
                    .get(序号)
                    .copied()
                    .unwrap_or(连接方式::管道);
                错误进管道.push(链 == 连接方式::管道含错误);
            }
        }

        Ok(释义::管道(管道片段 {
            命令: 命令列表,
            错误进管道,
            重定向: 重定向 {
                输入: 待释义作业.输入.as_ref().map(|字| self.展开拼接(字)),
                输出: 待释义作业
                    .输出
                    .as_ref()
                    .map(|(字, 追加)| (self.展开拼接(字), *追加)),
                错误输出: 待释义作业
                    .错误输出
                    .as_ref()
                    .map(|(字, 追加)| (self.展开拼接(字), *追加)),
                合并错误: 待释义作业.合并错误,
            },
            后台: 待释义作业.后台,
            输入来自文件: 待释义作业.输入.is_some(),
        }))
    }
}
