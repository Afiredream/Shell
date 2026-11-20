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
///
/// `字面量区` 与 `文本` 按字符一一对应：为 `true` 的字符来自单引号内部，
/// 展开时原样保留（不认 `$`、不认通配、不认首个 `~`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct 词 {
    pub 文本: String,
    字面量区: Vec<bool>,
    /// 含有双引号内容：`$` 展开，但不做通配 / `~` 展开
    pub 含双引号: bool,
}

impl 词 {
    /// 在词尾追加字符，`是否字面量` 表示它是否来自单引号内部。
    fn 追加(&mut self, 字符: char, 是否字面量: bool) {
        self.文本.push(字符);
        self.字面量区.push(是否字面量);
    }

    /// 去掉前 `个数` 个字符（用于把 `名称=值` 拆成赋值）。
    fn 去掉前若干字符(&mut self, 个数: usize) {
        self.文本 = self.文本.chars().skip(个数).collect();
        self.字面量区.drain(..个数.min(self.字面量区.len()));
    }
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
    字符 == ' ' || 字符 == '\t' || 字符 == '\r' || 字符 == '\n'
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
        let Some(mut 正在拼的词) = self.当前词.take() else {
            return;
        };
        if let Some(位) = 等号位置.filter(|_| self.当前命令.词列表.is_empty()) {
            // `位` 是「已打字数」（字符数），不是字节偏移；中文变量名必须按字符切分
            let 名称字符: Vec<char> = 正在拼的词.文本.chars().take(位).collect();
            let 名称合法 = 名称字符.first().is_some_and(|首字| 首字.is_alphabetic() || *首字 == '_')
                && 名称字符
                    .iter()
                    .all(|字符| 字符.is_alphanumeric() || *字符 == '_');
            if 名称合法 {
                // 去掉「名称 + 等号」那一段（按字符计，`位 + 1` 含 `=`）
                正在拼的词.去掉前若干字符(位 + 1);
                let 名称: String = 名称字符.into_iter().collect();
                self.当前命令.赋值.push((名称, 正在拼的词));
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
        let mut 目标 = 词::default();
        while let Some(字符) = self.预看(0) {
            match 字符 {
                '\\' => {
                    self.当前位置 += 1;
                    match self.预看(0) {
                        Some(下一个) => {
                            目标.追加(下一个, true);
                            self.当前位置 += 1;
                        }
                        None => return Err(语法错误::反斜杠在行尾),
                    }
                }
                '\'' | '"' => {
                    self.读引号进目标(&mut 目标, 字符)?;
                }
                _ if 是空白(字符) || 是操作符(字符) => break,
                _ => {
                    目标.追加(字符, false);
                    self.当前位置 += 1;
                }
            }
        }
        if 目标.文本.is_empty() {
            return Err(语法错误::重定向缺少目标);
        }
        Ok(目标)
    }

    /// 从当前引号开始，读到配对引号为止，内容写进 `目标`。
    fn 读引号进目标(&mut self, 目标: &mut 词, 引号: char) -> Result<(), 语法错误> {
        let 来自单引号 = 引号 == '\'';
        if !来自单引号 {
            目标.含双引号 = true;
        }
        self.当前位置 += 1;
        while let Some(内部字符) = self.预看(0) {
            if 内部字符 == 引号 {
                self.当前位置 += 1;
                return Ok(());
            }
            if 内部字符 == '\\' {
                self.当前位置 += 1;
                match self.预看(0) {
                    Some(下一个) => {
                        目标.追加(下一个, 来自单引号);
                        self.当前位置 += 1;
                    }
                    None => return Err(语法错误::反斜杠在行尾),
                }
                continue;
            }
            目标.追加(内部字符, 来自单引号);
            self.当前位置 += 1;
        }
        Err(语法错误::引号未闭合(引号))
    }

    fn 运行(&mut self) -> Result<(), 语法错误> {
        while let Some(字符) = self.预看(0) {
            match 字符 {
                // 反斜杠转义：`\` + 换行 是续行，其余取下一个字符的字面值
                '\\' => {
                    self.当前位置 += 1;
                    match self.预看(0) {
                        None => return Err(语法错误::反斜杠在行尾),
                        Some('\n') => self.当前位置 += 1,
                        Some(下一个) => {
                            self.取当前词().追加(下一个, false);
                            self.当前位置 += 1;
                        }
                    }
                }
                '\'' | '"' => self.读引号(字符)?,
                '\n' => {
                    self.当前位置 += 1;
                    self.结束作业()?;
                }
                c if 是空白(c) => {
                    self.收集词();
                    self.当前位置 += 1;
                }
                // `#` 在词的开头位置才视为注释
                '#' if self.当前词.is_none() => {
                    while self.预看(0).is_some_and(|下一个| 下一个 != '\n') {
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
        let 来自单引号 = 引号 == '\'';
        while let Some(当前字符) = self.预看(0) {
            self.当前位置 += 1;
            if 当前字符 == 引号 {
                if 引号 == '"' {
                    self.取当前词().含双引号 = true;
                }
                return Ok(());
            }
            // 单引号内只认 `\'`（与 bash 一致）；双引号内只认 `$` ` \` 四个字符
            if 当前字符 == '\\' {
                match self.预看(0) {
                    Some(下一个) if 来自单引号 && 下一个 == '\'' => {
                        self.当前位置 += 1;
                        self.取当前词().追加('\'', true);
                        continue;
                    }
                    Some(下一个 @ ('$' | '`' | '"' | '\\')) if !来自单引号 => {
                        self.当前位置 += 1;
                        self.取当前词().追加(下一个, false);
                        continue;
                    }
                    Some('\n') => {
                        self.当前位置 += 1;
                        continue;
                    }
                    Some(其它) if !来自单引号 => {
                        let 当前词 = self.取当前词();
                        当前词.追加('\\', false);
                        当前词.追加(其它, false);
                        self.当前位置 += 1;
                        continue;
                    }
                    Some(其它) => {
                        // 单引号内 `\` 后跟非 `'`：反斜杠是字面量
                        self.取当前词().追加('\\', true);
                        let _ = 其它;
                        continue;
                    }
                    None => return Err(语法错误::反斜杠在行尾),
                }
            }
            self.取当前词().追加(当前字符, 来自单引号);
        }
        Err(语法错误::引号未闭合(引号))
    }

}

/// 执行前的展开：`~`、`$变量`、`${变量}` 与通配符。
///
/// - 单引号内的字符是字面量：不展开 `$`，也不通配；
/// - 含有双引号的词只展开变量，不做通配与 `~`；
/// - `查变量` 用于解析变量与 `$HOME`。
pub fn 展开词(单字: &词, 查变量: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let 文本 = 展开变量(单字, 查变量);
    if let Some(匹配列表) = 通配展开(单字, &文本) {
        return 匹配列表;
    }
    vec![文本]
}

/// 变量与 `~` 展开（不做通配）。
pub fn 展开变量(单字: &词, 查变量: &dyn Fn(&str) -> Option<String>) -> String {
    let 字符表: Vec<char> = 单字.文本.chars().collect();
    let mut 结果 = String::new();
    let mut 游标 = 0;
    while 游标 < 字符表.len() {
        let 是字面量 = 单字.字面量区.get(游标).copied().unwrap_or(false);
        if 字符表[游标] != '$' || 是字面量 {
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
                    // 没有闭合的 `}`：`$` 当字面量
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

    // `~` 只在词首、位于单引号/转义之外、且词里没有双引号时展开
    let 首字是字面量 = 单字.字面量区.first().copied().unwrap_or(false);
    if !首字是字面量 && !单字.含双引号 {
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
///
/// 只有词里**原样写出**的 `*`、`?`、`[` 才触发通配：单引号/转义得到的、
/// 以及变量展开出来的内容都原样交给程序。
pub fn 通配展开(单字: &词, 模式: &str) -> Option<Vec<String>> {
    if 单字.含双引号 {
        return None;
    }
    let 展开后字符: Vec<char> = 模式.chars().collect();
    let 有原样通配符 = |序号: usize| {
        let 是字面量 = 单字.字面量区.get(序号).copied().unwrap_or(false);
        !是字面量 && matches!(展开后字符[序号], '*' | '?' | '[')
    };
    let 有原样通配 = 单字.文本.chars().count() == 展开后字符.len()
        && (0..展开后字符.len()).any(有原样通配符);
    if !有原样通配 {
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
    fn 单引号段按字面量处理() {
        // 只有单引号段受限，后面的裸 `$` 照常展开（与 bash 一致）
        let 作业列表 = 解析命令(r#"echo 'A'"$名字""#).unwrap();
        assert_eq!(展开全部(&作业列表[0].管道.命令[0]), ["echo", "A世界"]);

        let 作业列表 = 解析命令(r"echo 'a\'b'").unwrap();
        assert_eq!(展开全部(&作业列表[0].管道.命令[0]), ["echo", "a'b"]);

        // 双引号里 `\` 可转义 `"`，但 `\t` 保持字面
        let 作业列表 = 解析命令(r#"echo "x\"y" "a\tb""#).unwrap();
        assert_eq!(展开全部(&作业列表[0].管道.命令[0]), ["echo", "x\"y", r"a\tb"]);

        // 单个 `~` 只有不被引号包住才展开
        let 作业列表 = 解析命令(r#"echo ~ '~'"#).unwrap();
        assert_eq!(展开全部(&作业列表[0].管道.命令[0]), ["echo", "/home/u", "~"]);
    }

    #[test]
    fn 井号紧跟引号内容() {
        // 引号里的 `#` 属于词本身；引号后面紧跟着的 `#` 也是同一个词的一部分
        let 作业列表 = 解析命令(r"echo '#'").unwrap();
        assert_eq!(展开全部(&作业列表[0].管道.命令[0]), ["echo", "#"]);

        let 作业列表 = 解析命令(r"echo prev'#'后").unwrap();
        assert_eq!(展开全部(&作业列表[0].管道.命令[0]), ["echo", "prev#后"]);
    }

    #[test]
    fn 中文变量赋值() {
        // 等号位置按字符计，中文变量名过去会按字节切片而崩溃
        let 作业列表 = 解析命令("名字=壳语言 echo $名字").unwrap();
        let 第一条命令 = &作业列表[0].管道.命令[0];
        assert_eq!(第一条命令.赋值.len(), 1);
        assert_eq!(第一条命令.赋值[0].0, "名字");
        assert_eq!(第一条命令.赋值[0].1.文本, "壳语言");
        // 前置赋值只影响子进程：命令行里的 `$名字` 仍按展开时的环境解析
        assert_eq!(展开全部(第一条命令), ["echo", "世界"]);

        let 作业列表 = 解析命令("名=值").unwrap();
        assert_eq!(作业列表[0].管道.命令[0].赋值[0].0, "名");
        assert_eq!(作业列表[0].管道.命令[0].赋值[0].1.文本, "值");
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
    fn 重定向目标不吃换行() {
        // 曾经：重定向目标把行尾换行也吞进文件名（`a.txt\n`），并把后面的行并进目标
        let 作业列表 = 解析命令("echo hi > 输出.txt\npwd").unwrap();
        assert_eq!(作业列表.len(), 2);
        assert_eq!(作业列表[0].输出.as_ref().unwrap().0.文本, "输出.txt");
        assert_eq!(展开全部(&作业列表[1].管道.命令[0]), ["pwd"]);

        // 带引号、带空格、带空白的各种写法
        for (输入, 期望) in [
            ("echo x > \"带 空格.txt\"", "带 空格.txt"),
            ("echo x > '单 引号.txt'", "单 引号.txt"),
            ("echo x > 反\\ 斜杠.txt", "反 斜杠.txt"),
            ("echo x > a.txt\r\nls", "a.txt"),
        ] {
            let 作业列表 = 解析命令(输入).unwrap();
            assert_eq!(作业列表[0].输出.as_ref().unwrap().0.文本, 期望, "输入：{输入}");
        }
        assert_eq!(解析命令("echo x > a.txt\r\nls").unwrap().len(), 2);
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
