//! 释义层：把 [`super::词法::作业`] 翻译成可以直接交给执行层的 [`super::执行::管道片段`]。
//!
//! 这里负责三件事：
//! 1. 变量 / `~` / 通配 展开；
//! 2. 前置赋值 `A=1 cmd` 拆分为「只给子进程的环境变量」；
//! 3. 标出哪些命令是内置命令（单独出现时由 shell 亲自动手执行）。

use super::执行::{重定向, 简易命令, 管道片段};
use super::系统::整数;
use super::内置;
use super::词法::{连接方式, 作业, 词, 展开词};

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
    pub 环境表: &'a [(String, String)],
    /// 上一条命令的退出状态，供 `$?` 使用
    pub 最后状态: 整数,
}

impl<'a> 释义器<'a> {
    fn 查变量(&self, 名称: &str) -> Option<String> {
        if 名称 == "?" {
            return Some(self.最后状态.to_string());
        }
        if let Some((_, 值)) = self.环境表.iter().find(|(键, _)| 键 == 名称) {
            return Some(值.clone());
        }
        std::env::var(名称).ok()
    }

    /// 展开一个词为若干实参（变量 / `~` / 通配）。
    fn 展开(&self, 字: &词) -> Vec<String> {
        let 查变量 = |名称: &str| self.查变量(名称);
        展开词(字, &查变量)
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

            命令列表.push(简易命令 { 参数列表, 环境, 内置: 是内置命令 });

            if 序号 + 1 < 命令们.len() {
                let 链 = 待释义作业.管道.连接.get(序号).copied().unwrap_or(连接方式::管道);
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
        }))
    }
}
