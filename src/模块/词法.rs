//! 词法/语法分析：把一行输入变成「作业」序列，以及执行前的各种展开。
//!
//! 约定：Rust 自己起名的类型、函数、变量、文件名一律用中文；
//! 不翻译的只有三类——语言关键字与标准库接口、外部命令名/命令行选项、
//! 以及与 C 库 ABI 绑定的符号名和常量（它们不是本项目的命名）。
//!
//! 支持的语法：
//! - 管道 `|`、`|&`、`2>&1 |`
//! - 输出重定向 `>` `>>` `&>` `&>>`，错误重定向 `2>` `2>>` `2>&1`，输入 `<`
//! - 后台 `&`，命令分隔 `;` 与换行
//! - 单引号（完全原样）、双引号（仅 `$` `\` 可转义）、反斜杠转义、`#` 注释
//! - 执行前的 `~`、`$变量` / `${变量}` 展开与 `*` `?` `[...]` 通配展开

use std::fmt;

/// 一个词。引号只决定「怎么展开」，引号本身不会进入 `文本`。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct 词 {
    pub 文本: String,
    /// 含有单引号内容：整词原样，不做任何展开
    pub 含单引号: bool,
    /// 含有双引号内容：`$` 展开，但不做通配 / `~` 展开
    pub 含双引号: bool,
}

/// 一个最简单的命令（可带前置变量赋值，如 `A=1 echo $A`）。
#[derive(Debug, Clone, Default)]
pub struct 简单命令 {
    /// 前置赋值 `名称=值`，只对该条命令（子进程）生效
    pub 赋值: Vec<(String, 词)>,
    pub 词列表: Vec<词>,
}

impl 简单命令 {
    /// 命令名（尚未展开的第一个词）；纯赋值时为空。
    pub fn 名称(&self) -> Option<&str> {
        self.词列表.first().map(|单字| 单字.文本.as_str())
    }

    pub fn 是否为空(&self) -> bool {
        self.赋值.is_empty() && self.词列表.is_empty()
    }
}

/// 管道相邻两段的连接方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 连接方式 {
    管道,
    /// `|&`：前一段的标准错误也进入管道
    管道含错误,
}

#[derive(Debug, Clone, Default)]
pub struct 管道 {
    pub 命令: Vec<简单命令>,
    /// 第 i 项连接 `命令[i]` 与 `命令[i+1]`
    pub 连接: Vec<连接方式>,
}

/// 一条完整命令。
#[derive(Debug, Clone, Default)]
pub struct 作业 {
    pub 管道: 管道,
    /// 标准输出重定向：`(目标, 是否追加)`
    pub 输出: Option<(词, bool)>,
    /// 标准错误重定向
    pub 错误输出: Option<(词, bool)>,
    /// 标准输入重定向
    pub 输入: Option<词>,
    /// 命令里写过 `2>&1`
    pub 合并错误: bool,
    pub 后台: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum 语法错误 {
    引号未闭合(char),
    反斜杠在行尾,
    缺少命令,
    重定向缺少目标,
    意外符号(String),
}

impl fmt::Display for 语法错误 {
    fn fmt(&self, 格式: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::引号未闭合(引号) => write!(格式, "引号 {引号} 未闭合"),
            Self::反斜杠在行尾 => write!(格式, "行尾反斜杠没有可转义的字符"),
            Self::缺少命令 => write!(格式, "缺少命令"),
            Self::重定向缺少目标 => write!(格式, "重定向缺少目标文件"),
            Self::意外符号(符号) => write!(格式, "意外的符号 `{符号}`"),
        }
    }
}

fn 是空白(字符: char) -> bool {
    字符 == ' ' || 字符 == '\t' || 字符 == '\r'
}

fn 是操作符(字符: char) -> bool {
    matches!(字符, '|' | '&' | ';' | '<' | '>' | '(' | ')')
}

struct 解析器<'a> {
    字符表: &'a [char],
    当前位置: usize,
    当前词: Option<词>,
    /// 当前词中第一个未受引号保护的 `=` 的位置（按字符计）
    赋值等号位置: Option<usize>,
    当前命令: 简单命令,
    当前管道: 管道,
    作业列表: Vec<作业>,
    当前作业: 作业,
}

/// 把一行（或几行）输入解析为作业列表。
pub fn 解析命令(输入: &str) -> Result<Vec<作业>, 语法错误> {
    let 字符表: Vec<char> = 输入.chars().collect();
    let mut 解析器 = 解析器 {
        字符表: &字符表,
        当前位置: 0,
        当前词: None,
        赋值等号位置: None,
        当前命令: 简单命令::default(),
        当前管道: 管道::default(),
        作业列表: Vec::new(),
        当前作业: 作业::default(),
    };
    解析器.运行()?;
    Ok(解析器.作业列表)
}

impl<'a> 解析器<'a> {
    fn 预看(&self, 偏移: usize) -> Option<char> {
        self.字符表.get(self.当前位置 + 偏移).copied()
    }

    fn 取当前词(&mut self) -> &mut 词 {
        self.当前词.get_or_insert_with(词::default)
    }

    /// 把正在拼的词收进命令；命令名之前形如 `名称=值` 的词视为赋值前缀。
    fn 收集词(&mut self) {
        let 等号位置 = self.赋值等号位置.take();
        let Some(正在拼的词) = self.当前词.take() else {
            return;
        };
        if let Some(位置) = 等号位置.filter(|_| self.当前命令.词列表.is_empty()) {
            let 名称 = &正在拼的词.文本[..位置];
            let 名称合法 = !名称.is_empty()
                && 名称.chars().next().is_some_and(|首字| 首字.is_alphabetic() || 首字 == '_')
                && 名称.chars().all(|字符| 字符.is_alphanumeric() || 字符 == '_');
            if 名称合法 {
                let 值 = 词 {
                    文本: 正在拼的词.文本[位置 + 1..].to_string(),
                    含单引号: 正在拼的词.含单引号,
                    含双引号: 正在拼的词.含双引号,
                };
                self.当前命令.赋值.push((名称.to_string(), 值));
                return;
            }
        }
        self.当前命令.词列表.push(正在拼的词);
    }

    fn 结束命令(&mut self) {
        self.收集词();
        if self.当前命令.是否为空() {
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
            let 已完成作业 = std::mem::take(&mut self.当前作业);
            self.作业列表.push(已完成作业);
        }
        Ok(())
    }

    /// 读取重定向的目标文件名（支持引号与反斜杠转义）。
    fn 读重定向目标(&mut self) -> Result<词, 语法错误> {
        while self.预看(0).is_some_and(是空白) {
            self.当前位置 += 1;
        }
        let mut 文本 = String::new();
        let mut 含单引号 = false;
        let mut 含双引号 = false;
        while let Some(字符) = self.预看(0) {
            if 是空白(字符) || 是操作符(字符) {
                break;
            }
            match 字符 {
                '\\' => {
                    self.当前位置 += 1;
                    let Some(下一个) = self.预看(0) else {
                        return Err(语法错误::反斜杠在行尾);
                    };
                    文本.push(下一个);
                    含单引号 = true;
                }
                '\'' | '"' => {
                    let 引号 = 字符;
                    self.当前位置 += 1;
                    let mut 已闭合 = false;
                    while let Some(字符) = self.预看(0) {
                        self.当前位置 += 1;
                        if 字符 == 引号 {
                            已闭合 = true;
                            break;
                        }
                        if 引号 == '"' && 字符 == '\\' {
                            if let Some(下一个) = self.预看(0) {
                                self.当前位置 += 1;
                                文本.push(下一个);
                                continue;
                            }
                        }
                        文本.push(字符);
                    }
                    if !已闭合 {
                        return Err(语法错误::引号未闭合(引号));
                    }
                    if 引号 == '\'' {
                        含单引号 = true;
                    } else {
                        含双引号 = true;
                    }
                }
                _ => {
                    文本.push(字符);
                    self.当前位置 += 1;
                }
            }
        }
        if 文本.is_empty() {
            return Err(语法错误::重定向缺少目标);
        }
        Ok(词 { 文本, 含单引号, 含双引号 })
    }

    fn 运行(&mut self) -> Result<(), 语法错误> {
        while let Some(字符) = self.预看(0) {
            match 字符 {
                '\\' => {
                    self.当前位置 += 1;
                    match self.预看(0) {
                        None => return Err(语法错误::反斜杠在行尾),
                        Some('\n') => self.当前位置 += 1,
                        Some(下一个) => {
                            self.取当前词().文本.push(下一个);
                            self.当前位置 += 1;
                        }
                    }
                }
                '\'' | '"' => self.读引号(字符)?,
                _ if 是空白(字符) => {
                    self.收集词();
                    self.当前位置 += 1;
                }
                '\n' => {
                    self.当前位置 += 1;
                    self.结束作业()?;
                }
                // `#` 在词的开头位置才视为注释
                '#' if self.当前词.is_none() => {
                    while self.预看(0).is_some_and(|字符| 字符 != '\n') {
                        self.当前位置 += 1;
                    }
                }
                ';' => {
                    self.当前位置 += 1;
                    self.结束作业()?;
                }
                '|' => self.读管道()?,
                // `&>` / `&>>`：标准输出与标准错误一起重定向
                '&' if self.预看(1) == Some('>') => {
                    let 追加 = self.预看(2) == Some('>');
                    self.当前位置 += if 追加 { 3 } else { 2 };
                    let 目标 = self.读重定向目标()?;
                    self.当前作业.输出 = Some((目标.clone(), 追加));
                    self.当前作业.错误输出 = Some((目标, 追加));
                }
                // `|&` 里 `|` 的后面那个 `&` 不该被当成后台符号
                '&' if self.当前管道.命令.len() > self.当前管道.连接.len() => {}
                '&' => {
                    self.收集词();
                    if self.当前命令.是否为空() {
                        return Err(语法错误::缺少命令);
                    }
                    self.当前位置 += 1;
                    self.结束命令();
                    self.当前作业.后台 = true;
                    self.结束作业()?;
                }
                '<' => {
                    self.收集词();
                    self.当前位置 += 1;
                    let 目标 = self.读重定向目标()?;
                    self.当前作业.输入 = Some(目标);
                }
                '>' => {
                    self.收集词();
                    let 追加 = self.预看(1) == Some('>');
                    self.当前位置 += if 追加 { 2 } else { 1 };
                    let 目标 = self.读重定向目标()?;
                    self.当前作业.输出 = Some((目标, 追加));
                }
                // 只在此处消费 `2`，不会吞掉以 `2` 开头的普通参数
                '2' if self.当前词.is_none() && self.预看(1) == Some('>') => {
                    self.读错误重定向()?;
                }
                '(' | ')' => return Err(语法错误::意外符号(字符.to_string())),
                _ => {
                    if 字符 == '=' && self.赋值等号位置.is_none() {
                        let 已打字数 = self.当前词.as_ref().map_or(0, |单字| 单字.文本.chars().count());
                        self.赋值等号位置 = Some(已打字数);
                    }
                    self.取当前词().文本.push(字符);
                    self.当前位置 += 1;
                }
            }
        }
        self.结束作业()
    }

    /// 解析 `2>` `2>>` `2>&1`。
    fn 读错误重定向(&mut self) -> Result<(), 语法错误> {
        if self.预看(2) == Some('&') && self.预看(3) == Some('1') {
            self.当前位置 += 4;
            self.合并错误输出();
            return Ok(());
        }
        let 追加 = self.预看(2) == Some('>');
        self.当前位置 += if 追加 { 3 } else { 2 };
        let 目标 = self.读重定向目标()?;
        self.当前作业.错误输出 = Some((目标, 追加));
        Ok(())
    }

    /// 处理 `|`、`|&`。`|` 左侧的 `2>&1` 已经由 `>` 分支记在「作业级」上。
    fn 读管道(&mut self) -> Result<(), 语法错误> {
        self.收集词();
        if self.当前命令.是否为空() && self.当前管道.命令.is_empty() {
            return Err(语法错误::缺少命令);
        }
        self.当前管道.命令.push(std::mem::take(&mut self.当前命令));

        self.当前位置 += 1;
        let 连接 = if self.预看(0) == Some('&') {
            self.当前位置 += 1;
            连接方式::管道含错误
        } else {
            连接方式::管道
        };
        self.当前管道.连接.push(连接);
        Ok(())
    }

    /// `2>&1`：标准错误跟随标准输出。
    fn 合并错误输出(&mut self) {
        self.当前作业.合并错误 = true;
    }

    fn 读引号(&mut self, 引号: char) -> Result<(), 语法错误> {
        self.当前位置 += 1;
        while let Some(字符) = self.预看(0) {
            self.当前位置 += 1;
            if 字符 == 引号 {
                let 当前词 = self.取当前词();
                if 引号 == '\'' {
                    当前词.含单引号 = true;
                } else {
                    当前词.含双引号 = true;
                }
                return Ok(());
            }
            if 引号 == '"' && 字符 == '\\' {
                match self.预看(0) {
                    Some(下一个 @ ('$' | '`' | '"' | '\\')) => {
                        self.当前位置 += 1;
                        self.取当前词().文本.push(下一个);
                    }
                    Some('\n') => self.当前位置 += 1,
                    Some(其它) => self.保留反斜杠(其它),
                    None => return Err(语法错误::反斜杠在行尾),
                }
                continue;
            }
            self.取当前词().文本.push(字符);
        }
        Err(语法错误::引号未闭合(引号))
    }

    /// 双引号内，`\` 后跟非特殊字符时，反斜杠本身要保留。
    fn 保留反斜杠(&mut self, 字符: char) {
        let 当前词 = self.取当前词();
        当前词.文本.push('\\');
        当前词.文本.push(字符);
    }
}

/// 执行前的展开：`~`、`$变量`、`${变量}` 与通配符。
/// 有双引号的词只展开变量、不展开通配与 `~`；有单引号的词原样保留。
/// `查变量` 用于解析变量与 `$HOME`。
pub fn 展开词(单字: &词, 查变量: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let 文本 = 展开变量(单字, 查变量);
    if !单字.含单引号 && !单字.含双引号 {
        if let Some(匹配列表) = 通配展开(&文本) {
            return 匹配列表;
        }
    }
    vec![文本]
}

/// 变量与 `~` 展开（不做通配）。
pub fn 展开变量(单字: &词, 查变量: &dyn Fn(&str) -> Option<String>) -> String {
    if 单字.含单引号 {
        // 引号里的 `$` 不展开，原样交给程序
        return 单字.文本.clone();
    }
    let 字符表: Vec<char> = 单字.文本.chars().collect();
    let mut 结果 = String::new();
    let mut 游标 = 0;
    while 游标 < 字符表.len() {
        if 字符表[游标] != '$' {
            结果.push(字符表[游标]);
            游标 += 1;
            continue;
        }
        match 字符表.get(游标 + 1) {
            // `${名称}`
            Some('{') => {
                let 名称起点 = 游标 + 2;
                match 字符表[名称起点..].iter().position(|&字符| 字符 == '}') {
                    Some(偏移) => {
                        let 名称: String = 字符表[名称起点..名称起点 + 偏移].iter().collect();
                        结果.push_str(&查变量(&名称).unwrap_or_default());
                        游标 = 名称起点 + 偏移 + 1;
                    }
                    None => {
                        结果.push('$');
                        游标 += 1;
                    }
                }
            }
            // `$名称`
            Some(&首字) if 首字.is_alphanumeric() || 首字 == '_' => {
                let 名称起点 = 游标 + 1;
                let mut 名称终点 = 名称起点;
                while 名称终点 < 字符表.len()
                    && (字符表[名称终点].is_alphanumeric() || 字符表[名称终点] == '_')
                {
                    名称终点 += 1;
                }
                let 名称: String = 字符表[名称起点..名称终点].iter().collect();
                结果.push_str(&查变量(&名称).unwrap_or_default());
                游标 = 名称终点;
            }
            _ => {
                结果.push('$');
                游标 += 1;
            }
        }
    }
    // `~` 只在词首、且没有引号时展开
    if !单字.含单引号 && !单字.含双引号 {
        if let Some(家目录) = 查变量("HOME") {
            if 结果 == "~" {
                return 家目录;
            }
            if let Some(余下) = 结果.strip_prefix("~/") {
                return format!("{家目录}/{余下}");
            }
        }
    }
    结果
}

/// 通配符展开；没有匹配项时返回 None（保留原样）。
pub fn 通配展开(模式: &str) -> Option<Vec<String>> {
    if !模式.contains(['*', '?', '[']) {
        return None;
    }
    let (目录部分, 文件名模式) = match 模式.rfind('/') {
        Some(位置) => (&模式[..=位置], &模式[位置 + 1..]),
        None => ("", 模式),
    };
    let 要读的目录 = if 目录部分.is_empty() { "." } else { 目录部分 };
    let mut 匹配列表: Vec<String> = std::fs::read_dir(要读的目录)
        .ok()?
        .flatten()
        .map(|条目| 条目.file_name().to_string_lossy().into_owned())
        .filter(|文件名| !文件名.starts_with('.') || 文件名模式.starts_with('.'))
        .filter(|文件名| 模式匹配(文件名模式, 文件名))
        .map(|文件名| format!("{目录部分}{文件名}"))
        .collect();
    匹配列表.sort();
    if 匹配列表.is_empty() {
        None
    } else {
        Some(匹配列表)
    }
}

/// shell 风格模式匹配：`*` 任意串、`?` 单字符、`[...]` 字符类（支持 `a-z`、`!`/`^` 取反）。
pub fn 模式匹配(模式: &str, 文本: &str) -> bool {
    let 模式字符: Vec<char> = 模式.chars().collect();
    let 文本字符: Vec<char> = 文本.chars().collect();
    let (mut 模式位, mut 文本位) = (0usize, 0usize);
    let mut 最近星号: Option<(usize, usize)> = None;
    '逐个匹配: while 文本位 < 文本字符.len() {
        let 命中 = 模式位 < 模式字符.len()
            && match 模式字符[模式位] {
                '*' => {
                    最近星号 = Some((模式位, 文本位));
                    模式位 += 1;
                    continue '逐个匹配;
                }
                '?' => true,
                '[' => match 字符类匹配(&模式字符[模式位..], 文本字符[文本位]) {
                    Some((是否命中, 类长度)) => {
                        if 是否命中 {
                            模式位 += 类长度;
                            文本位 += 1;
                            continue '逐个匹配;
                        }
                        false
                    }
                    // 不像字符类（没有闭合的 `]`）：把 `[` 当普通字符
                    None => 模式字符[模式位] == 文本字符[文本位],
                },
                固定字符 => 固定字符 == 文本字符[文本位],
            };
        if 命中 {
            模式位 += 1;
            文本位 += 1;
        } else if let Some((星号位, 星号处文本位)) = 最近星号 {
            // 失配：回退到最近的 `*`，让它多吃一个字符
            最近星号 = Some((星号位, 星号处文本位 + 1));
            模式位 = 星号位 + 1;
            文本位 = 星号处文本位 + 1;
        } else {
            return false;
        }
    }
    while 模式位 < 模式字符.len() && 模式字符[模式位] == '*' {
        模式位 += 1;
    }
    模式位 == 模式字符.len()
}

/// 解析 `[...]`；返回（是否命中，字符类总长）。不像字符类则返回 None。
fn 字符类匹配(模式: &[char], 目标字符: char) -> Option<(bool, usize)> {
    let mut 游标 = 1;
    let mut 取反 = false;
    if matches!(模式.get(游标), Some('!') | Some('^')) {
        取反 = true;
        游标 += 1;
    }
    let mut 命中 = false;
    let mut 是首位 = true;
    while 游标 < 模式.len() {
        if 模式[游标] == ']' && !是首位 {
            return Some((命中 != 取反, 游标 + 1));
        }
        是首位 = false;
        if 游标 + 2 < 模式.len() && 模式[游标 + 1] == '-' && 模式[游标 + 2] != ']' {
            if 模式[游标] <= 目标字符 && 目标字符 <= 模式[游标 + 2] {
                命中 = true;
            }
            游标 += 3;
        } else {
            if 模式[游标] == 目标字符 {
                命中 = true;
            }
            游标 += 1;
        }
    }
    None
}

#[cfg(test)]
mod 测试 {
    use super::*;

    fn 测试变量(键: &str) -> Option<String> {
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
            .flat_map(|单字| 展开词(单字, &测试变量))
            .collect()
    }

    #[test]
    fn 基本分词与引号() {
        // 单引号：变量不展开；双引号：变量展开、空白不切分
        let 作业列表 = 解析命令(r#"echo 你好 "a b" 'c $d' "$名字""#).unwrap();
        assert_eq!(作业列表.len(), 1);
        let 展开后 = 展开全部(&作业列表[0].管道.命令[0]);
        assert_eq!(展开后, ["echo", "你好", "a b", "c $d", "世界"]);
    }

    #[test]
    fn 管道与重定向() {
        let 作业列表 = 解析命令("ls -l | grep rust > /tmp/x 2>&1").unwrap();
        let 第一个作业 = &作业列表[0];
        assert_eq!(第一个作业.管道.命令.len(), 2);
        assert_eq!(第一个作业.输出.as_ref().unwrap().0.文本, "/tmp/x");
        assert!(第一个作业.合并错误);
        assert!(第一个作业.错误输出.is_none());
    }

    #[test]
    fn 错误重定向() {
        let 作业列表 = 解析命令("cmd 2>>err.log <in.txt").unwrap();
        let 第一个作业 = &作业列表[0];
        assert_eq!(第一个作业.错误输出.as_ref().unwrap().0.文本, "err.log");
        assert!(第一个作业.错误输出.as_ref().unwrap().1);
        assert_eq!(第一个作业.输入.as_ref().unwrap().文本, "in.txt");

        // `&>` 同时重定向标准输出与标准错误；之后的 `2>err.log` 覆盖标准错误
        let 作业列表 = 解析命令("cmd &>both.log 2>err.log").unwrap();
        let 第一个作业 = &作业列表[0];
        assert_eq!(第一个作业.输出.as_ref().unwrap().0.文本, "both.log");
        assert_eq!(第一个作业.错误输出.as_ref().unwrap().0.文本, "err.log");

        // `2` 只在词首且紧跟 `>` 时才被当成描述符前缀
        let 作业列表 = 解析命令("echo 2-3").unwrap();
        assert_eq!(展开全部(&作业列表[0].管道.命令[0]), ["echo", "2-3"]);
    }

    #[test]
    fn 后台与分号() {
        let 作业列表 = 解析命令("sleep 1 & echo hi; pwd").unwrap();
        assert_eq!(作业列表.len(), 3);
        assert!(作业列表[0].后台);
        assert!(!作业列表[1].后台);
    }

    #[test]
    fn 变量与波浪号() {
        let 作业列表 = 解析命令("echo $名字 ~/x ${HOME} $未知").unwrap();
        let 展开后 = 展开全部(&作业列表[0].管道.命令[0]);
        assert_eq!(展开后, ["echo", "世界", "/home/u/x", "/home/u", ""]);
    }

    #[test]
    fn 前置赋值() {
        let 作业列表 = 解析命令("A=1 B=\"x y\" echo hi").unwrap();
        let 第一条命令 = &作业列表[0].管道.命令[0];
        assert_eq!(第一条命令.赋值.len(), 2);
        assert_eq!(第一条命令.赋值[1].1.文本, "x y");
        assert_eq!(展开全部(第一条命令), ["echo", "hi"]);
    }

    #[test]
    fn 引号未闭合() {
        assert_eq!(解析命令("echo 'abc").unwrap_err(), 语法错误::引号未闭合('\''));
        assert_eq!(解析命令("echo \"abc").unwrap_err(), 语法错误::引号未闭合('"'));
    }

    #[test]
    fn 空管道报错() {
        assert_eq!(解析命令("echo a |").unwrap_err(), 语法错误::缺少命令);
        assert_eq!(解析命令("| echo").unwrap_err(), 语法错误::缺少命令);
    }

    #[test]
    fn 通配匹配() {
        assert!(模式匹配("*.rs", "main.rs"));
        assert!(!模式匹配("*.rs", "main.py"));
        assert!(模式匹配("a?c", "abc"));
        assert!(模式匹配("[a-c]*", "banana"));
        assert!(模式匹配("[!a-c]*", "dog"));
        assert!(!模式匹配("[!a-c]*", "cat"));
        assert!(模式匹配("中*", "中文"));
        assert!(模式匹配("*", ""));
    }

    #[test]
    fn 注释与续行() {
        let 作业列表 = 解析命令("echo hi # 注释\npwd").unwrap();
        assert_eq!(作业列表.len(), 2);
        let 作业列表 = 解析命令("echo \\\nhi").unwrap();
        assert_eq!(作业列表.len(), 1);
    }

    #[test]
    fn 管道与后台不冲突() {
        let 作业列表 = 解析命令("sleep 1 & echo |& cat").unwrap();
        assert_eq!(作业列表.len(), 2);
        assert!(作业列表[0].后台);
        assert_eq!(作业列表[1].管道.连接, [连接方式::管道含错误]);

        let 作业列表 = 解析命令("echo a &> 全部.txt").unwrap();
        assert!(!作业列表[0].后台);
        assert_eq!(作业列表[0].输出.as_ref().unwrap().0.文本, "全部.txt");
        assert_eq!(作业列表[0].错误输出.as_ref().unwrap().0.文本, "全部.txt");
    }

    #[test]
    fn 管道与错误() {
        let 作业列表 = 解析命令("make |& tee log").unwrap();
        assert_eq!(作业列表[0].管道.连接, [连接方式::管道含错误]);
        // `2>&1 |` 让左段的标准错误也进管道
        let 作业列表 = 解析命令("make 2>&1 | tee log").unwrap();
        assert!(作业列表[0].合并错误);
        // 注意：`2>&1 |` 中的 `2>&1` 属于「作业级」合并，`|` 本身仍是普通管道
        assert_eq!(作业列表[0].管道.连接, [连接方式::管道]);
        let 作业列表 = 解析命令("make 2>&1").unwrap();
        assert!(作业列表[0].合并错误);
        assert!(作业列表[0].管道.连接.is_empty());
    }
}
