//! 运行时值与表达式：整数、文本、数组、对象。
//!
//! 对象只是**有序键值对数据**，没有方法、没有类、没有继承——不是 OOP。
//! 数组与对象可以互相嵌套，也可以用 `$变量` 把 shell 变量的值取进来。

// 与既有命名风格一致：JSON 作为名字的一部分保留大写
#![allow(non_snake_case)]

use std::fmt;

/// 运行时值。
#[derive(Debug, Clone, PartialEq)]
pub enum 值 {
    整数(i64),
    文本(String),
    数组(Vec<值>),
    对象(Vec<(String, 值)>),
}

impl 值 {
    /// 转成可在命令行里直接使用的文本：数组是 `(a b c)`，对象是 `{键=值 ...}`。
    pub fn 转文本(&self) -> String {
        match self {
            值::整数(数) => 数.to_string(),
            值::文本(文本) => 文本.clone(),
            值::数组(项们) => {
                let 内容: Vec<String> = 项们.iter().map(|项| 项.转文本()).collect();
                format!("({})", 内容.join(" "))
            }
            值::对象(项们) => {
                let 内容: Vec<String> = 项们
                    .iter()
                    .map(|(键, 项)| format!("{键}={}", 项.转文本()))
                    .collect();
                format!("{{{}}}", 内容.join(" "))
            }
        }
    }

    /// 序列化为 JSON（与 `写JSON` 相同，保留旧名字）。
    #[allow(non_snake_case)]
    pub fn 转JSON(&self) -> String {
        self.写JSON()
    }

    /// 转成退出码：整数取自身；文本若是数字也按数字取，其余视为成功（0）。
    pub fn 转退出码(&self) -> i64 {
        match self {
            值::整数(数) => *数,
            值::文本(文本) => 文本.parse().unwrap_or(0),
            _ => 0,
        }
    }
}

impl 值 {
    /// 读取 JSON 文本：对象是 `值::对象`（保持键序），数组是 `值::数组`，
    /// 整数在 i64 范围内取整数，其它数字/布尔/空值按文本。不是合法 JSON 时返回 None。
    pub fn 读JSON(文本: &str) -> Option<值> {
        let 字节 = 文本.trim().as_bytes();
        let mut 读位 = 0;
        let 结果 = 读JSON值(字节, &mut 读位)?;
        跳过空白(字节, &mut 读位);
        if 读位 == 字节.len() {
            Some(结果)
        } else {
            None
        }
    }

    /// 写成 JSON 文本（键序保持不变）。
    pub fn 写JSON(&self) -> String {
        let mut 输出 = String::new();
        写JSON值(self, &mut 输出);
        输出
    }
}

fn 跳过空白(字节: &[u8], 读位: &mut usize) {
    while *读位 < 字节.len() && matches!(字节[*读位], b' ' | b'\t' | b'\r' | b'\n') {
        *读位 += 1;
    }
}

fn 读JSON值(字节: &[u8], 读位: &mut usize) -> Option<值> {
    跳过空白(字节, 读位);
    match *字节.get(*读位)? {
        b'{' => {
            *读位 += 1;
            let mut 项们 = Vec::new();
            跳过空白(字节, 读位);
            if 字节.get(*读位) == Some(&b'}') {
                *读位 += 1;
                return Some(值::对象(项们));
            }
            loop {
                跳过空白(字节, 读位);
                let 键 = 读JSON文本(字节, 读位)?;
                跳过空白(字节, 读位);
                if 字节.get(*读位) != Some(&b':') {
                    return None;
                }
                *读位 += 1;
                项们.push((键, 读JSON值(字节, 读位)?));
                跳过空白(字节, 读位);
                match 字节.get(*读位)? {
                    b',' => *读位 += 1,
                    b'}' => {
                        *读位 += 1;
                        return Some(值::对象(项们));
                    }
                    _ => return None,
                }
            }
        }
        b'[' => {
            *读位 += 1;
            let mut 项们 = Vec::new();
            跳过空白(字节, 读位);
            if 字节.get(*读位) == Some(&b']') {
                *读位 += 1;
                return Some(值::数组(项们));
            }
            loop {
                项们.push(读JSON值(字节, 读位)?);
                跳过空白(字节, 读位);
                match 字节.get(*读位)? {
                    b',' => *读位 += 1,
                    b']' => {
                        *读位 += 1;
                        return Some(值::数组(项们));
                    }
                    _ => return None,
                }
            }
        }
        b'"' => Some(值::文本(读JSON文本(字节, 读位)?)),
        b't' if 字节[*读位..].starts_with(b"true") => {
            *读位 += 4;
            Some(值::文本("true".into()))
        }
        b'f' if 字节[*读位..].starts_with(b"false") => {
            *读位 += 5;
            Some(值::文本("false".into()))
        }
        b'n' if 字节[*读位..].starts_with(b"null") => {
            *读位 += 4;
            Some(值::文本(String::new()))
        }
        _ => {
            let 起点 = *读位;
            while *读位 < 字节.len() && matches!(字节[*读位], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                *读位 += 1;
            }
            if 起点 == *读位 {
                return None;
            }
            let 数字 = std::str::from_utf8(&字节[起点..*读位]).ok()?;
            match 数字.parse::<i64>() {
                Ok(整数) => Some(值::整数(整数)),
                Err(_) => Some(值::文本(数字.to_string())),
            }
        }
    }
}

fn 读JSON文本(字节: &[u8], 读位: &mut usize) -> Option<String> {
    if 字节.get(*读位) != Some(&b'"') {
        return None;
    }
    *读位 += 1;
    let mut 输出 = String::new();
    loop {
        let 字节值 = *字节.get(*读位)?;
        *读位 += 1;
        match 字节值 {
            b'"' => return Some(输出),
            b'\\' => {
                let 转义 = *字节.get(*读位)?;
                *读位 += 1;
                match 转义 {
                    b'"' => 输出.push('"'),
                    b'\\' => 输出.push('\\'),
                    b'/' => 输出.push('/'),
                    b'b' => 输出.push('\u{8}'),
                    b'f' => 输出.push('\u{c}'),
                    b'n' => 输出.push('\n'),
                    b'r' => 输出.push('\r'),
                    b't' => 输出.push('\t'),
                    b'u' => {
                        let 十六 = std::str::from_utf8(字节.get(*读位..*读位 + 4)?).ok()?;
                        *读位 += 4;
                        let 码 = u32::from_str_radix(十六, 16).ok()?;
                        输出.push(char::from_u32(码).unwrap_or('\u{fffd}'));
                    }
                    _ => return None,
                }
            }
            控制 if 控制 < b' ' => return None,
            _ => {
                // 非转义字节：按 UTF-8 解码一个字符（可能多字节）
                let 起点 = *读位 - 1;
                let 长度 = match 字节值 {
                    0x00..=0x7f => 1,
                    0xc0..=0xdf => 2,
                    0xe0..=0xef => 3,
                    _ => 4,
                };
                let 段 = 字节.get(起点..起点 + 长度)?;
                *读位 = 起点 + 长度;
                输出.push_str(std::str::from_utf8(段).ok()?);
            }
        }
    }
}

impl 值 {
    /// 按文件类型解析文本：`JSON` / `INI` / `文本`（其它类型名返回错误）。
    pub fn 解析文本(文本: &str, 类型: &str) -> Result<值, String> {
        match 类型.to_ascii_uppercase().as_str() {
            "JSON" => 值::读JSON(文本).ok_or_else(|| "不是合法的 JSON".to_string()),
            "INI" => Ok(读INI(文本)),
            "文本" | "TEXT" => Ok(值::文本(文本.to_string())),
            其它 => Err(format!("不支持的类型 `{其它}`（可用 JSON、INI、文本）")),
        }
    }
}

impl 值 {
    /// 按文件类型写出：`JSON` 输出 JSON（带缩进），`INI` 输出段与 `键=值`，
    /// `文本` 原样输出。内容不是能解析的表达式时按纯文本处理。
    pub fn 按类型写出(文本: &str, 类型: &str) -> Result<String, String> {
        let 类型名 = 类型.to_ascii_uppercase();
        if 类型名 == "文本" || 类型名 == "TEXT" {
            return Ok(文本.to_string());
        }
        // 内容可以是表达式文本（例如 `$返回值对象` 展开后的 `{键=值}`），也可以直接是 JSON
        let 内容 = match 解析表达式(文本) {
            // 单个裸词解析出来是文本，先按对应格式再读一次（`名称=壳` 之类的键值写法）
            Ok(表达式::值(值::文本(原样))) => match 值::解析文本(&原样, &类型名) {
                Ok(解析结果) => 解析结果,
                Err(_) => 值::文本(原样),
            },
            Ok(表达式) => 求值(&表达式, &|_| None),
            Err(_) => 值::解析文本(文本, &类型名)?,
        };
        match 类型名.as_str() {
            "JSON" => Ok(缩进JSON(&内容, 0)),
            "INI" => Ok(写INI(&内容)),
            其它 => Err(format!("不支持的类型 `{其它}`（可用 JSON、INI、文本）")),
        }
    }
}

/// JSON 美化：两空格缩进，数组单行时保持单行更省地方。
fn 缩进JSON(项: &值, 缩进: usize) -> String {
    let 前缀 = "  ".repeat(缩进);
    match 项 {
        值::对象(项们) if !项们.is_empty() => {
            let 子件: Vec<String> = 项们
                .iter()
                .map(|(键, 子项)| {
                    let mut 键文本 = String::new();
                    写JSON文本(键, &mut 键文本);
                    format!("{}  {}: {}", 前缀, 键文本, 缩进JSON(子项, 缩进 + 1))
                })
                .collect();
            format!("{{\n{}\n{}}}", 子件.join(",\n"), 前缀)
        }
        值::数组(项们) if !项们.is_empty() => {
            let 全部简单 = 项们.iter().all(|子项| !matches!(子项, 值::对象(_) | 值::数组(_)));
            if 全部简单 {
                项.写JSON()
            } else {
                let 子件: Vec<String> =
                    项们.iter().map(|子项| format!("{}  {}", 前缀, 缩进JSON(子项, 缩进 + 1))).collect();
                format!("[\n{}\n{}]", 子件.join(",\n"), 前缀)
            }
        }
        _ => 项.写JSON(),
    }
}

/// INI 写出：顶层对象视为若干段（值为对象的项），另有非对象的键时放在 `[全局]` 下；
/// 数组写成逗号分隔的值，含空格或逗号的值加双引号。
fn 写INI(项: &值) -> String {
    fn 值文本(子项: &值) -> String {
        match 子项 {
            值::数组(项们) => 项们.iter().map(值文本).collect::<Vec<_>>().join(","),
            值::对象(_) => 子项.写JSON(),
            _ => {
                let 文本 = 子项.转文本();
                if 文本.contains([' ', ',', '\'', '=', '"']) || 文本.is_empty() {
                    format!("\"{文本}\"")
                } else {
                    文本
                }
            }
        }
    }
    let mut 输出 = String::new();
    let mut 全局行 = Vec::new();
    let 值::对象(项们) = 项 else {
        return 值文本(项);
    };
    for (键, 子项) in 项们 {
        match 子项 {
            值::对象(段内) => {
                if !输出.is_empty() {
                    输出.push('\n');
                }
                输出.push_str(&format!("[{键}]\n"));
                for (段键, 段值) in 段内 {
                    输出.push_str(&format!("{段键}={}\n", 值文本(段值)));
                }
            }
            _ => 全局行.push(format!("{键}={}", 值文本(子项))),
        }
    }
    if !全局行.is_empty() {
        let 全局段 = format!("[全局]\n{}\n", 全局行.join("\n"));
        输出 = 全局段 + &输出;
    }
    输出
}

/// 极简 INI：`[段]` 开启一段，`键=值` 归属最近一段（段之前的键归入 `全局`），
/// 值含 `,` 时拆成数组，`#`/`;` 整行注释，行尾注释只在 `#` 前有空白时才算。
/// 段与键按出现顺序保存；同名键后者覆盖前者。
fn 读INI(文本: &str) -> 值 {
    // 当前段的名字与该段的键值表
    let mut 段名 = String::from("全局");
    let mut 段表: Vec<(String, Vec<(String, 值)>)> = vec![(段名.clone(), Vec::new())];
    for 原行 in 文本.lines() {
        let 行 = 原行.trim();
        if 行.is_empty() || 行.starts_with('#') || 行.starts_with(';') {
            continue;
        }
        if let Some(段) = 行.strip_prefix('[').and_then(|余| 余.strip_suffix(']')) {
            段名 = 段.trim().to_string();
            if !段表.iter().any(|(名字, _)| *名字 == 段名) {
                段表.push((段名.clone(), Vec::new()));
            }
            continue;
        }
        let Some((键, 原值)) = 行.split_once('=') else {
            continue;
        };
        let 键 = 键.trim().to_string();
        let 值 = ini键值(去掉行尾注释(原值.trim()));
        let 段 = 段表
            .iter_mut()
            .find(|(名字, _)| *名字 == 段名)
            .expect("段表中必有当前段");
        match 段.1.iter_mut().find(|(已有键, _)| *已有键 == 键) {
            Some((_, 旧值)) => *旧值 = 值,
            None => 段.1.push((键, 值)),
        }
    }
    // 没有键的段不入结果（`[段]` 后直接结束的段仍保留空对象，方便判断存在性）
    值::对象(
        段表
            .into_iter()
            .filter(|(_, 键值)| !键值.is_empty())
            .map(|(名字, 键值)| (名字, 值::对象(键值)))
            .collect(),
    )
}

/// 行尾注释：`#` 前是空白、且不在引号里，视为注释开始。
fn 去掉行尾注释(原值: &str) -> &str {
    let 字节 = 原值.as_bytes();
    let mut 引导号: Option<u8> = None;
    for (序, &字) in 字节.iter().enumerate() {
        match 字 {
            b'"' | b'\'' => match 引导号 {
                None => 引导号 = Some(字),
                Some(引导) if 引导 == 字 => 引导号 = None,
                _ => {}
            },
            b'#' | b';' if 引导号.is_none() && 序 > 0 && 字节[序 - 1].is_ascii_whitespace() => {
                return 原值[..序].trim_end()
            }
            _ => {}
        }
    }
    原值
}

fn ini键值(原值: &str) -> 值 {
    // 引号包起来的是纯文本
    if 原值.len() >= 2
        && ((原值.starts_with('"') && 原值.ends_with('"'))
            || (原值.starts_with('\'') && 原值.ends_with('\'')))
    {
        return 值::文本(原值[1..原值.len() - 1].to_string());
    }
    let 项们: Vec<&str> = 原值.split(',').map(str::trim).filter(|项| !项.is_empty()).collect();
    let 单值 = |文本: &str| match 文本.parse::<i64>() {
        Ok(整数) => 值::整数(整数),
        Err(_) => match 文本.to_ascii_lowercase().as_str() {
            "true" => 值::文本("true".into()),
            "false" => 值::文本("false".into()),
            _ => 值::文本(文本.to_string()),
        },
    };
    if 项们.len() > 1 {
        值::数组(项们.into_iter().map(单值).collect())
    } else {
        单值(原值)
    }
}

fn 写JSON值(项: &值, 输出: &mut String) {
    match 项 {
        值::整数(数) => 输出.push_str(&数.to_string()),
        值::文本(文本) => 写JSON文本(文本, 输出),
        值::数组(项们) => {
            输出.push('[');
            for (序, 子项) in 项们.iter().enumerate() {
                if 序 > 0 {
                    输出.push(',');
                }
                写JSON值(子项, 输出);
            }
            输出.push(']');
        }
        值::对象(项们) => {
            输出.push('{');
            for (序, (键, 子项)) in 项们.iter().enumerate() {
                if 序 > 0 {
                    输出.push(',');
                }
                写JSON文本(键, 输出);
                输出.push(':');
                写JSON值(子项, 输出);
            }
            输出.push('}');
        }
    }
}

fn 写JSON文本(文本: &str, 输出: &mut String) {
    输出.push('"');
    for 字符 in 文本.chars() {
        match 字符 {
            '"' => 输出.push_str("\\\""),
            '\\' => 输出.push_str("\\\\"),
            '\n' => 输出.push_str("\\n"),
            '\r' => 输出.push_str("\\r"),
            '\t' => 输出.push_str("\\t"),
            控制 if (控制 as u32) < 0x20 => 输出.push_str(&format!("\\u{:04x}", 控制 as u32)),
            其它 => 输出.push(其它),
        }
    }
    输出.push('"');
}

impl fmt::Display for 值 {
    fn fmt(&self, 格式: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(格式, "{}", self.转文本())
    }
}

/// 表达式。求值时变量由调用方提供。
#[derive(Debug, Clone, PartialEq)]
pub enum 表达式 {
    值(值),
    变量(String),
    数组(Vec<表达式>),
    对象(Vec<(String, 表达式)>),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum 表达式错误 {
    #[error("{0} 没有闭合")]
    括号未闭合(char),
    #[error("多余的符号 `{0}`")]
    多余符号(char),
    #[error("对象项必须写成 `键=值`")]
    对象项格式错误,
    #[error("表达式为空")]
    空表达式,
}

/// 求值：把表达式变成运行时值。
pub fn 求值(表达式: &表达式, 查变量: &dyn Fn(&str) -> Option<String>) -> 值 {
    match 表达式 {
        表达式::值(值) => 值.clone(),
        表达式::变量(名) => 值::文本(查变量(名).unwrap_or_default()),
        表达式::数组(项们) => 值::数组(项们.iter().map(|项| 求值(项, 查变量)).collect()),
        表达式::对象(项们) => 值::对象(
            项们
                .iter()
                .map(|(键, 项)| (键.clone(), 求值(项, 查变量)))
                .collect(),
        ),
    }
}

/// 解析一段文本为表达式。
pub fn 解析表达式(文本: &str) -> Result<表达式, 表达式错误> {
    let 字符表: Vec<char> = 文本.chars().collect();
    let mut 解析器 = 解析器 {
        字符表: &字符表,
        位置: 0,
    };
    解析器.跳过空白();
    let 结果 = 解析器.解析值()?;
    解析器.跳过空白();
    if let Some(符号) = 解析器.当前() {
        return Err(表达式错误::多余符号(符号));
    }
    Ok(结果)
}

struct 解析器<'a> {
    字符表: &'a [char],
    位置: usize,
}

impl 解析器<'_> {
    fn 当前(&self) -> Option<char> {
        self.字符表.get(self.位置).copied()
    }

    fn 跳过空白(&mut self) {
        while self.当前().is_some_and(char::is_whitespace) {
            self.位置 += 1;
        }
    }

    fn 解析值(&mut self) -> Result<表达式, 表达式错误> {
        self.跳过空白();
        match self.当前() {
            None => Err(表达式错误::空表达式),
            Some('(') => self.解析数组(),
            Some('{') => self.解析对象(),
            Some('$') => self.解析变量(),
            Some('\'') | Some('"') => self.解析引号文本(),
            _ => self.解析词(),
        }
    }

    fn 解析数组(&mut self) -> Result<表达式, 表达式错误> {
        self.位置 += 1;
        let mut 项们 = Vec::new();
        loop {
            self.跳过空白();
            match self.当前() {
                None => return Err(表达式错误::括号未闭合('(')),
                Some(')') => {
                    self.位置 += 1;
                    break;
                }
                _ => 项们.push(self.解析值()?),
            }
        }
        Ok(表达式::数组(项们))
    }

    fn 解析对象(&mut self) -> Result<表达式, 表达式错误> {
        self.位置 += 1;
        let mut 项们 = Vec::new();
        loop {
            self.跳过空白();
            match self.当前() {
                None => return Err(表达式错误::括号未闭合('{')),
                Some('}') => {
                    self.位置 += 1;
                    break;
                }
                _ => {
                    let 键 = self.读词()?;
                    self.跳过空白();
                    if self.当前() != Some('=') {
                        return Err(表达式错误::对象项格式错误);
                    }
                    self.位置 += 1;
                    let 项 = self.解析值()?;
                    项们.push((键, 项));
                }
            }
        }
        Ok(表达式::对象(项们))
    }

    fn 解析变量(&mut self) -> Result<表达式, 表达式错误> {
        self.位置 += 1;
        if self.当前() == Some('{') {
            self.位置 += 1;
            let 起点 = self.位置;
            while self.当前().is_some_and(|字符| 字符 != '}') {
                self.位置 += 1;
            }
            if self.当前().is_none() {
                return Err(表达式错误::括号未闭合('{'));
            }
            let 名: String = self.字符表[起点..self.位置].iter().collect();
            self.位置 += 1;
            return Ok(表达式::变量(名));
        }
        let 起点 = self.位置;
        while self
             .当前()
             .is_some_and(|字符| 字符.is_alphanumeric() || 字符 == '_')
        {
            self.位置 += 1;
        }
        let 名: String = self.字符表[起点..self.位置].iter().collect();
        Ok(表达式::变量(名))
    }

    fn 解析引号文本(&mut self) -> Result<表达式, 表达式错误> {
        let 引号 = self.当前().unwrap();
        self.位置 += 1;
        let mut 文本 = String::new();
        while let Some(字符) = self.当前() {
            if 字符 == 引号 {
                self.位置 += 1;
                return Ok(表达式::值(值::文本(文本)));
            }
            文本.push(字符);
            self.位置 += 1;
        }
        Err(表达式错误::括号未闭合(引号))
    }

    fn 解析词(&mut self) -> Result<表达式, 表达式错误> {
        let 词 = self.读词()?;
        if let Ok(数) = 词.parse::<i64>() {
            Ok(表达式::值(值::整数(数)))
        } else {
            Ok(表达式::值(值::文本(词)))
        }
    }

    fn 读词(&mut self) -> Result<String, 表达式错误> {
        let 起点 = self.位置;
        while let Some(字符) = self.当前() {
            if 字符.is_whitespace() || matches!(字符, '(' | ')' | '{' | '}' | '=') {
                break;
            }
            self.位置 += 1;
        }
        if self.位置 == 起点 {
            return Err(表达式错误::空表达式);
        }
        Ok(self.字符表[起点..self.位置].iter().collect())
    }
}

#[cfg(test)]
mod 测试 {
    use super::*;

    fn 求值文本(文本: &str) -> 值 {
        求值(&解析表达式(文本).unwrap(), &|_| None)
    }

    #[test]
    fn 基本类型() {
        assert_eq!(求值文本("42"), 值::整数(42));
        assert_eq!(求值文本("\"你好\""), 值::文本("你好".into()));
        assert_eq!(求值文本("甲"), 值::文本("甲".into()));
    }

    #[test]
    fn 数组与对象() {
        assert_eq!(
            求值文本("(1 2 三)"),
            值::数组(vec![值::整数(1), 值::整数(2), 值::文本("三".into())])
        );
        assert_eq!(
            求值文本("{甲=1 乙=(2 3)}"),
            值::对象(vec![
                ("甲".into(), 值::整数(1)),
                ("乙".into(), 值::数组(vec![值::整数(2), 值::整数(3)])),
            ])
        );
        // 文本形式可原样再读回来
        let 值 = 求值文本("{甲=1}");
        assert_eq!(求值文本(&值.转文本()), 值);
    }

    #[test]
    fn 变量求值() {
        let 表达式 = 解析表达式("($甲 $乙)").unwrap();
        let 值 = 求值(&表达式, &|名| match 名 {
            "甲" => Some("1".into()),
            "乙" => Some("你好".into()),
            _ => None,
        });
        assert_eq!(
            值,
            值::数组(vec![值::文本("1".into()), 值::文本("你好".into())])
        );
    }

    #[test]
    fn JSON往返() {
        for 原文 in [r#"{"a":"x","n":3}"#, r#"[1,2,"三"]"#, r#""文本""#, r#"7"#] {
            let 值 = 值::读JSON(原文).unwrap();
            assert_eq!(值::读JSON(&值.写JSON()).unwrap(), 值);
        }
        assert_eq!(
            值::读JSON(r#"{"n":3}"#).unwrap(),
            值::对象(vec![("n".into(), 值::整数(3))])
        );
        assert!(值::读JSON("{不是JSON}").is_none());
    }

    #[test]
    fn 写出JSON与INI() {
        let 对象 = 值::读JSON(r#"{"名称":"壳","端口":8080,"标签":["甲","乙"],"服务":{"地址":"127.0.0.1:80"}}"#).unwrap();
        let json = 值::按类型写出(&对象.写JSON(), "JSON").unwrap();
        assert!(json.contains("\n  \"名称\": \"壳\""));
        assert_eq!(值::读JSON(&json).unwrap(), 对象);

        let ini = 值::按类型写出(&对象.转文本(), "INI").unwrap();
        let 读回 = 读INI(&ini);
        assert_eq!(
            读回,
            值::对象(vec![
                (
                    "全局".into(),
                    值::对象(vec![
                        ("名称".into(), 值::文本("壳".into())),
                        ("端口".into(), 值::整数(8080)),
                        (
                            "标签".into(),
                            值::数组(vec![值::文本("甲".into()), 值::文本("乙".into())])
                        ),
                    ])
                ),
                (
                    "服务".into(),
                    值::对象(vec![("地址".into(), 值::文本("127.0.0.1:80".into()))])
                ),
            ])
        );
        assert_eq!(值::按类型写出("原样", "文本").unwrap(), "原样");
        assert!(值::按类型写出("x", "XML").is_err());
    }

    #[test]
    fn 解析INI() {
        let 文本 = "[全局]\n名称=壳\n# 注释\n[服务]\n端口=8080\n地址=\"127.0.0.1:80\"\n标签=甲, 乙 ";
        let 结果 = 值::解析文本(文本, "INI").unwrap();
        assert_eq!(
            结果,
            值::对象(vec![
                (
                    "全局".into(),
                    值::对象(vec![("名称".into(), 值::文本("壳".into()))])
                ),
                (
                    "服务".into(),
                    值::对象(vec![
                        ("端口".into(), 值::整数(8080)),
                        ("地址".into(), 值::文本("127.0.0.1:80".into())),
                        (
                            "标签".into(),
                            值::数组(vec![值::文本("甲".into()), 值::文本("乙".into())])
                        ),
                    ])
                ),
            ])
        );
        assert_eq!(结果.写JSON(), r#"{"全局":{"名称":"壳"},"服务":{"端口":8080,"地址":"127.0.0.1:80","标签":["甲","乙"]}}"#);
        // 类型名不区分大小写；不认识的类型报错
        assert!(值::解析文本("{}", "json").is_ok());
        assert!(值::解析文本("{}", "INI").is_ok());
        assert!(值::解析文本("x", "YAML").is_err());
        assert!(值::解析文本("{坏", "JSON").is_err());
        assert_eq!(值::解析文本("原样 # 不处理", "文本").unwrap(), 值::文本("原样 # 不处理".into()));
    }

    #[test]
    fn 表达式错误() {
        assert_eq!(解析表达式("(1 2"), Err(表达式错误::括号未闭合('(')));
        assert_eq!(解析表达式("{甲}"), Err(表达式错误::对象项格式错误));
    }
}
