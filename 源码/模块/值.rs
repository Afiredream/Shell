//! 运行时值与表达式：整数、文本、数组、对象。
//!
//! 对象只是**有序键值对数据**，没有方法、没有类、没有继承——不是 OOP。
//! 数组与对象可以互相嵌套，也可以用 `$变量` 把 shell 变量的值取进来。

use std::fmt;

use serde::{Deserialize, Serialize};

/// 运行时值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    /// 序列化为 JSON（借助 serde_json）。
    #[allow(non_snake_case)]
    pub fn 转JSON(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// 从 JSON 还原为值。
    #[allow(non_snake_case)]
    pub fn 从JSON(文本: &str) -> Option<Self> {
        serde_json::from_str(文本).ok()
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
    fn json往返() {
        let 值 = 求值文本("{甲=1 列表=(2 三)}");
        let json = 值.转JSON();
        assert_eq!(值::从JSON(&json), Some(值));
    }

    #[test]
    fn 表达式错误() {
        assert_eq!(解析表达式("(1 2"), Err(表达式错误::括号未闭合('(')));
        assert_eq!(解析表达式("{甲}"), Err(表达式错误::对象项格式错误));
    }
}
