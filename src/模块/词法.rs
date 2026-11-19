//! 词法/语法分析：把一行输入变成「作业」序列。
//!
//! 约定：在本 crate 内可自由命名的类型、函数、文件一律用中文；与操作系统、
//! libc、外部命令交互的名称（`fork`、`pipe`、`Command` 等）保持英文原名。
//!
//! 支持的语法：
//! - 管道 `|`、`|&`、`2>&1 |`
//! - 输出重定向 `>` `>>` `&>` `&>>`，错误重定向 `2>` `2>>` `2>&1`，输入 `<`
//! - 后台 `&`，命令分隔 `;` 与换行
//! - 单引号（完全原样）、双引号（仅 `$` `\` 可转义）、反斜杠转义、`#` 注释
//! - 执行前的 `~`、`$VAR` / `${VAR}` 展开与 `*` `?` `[...]` 通配展开

use std::fmt;

/// 一个可能带引号保护的词。`受保护` 表示其中含有带引号的字面内容，
/// 这样的词不参与通配符展开。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct 词 {
    pub 文本: String,
    /// 含有单引号内容：整词原样，不做任何展开
    pub 受保护: bool,
    /// 含有双引号内容：`$` 展开，但不做通配 / `~` 展开
    pub 双引号: bool,
}

/// 一个最简单的命令（可带前置变量赋值，如 `A=1 echo $A`）。
#[derive(Debug, Clone, Default)]
pub struct 简单命令 {
    /// 前置赋值 `名称=值`，只在外部命令时生效
    pub 赋值: Vec<(String, 词)>,
    pub 词列表: Vec<词>,
}

impl 简单命令 {
    /// 命令名（已展开的第一个词）；纯赋值时为空。
    pub fn 名称(&self) -> Option<&str> {
        self.词列表.first().map(|w| w.文本.as_str())
    }

    pub fn 空(&self) -> bool {
        self.赋值.is_empty() && self.词列表.is_empty()
    }
}

/// 管道相邻两段的连接方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 连接 {
    管道,
    /// 管道前的 `2>&1` 或 `|&`：stderr 也进入管道
    管道含错误,
}

#[derive(Debug, Clone, Default)]
pub struct 管道 {
    pub 命令: Vec<简单命令>,
    /// 第 i 项连接 `命令[i]` 与 `命令[i+1]`
    pub 连接: Vec<连接>,
}

/// 一条完整命令。
#[derive(Debug, Clone, Default)]
pub struct 作业 {
    pub 管道: 管道,
    /// stdout 重定向：`(目标, 是否追加)`
    pub 输出: Option<(词, bool)>,
    /// stderr 重定向
    pub 错误输出: Option<(词, bool)>,
    /// 输入重定向
    pub 输入: Option<词>,
    /// 当前管道之前/之中的命令写下了 `2>&1`
    pub 合并错误: bool,
    pub 后台: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum 语法错误 {
    未闭合引号(char),
    反斜杠结尾,
    缺少命令,
    缺少重定向目标,
    意外符号(String),
}

impl fmt::Display for 语法错误 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::未闭合引号(c) => write!(f, "引号 {c} 未闭合"),
            Self::反斜杠结尾 => write!(f, "行尾反斜杠没有可转义的字符"),
            Self::缺少命令 => write!(f, "缺少命令"),
            Self::缺少重定向目标 => write!(f, "重定向缺少目标文件"),
            Self::意外符号(s) => write!(f, "意外的符号 `{s}`"),
        }
    }
}

fn 是空白(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\r'
}

fn 是操作符(c: char) -> bool {
    matches!(c, '|' | '&' | ';' | '<' | '>' | '(' | ')')
}

struct 解析器<'a> {
    字符: &'a [char],
    位置: usize,
    当前词: Option<词>,
    /// 当前词中第一个未受引号保护的 `=` 的位置
    赋值等号: Option<usize>,
    当前命令: 简单命令,
    当前管道: 管道,
    作业列表: Vec<作业>,
    当前作业: 作业,
}

/// 把一行（或几行）输入解析为作业列表。
pub fn 解析命令(输入: &str) -> Result<Vec<作业>, 语法错误> {
    let 字符: Vec<char> = 输入.chars().collect();
    let mut 解析器 = 解析器 {
        字符: &字符,
        位置: 0,
        当前词: None,
        赋值等号: None,
        当前命令: 简单命令::default(),
        当前管道: 管道::default(),
        作业列表: Vec::new(),
        当前作业: 作业::default(),
    };
    解析器.运行()?;
    Ok(解析器.作业列表)
}

impl<'a> 解析器<'a> {
    fn 剩余(&self, 偏移: usize) -> Option<char> {
        self.字符.get(self.位置 + 偏移).copied()
    }

    fn 取词(&mut self) -> &mut 词 {
        self.当前词.get_or_insert_with(词::default)
    }

    fn 收词(&mut self) {
        let 等号 = self.赋值等号.take();
        if let Some(w) = self.当前词.take() {
            // 命令名尚未出现时的 `名称=值` 视为赋值前缀（`=` 须未被引号保护）
            let 位置 = 等号.filter(|_| self.当前命令.词列表.is_empty());
            if let Some(位置) = 位置 {
                let 名称 = &w.文本[..位置];
                let 合法 = !名称.is_empty()
                    && 名称.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
                    && 名称.chars().all(|c| c.is_alphanumeric() || c == '_');
                if 合法 {
                    // `=` 之后的值同样是 shell 参与展开的部分
                    let 值 = 词 {
                        文本: w.文本[位置 + 1..].to_string(),
                        受保护: w.受保护,
                        双引号: w.双引号,
                    };
                    self.当前命令.赋值.push((名称.to_string(), 值));
                    return;
                }
            }
            self.当前命令.词列表.push(w);
        }
    }

    fn 结束命令(&mut self) {
        self.收词();
        if self.当前命令.空() {
            return;
        }
        self.当前管道.命令.push(std::mem::take(&mut self.当前命令));
    }

    /// 结束当前作业并压入结果；`|` 之后没有命令时报「缺少命令」。
    fn 结束作业(&mut self) -> Result<(), 语法错误> {
        self.结束命令();
        if self.当前管道.命令.len() <= self.当前管道.连接.len() && !self.当前管道.命令.is_empty() {
            self.当前管道 = 管道::default();
            return Err(语法错误::缺少命令);
        }
        if !self.当前管道.命令.is_empty() {
            self.当前作业.管道 = std::mem::take(&mut self.当前管道);
            let 已完成 = std::mem::take(&mut self.当前作业);
            self.作业列表.push(已完成);
        }
        Ok(())
    }

    fn 读目标(&mut self) -> Result<词, 语法错误> {
        while self.剩余(0).is_some_and(是空白) {
            self.位置 += 1;
        }
        let mut 文本 = String::new();
        let mut 受保护 = false;
        let mut 双引号 = false;
        while let Some(c) = self.剩余(0) {
            if 是空白(c) || 是操作符(c) {
                break;
            }
            match c {
                '\\' => {
                    self.位置 += 1;
                    let Some(下一个) = self.剩余(0) else {
                        return Err(语法错误::反斜杠结尾);
                    };
                    文本.push(下一个);
                    受保护 = true;
                }
                '\'' | '"' => {
                    let 引号 = c;
                    self.位置 += 1;
                    let mut 闭合 = false;
                    while let Some(x) = self.剩余(0) {
                        self.位置 += 1;
                        if x == 引号 {
                            闭合 = true;
                            break;
                        }
                        if 引号 == '"' && x == '\\' {
                            if let Some(下一) = self.剩余(0) {
                                self.位置 += 1;
                                文本.push(下一);
                                continue;
                            }
                        }
                        文本.push(x);
                    }
                    if !闭合 {
                        return Err(语法错误::未闭合引号(引号));
                    }
                    if 引号 == '\'' {
                        受保护 = true;
                    } else {
                        双引号 = true;
                    }
                }
                _ => {
                    文本.push(c);
                    self.位置 += 1;
                }
            }
        }
        if 文本.is_empty() {
            return Err(语法错误::缺少重定向目标);
        }
        Ok(词 { 文本, 受保护, 双引号 })
    }

    fn 运行(&mut self) -> Result<(), 语法错误> {
        while let Some(c) = self.剩余(0) {
            match c {
                '\\' => {
                    self.位置 += 1;
                    match self.剩余(0) {
                        None => return Err(语法错误::反斜杠结尾),
                        Some('\n') => self.位置 += 1,
                        Some(下一个) => {
                            self.取词().文本.push(下一个);
                            self.位置 += 1;
                        }
                    }
                }
                '\'' | '"' => self.读引号(c)?,
                c if 是空白(c) => {
                    self.收词();
                    self.位置 += 1;
                }
                '\n' => {
                    self.位置 += 1;
                    self.结束作业()?;
                }
                '#' if self.当前词.is_none() => {
                    while self.剩余(0).is_some_and(|x| x != '\n') {
                        self.位置 += 1;
                    }
                }
                ';' => {
                    self.位置 += 1;
                    self.结束作业()?;
                }
                '|' => self.读管道()?,
                // `&>` / `&>>`：stdout 与 stderr 一起重定向
                '&' if self.剩余(1) == Some('>') => {
                    let 追加 = self.剩余(2) == Some('>');
                    self.位置 += if 追加 { 3 } else { 2 };
                    let 目标 = self.读目标()?;
                    self.当前作业.输出 = Some((目标.clone(), 追加));
                    self.当前作业.错误输出 = Some((目标, 追加));
                }
                // `|&` 里 `|` 的后面那个 `&` 不该被当成后台符号
                '&' if self.当前管道.命令.len() > self.当前管道.连接.len() => {}
                '&' => {
                    self.收词();
                    if self.当前命令.空() {
                        return Err(语法错误::缺少命令);
                    }
                    self.位置 += 1;
                    self.结束命令();
                    self.当前作业.后台 = true;
                    self.结束作业()?;
                }
                '<' => {
                    self.收词();
                    self.位置 += 1;
                    let 目标 = self.读目标()?;
                    self.当前作业.输入 = Some(目标);
                }
                '>' => {
                    self.收词();
                    let 追加 = self.剩余(1) == Some('>');
                    self.位置 += if 追加 { 2 } else { 1 };
                    let 目标 = self.读目标()?;
                    self.当前作业.输出 = Some((目标, 追加));
                }
                '2' if self.当前词.is_none() && self.剩余(1) == Some('>') => {
                    self.读错误重定向()?;
                }
                '(' | ')' => return Err(语法错误::意外符号(c.to_string())),
                _ => {
                    if c == '=' && self.赋值等号.is_none() {
                        self.赋值等号 = Some(self.当前词.as_ref().map_or(0, |w| w.文本.chars().count()));
                    }
                    self.取词().文本.push(c);
                    self.位置 += 1;
                }
            }
        }
        self.结束作业()
    }

    /// 解析 `2>` `2>>` `2>&1`。只在此处消费 `2`，不会吞掉以 `2` 开头的普通参数。
    fn 读错误重定向(&mut self) -> Result<(), 语法错误> {
        if self.剩余(2) == Some('&') && self.剩余(3) == Some('1') {
            self.位置 += 4;
            self.合并错误输出();
            return Ok(());
        }
        let 追加 = self.剩余(2) == Some('>');
        self.位置 += if 追加 { 3 } else { 2 };
        let 目标 = self.读目标()?;
        self.当前作业.错误输出 = Some((目标, 追加));
        Ok(())
    }

    /// 处理 `|`、`|&`。`|` 左侧的 `2>&1` 已经由 `>` 分支记在「作业级」上。
    fn 读管道(&mut self) -> Result<(), 语法错误> {
        // 先把管道左侧的命令收好
        self.收词();
        if self.当前命令.空() && self.当前管道.命令.is_empty() {
            return Err(语法错误::缺少命令);
        }
        self.当前管道.命令.push(std::mem::take(&mut self.当前命令));

        self.位置 += 1;
        let 连接方式 = if self.剩余(0) == Some('&') {
            self.位置 += 1;
            连接::管道含错误
        } else {
            连接::管道
        };
        self.当前管道.连接.push(连接方式);
        Ok(())
    }

    /// `2>&1`：stderr 跟随 stdout。
    fn 合并错误输出(&mut self) {
        self.当前作业.合并错误 = true;
    }

    fn 读引号(&mut self, 引号: char) -> Result<(), 语法错误> {
        self.位置 += 1;
        while let Some(c) = self.剩余(0) {
            self.位置 += 1;
            if c == 引号 {
                let 词 = self.取词();
                if 引号 == '\'' {
                    词.受保护 = true;
                } else {
                    词.双引号 = true;
                }
                return Ok(());
            }
            if 引号 == '"' && c == '\\' {
                match self.剩余(0) {
                    Some(下一 @ ('$' | '`' | '"' | '\\')) => {
                        self.位置 += 1;
                        self.取词().文本.push(下一);
                    }
                    Some('\n') => self.位置 += 1,
                    Some(其他) => self.保留反斜杠(其他),
                    None => return Err(语法错误::反斜杠结尾),
                }
                continue;
            }
            self.取词().文本.push(c);
        }
        Err(语法错误::未闭合引号(引号))
    }

    fn 保留反斜杠(&mut self, c: char) {
        let 词 = self.取词();
        词.文本.push('\\');
        词.文本.push(c);
    }
}

/// 执行前的展开：`~`、`$VAR`、`${VAR}` 与通配符。
/// 有双引号的词只展开变量、不展开通配与 `~`；有单引号的词原样保留。
/// `查变量` 用于解析变量与 `$HOME`。
pub fn 展开词(字: &词, 查变量: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let 文本 = 展开变量(字, 查变量);
    if !字.受保护 && !字.双引号 {
        if let Some(列表) = 通配(&文本) {
            return 列表;
        }
    }
    vec![文本]
}

/// 变量与 `~` 展开（不做通配）。
pub fn 展开变量(字: &词, 查变量: &dyn Fn(&str) -> Option<String>) -> String {
    if 字.受保护 {
        // 带引号的词按原样交给程序（引号里的 `$` 不展开）
        return 字.文本.clone();
    }
    let 字符: Vec<char> = 字.文本.chars().collect();
    let mut 结果 = String::new();
    let mut i = 0;
    while i < 字符.len() {
        if 字符[i] == '$' {
            match 字符.get(i + 1) {
                Some('{') => {
                    let 起点 = i + 2;
                    match 字符[起点..].iter().position(|&x| x == '}') {
                        Some(偏移) => {
                            let 名称: String = 字符[起点..起点 + 偏移].iter().collect();
                            结果.push_str(&查变量(&名称).unwrap_or_default());
                            i = 起点 + 偏移 + 1;
                        }
                        None => {
                            结果.push('$');
                            i += 1;
                        }
                    }
                }
                Some(c) if c.is_alphanumeric() || *c == '_' => {
                    let 起点 = i + 1;
                    let mut 终点 = 起点;
                    while 终点 < 字符.len()
                        && (字符[终点].is_alphanumeric() || 字符[终点] == '_')
                    {
                        终点 += 1;
                    }
                    let 名称: String = 字符[起点..终点].iter().collect();
                    结果.push_str(&查变量(&名称).unwrap_or_default());
                    i = 终点;
                }
                _ => {
                    结果.push('$');
                    i += 1;
                }
            }
        } else {
            结果.push(字符[i]);
            i += 1;
        }
    }
    // `~` 只在词首、且没有引号时展开
    if !字.受保护 && !字.双引号 {
        if let Some(家) = 查变量("HOME") {
            if 结果 == "~" {
                return 家;
            }
            if let Some(余) = 结果.strip_prefix("~/") {
                return format!("{家}/{余}");
            }
        }
    }
    结果
}

/// 通配符展开；没有匹配项时返回 None（保留原样）。
pub fn 通配(模式: &str) -> Option<Vec<String>> {
    if !模式.contains(['*', '?', '[']) {
        return None;
    }
    let (目录, 文件名模式) = match 模式.rfind('/') {
        Some(位置) => (&模式[..=位置], &模式[位置 + 1..]),
        None => ("", 模式),
    };
    let 读目录 = if 目录.is_empty() { "." } else { 目录 };
    let mut 结果: Vec<String> = std::fs::read_dir(读目录)
        .ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|名| !名.starts_with('.') || 文件名模式.starts_with('.'))
        .filter(|名| 匹配(文件名模式, 名))
        .map(|名| format!("{目录}{名}"))
        .collect();
    结果.sort();
    if 结果.is_empty() { None } else { Some(结果) }
}

/// shell 风格模式匹配：`*` 任意串、`?` 单字符、`[...]` 字符类（支持 `a-z`、`!`/`^` 取反）。
pub fn 匹配(模式: &str, 文本: &str) -> bool {
    let 模式: Vec<char> = 模式.chars().collect();
    let 文本: Vec<char> = 文本.chars().collect();
    let (mut p, mut t) = (0usize, 0usize);
    let mut 回退: Option<(usize, usize)> = None;
    while t < 文本.len() {
        let 可行 = p < 模式.len()
            && match 模式[p] {
                '*' => {
                    回退 = Some((p, t));
                    p += 1;
                    continue;
                }
                '?' => true,
                '[' => match 字符类匹配(&模式[p..], 文本[t]) {
                    Some((命中, 长度)) => {
                        if 命中 {
                            p += 长度;
                            t += 1;
                            continue;
                        }
                        false
                    }
                    None => 模式[p] == 文本[t],
                },
                c => c == 文本[t],
            };
        if 可行 {
            p += 1;
            t += 1;
        } else if let Some((星, 起点)) = 回退 {
            p = 星 + 1;
            t = 起点 + 1;
            回退 = Some((星, 起点 + 1));
        } else {
            return false;
        }
    }
    while p < 模式.len() && 模式[p] == '*' {
        p += 1;
    }
    p == 模式.len()
}

/// 解析 `[...]`；返回（是否命中，字符类总长）。不像字符类则返回 None。
fn 字符类匹配(模式: &[char], 字符: char) -> Option<(bool, usize)> {
    let mut i = 1;
    let mut 取反 = false;
    if matches!(模式.get(i), Some('!') | Some('^')) {
        取反 = true;
        i += 1;
    }
    let mut 命中 = false;
    let mut 首个 = true;
    while i < 模式.len() {
        if 模式[i] == ']' && !首个 {
            return Some((命中 != 取反, i + 1));
        }
        首个 = false;
        if i + 2 < 模式.len() && 模式[i + 1] == '-' && 模式[i + 2] != ']' {
            if 模式[i] <= 字符 && 字符 <= 模式[i + 2] {
                命中 = true;
            }
            i += 3;
        } else {
            if 模式[i] == 字符 {
                命中 = true;
            }
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod 测试 {
    use super::*;

    fn 变量表(键: &str) -> Option<String> {
        match 键 {
            "HOME" => Some("/home/u".into()),
            "名字" => Some("世界".into()),
            _ => None,
        }
    }

    fn 展开全部(命令: &简单命令) -> Vec<String> {
        命令
            .词列表
            .iter()
            .flat_map(|w| 展开词(w, &变量表))
            .collect()
    }

    #[test]
    fn 基本分词与引号() {
        // 单引号：变量不展开；双引号：变量展开、空白不切分
        let 作业 = 解析命令(r#"echo 你好 "a b" 'c $d' "$名字""#).unwrap();
        assert_eq!(作业.len(), 1);
        assert_eq!(展开全部(&作业[0].管道.命令[0]), ["echo", "你好", "a b", "c $d", "世界"]);
    }

    #[test]
    fn 管道与重定向() {
        let 作业 = 解析命令("ls -l | grep rust > /tmp/x 2>&1").unwrap();
        let j = &作业[0];
        assert_eq!(j.管道.命令.len(), 2);
        assert_eq!(j.输出.as_ref().unwrap().0.文本, "/tmp/x");
        assert!(j.合并错误);
        assert!(j.错误输出.is_none());
    }

    #[test]
    fn 错误重定向() {
        let 作业 = 解析命令("cmd 2>>err.log <in.txt").unwrap();
        let j = &作业[0];
        assert_eq!(j.错误输出.as_ref().unwrap().0.文本, "err.log");
        assert!(j.错误输出.as_ref().unwrap().1);
        assert_eq!(j.输入.as_ref().unwrap().文本, "in.txt");

        // `&>` 同时重定向 stdout 与 stderr；之后的 `2>err.log` 覆盖 stderr
        let 作业 = 解析命令("cmd &>both.log 2>err.log").unwrap();
        let j = &作业[0];
        assert_eq!(j.输出.as_ref().unwrap().0.文本, "both.log");
        assert_eq!(j.错误输出.as_ref().unwrap().0.文本, "err.log");

        // `2` 只在词首且紧跟 `>` 时才被当成 fd 前缀
        let 作业 = 解析命令("echo 2-3").unwrap();
        assert_eq!(展开全部(&作业[0].管道.命令[0]), ["echo", "2-3"]);
    }

    #[test]
    fn 后台与分号() {
        let 作业 = 解析命令("sleep 1 & echo hi; pwd").unwrap();
        assert_eq!(作业.len(), 3);
        assert!(作业[0].后台);
        assert!(!作业[1].后台);
    }

    #[test]
    fn 变量与波浪号() {
        let 作业 = 解析命令("echo $名字 ~/x ${HOME} $未知").unwrap();
        assert_eq!(展开全部(&作业[0].管道.命令[0]), ["echo", "世界", "/home/u/x", "/home/u", ""]);
    }

    #[test]
    fn 前置赋值() {
        let 作业 = 解析命令("A=1 B=\"x y\" echo hi").unwrap();
        let 命令 = &作业[0].管道.命令[0];
        assert_eq!(命令.赋值.len(), 2);
        assert_eq!(命令.赋值[1].1.文本, "x y");
        assert_eq!(展开全部(命令), ["echo", "hi"]);
    }

    #[test]
    fn 未闭合引号() {
        assert_eq!(解析命令("echo 'abc").unwrap_err(), 语法错误::未闭合引号('\''));
        assert_eq!(解析命令("echo \"abc").unwrap_err(), 语法错误::未闭合引号('"'));
    }

    #[test]
    fn 空管道报错() {
        assert_eq!(解析命令("echo a |").unwrap_err(), 语法错误::缺少命令);
        assert_eq!(解析命令("| echo").unwrap_err(), 语法错误::缺少命令);
    }

    #[test]
    fn 通配匹配() {
        assert!(匹配("*.rs", "main.rs"));
        assert!(!匹配("*.rs", "main.py"));
        assert!(匹配("a?c", "abc"));
        assert!(匹配("[a-c]*", "banana"));
        assert!(匹配("[!a-c]*", "dog"));
        assert!(!匹配("[!a-c]*", "cat"));
        assert!(匹配("中*", "中文"));
        assert!(匹配("*", ""));
    }

    #[test]
    fn 注释与续行() {
        let 作业 = 解析命令("echo hi # 注释\npwd").unwrap();
        assert_eq!(作业.len(), 2);
        let 作业 = 解析命令("echo \\\nhi").unwrap();
        assert_eq!(作业.len(), 1);
    }

    #[test]
    fn 管道与后台不冲突() {
        let 作业 = 解析命令("sleep 1 & echo |& cat").unwrap();
        assert_eq!(作业.len(), 2);
        assert!(作业[0].后台);
        assert_eq!(作业[1].管道.连接, [连接::管道含错误]);

        let 作业 = 解析命令("echo a &> 全部.txt").unwrap();
        assert!(!作业[0].后台);
        assert_eq!(作业[0].输出.as_ref().unwrap().0.文本, "全部.txt");
        assert_eq!(作业[0].错误输出.as_ref().unwrap().0.文本, "全部.txt");
    }

    #[test]
    fn 管道与错误() {
        let 作业 = 解析命令("make |& tee log").unwrap();
        assert_eq!(作业[0].管道.连接, [连接::管道含错误]);
        // `2>&1 |` 让左段的 stderr 也进管道
        let 作业 = 解析命令("make 2>&1 | tee log").unwrap();
        assert!(作业[0].合并错误);
        // 注意：`2>&1 |` 中的 `2>&1` 属于「作业级」合并，`|` 本身仍是普通管道
        assert_eq!(作业[0].管道.连接, [连接::管道]);
        let 作业 = 解析命令("make 2>&1").unwrap();
        assert!(作业[0].合并错误);
        assert!(作业[0].管道.连接.is_empty());
    }
}
