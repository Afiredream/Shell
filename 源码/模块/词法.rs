//! 词法/语法分析：把一行输入变成「作业」序列，以及执行前的各种展开。
//!
//! 约定：Rust 自己起名的类型、函数、变量、文件名一律用中文；
//! 不翻译的只有三类——语言关键字与标准库接口、外部命令名/命令行选项、
//! 以及与 C 库 ABI 绑定的符号名和常量（它们不是本项目的命名）。
//!
//! 支持的语法：
//! - 管道 `|`、`|&`、`2>&1 |`
//! - 输出重定向 `>` `>>` `&>` `&>>`，错误重定向 `2>` `2>>` `2>&1`，输入 `<`
//! - 后台 `异步 <命令> <参数>`（旧写法 `命令 &` 仍兼容），命令分隔 `;` 与换行
//! - 单引号（完全原样）、双引号（仅 `$` `\` 可转义）、反斜杠转义、`#` 注释
//! - 执行前的 `~`、`$变量` / `${变量}` 展开与 `*` `?` `[...]` 通配展开

use super::值::{self, 表达式, 表达式错误};

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

/// 控制流语句。块体递归包含语句，形成语句树。
#[derive(Debug, Clone)]
pub enum 语句 {
    /// 一段普通命令（由 ; 或换行分隔的作业）
    作业列表(Vec<作业>),
    /// `如果 <条件>` / `或者 <条件>` / `否则` … `结束`
    如果 {
        分支: Vec<(作业, Vec<语句>)>,
        否则体: Option<Vec<语句>>,
    },
    /// `对于 <变量> <词...>` … `结束`
    对于 {
        变量: String,
        取值: Vec<词>,
        体: Vec<语句>,
    },
    /// `每当 <条件>` … `结束`
    每当 { 条件: 作业, 体: Vec<语句> },
    /// 退出最近的 对于/每当
    跳出,
    /// 进入下一次循环
    继续,
    /// `函数 名称 参数...` … `结束`
    函数 {
        名称: String,
        参数: Vec<String>,
        体: Vec<语句>,
    },
    /// `返回 [表达式]`，从函数返回；表达式可以是整数、文本、数组、对象
    返回(Option<表达式>),
}

/// 块级错误，带行号包装成 [`脚本错误`]。
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum 块错误 {
    #[error("{0}")]
    底层(语法错误),
    #[error("「结束」已移除：块改用缩进表示")]
    不再需要结束,
    #[error("「如果」缺少条件")]
    如果缺少条件,
    #[error("「对于」格式应为「对于 变量 词...」")]
    对于格式错误,
    #[error("「或者」没有匹配的「如果」")]
    或者没有匹配的如果,
    #[error("「否则」之后不能再有「或者」")]
    否则之后不能再有或者,
    #[error("「跳出」/「继续」只能用在循环中")]
    循环外跳出或继续,
    #[error("条件不能使用后台（异步 / &）")]
    条件不能后台,
    #[error("「函数」缺少名称")]
    函数缺少名称,
    #[error("「返回」只能用在函数中")]
    返回不在函数中,
    #[error("{0}")]
    表达式错误(表达式错误),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("第 {行} 行：{原因}")]
pub struct 脚本错误 {
    pub 行: usize,
    pub 原因: 块错误,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum 语法错误 {
    #[error("引号 {0} 未闭合")]
    引号未闭合(char),
    #[error("行尾反斜杠没有可转义的字符")]
    反斜杠在行尾,
    #[error("缺少命令")]
    缺少命令,
    #[error("重定向缺少目标文件")]
    重定向缺少目标,
    #[error("意外的符号 `{0}`")]
    意外符号(String),
}

/// 变量名里允许的字符：字母、数字、`_`；中文等东亚文字的标点（。，、；：“演员” 等）不在此列。
fn 是名称字符(字符: char) -> bool {
    if 字符 == '_' {
        return true;
    }
    字符.is_alphanumeric() && !是中文标点(字符)
}

fn 是中文标点(字符: char) -> bool {
    matches!(
        字符,
        '\u{3000}'..='\u{303F}'   // CJK 符号和标点：。、《》「」【】…
        | '\u{FF00}'..='\u{FF0F}' // 全角标点（不含 0-9 A-Z a-z 区间）
        | '\u{FF1A}'..='\u{FF20}'
        | '\u{FF3B}'..='\u{FF40}'
        | '\u{FF5B}'..='\u{FF65}'
    )
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

/// 把整段文本解析为语句树（支持控制流）。普通行内容仍交给 [`解析命令`]，
/// 因此管道、重定向、异步、引号、注释等语法全部原样复用。
pub fn 解析脚本(输入: &str) -> Result<Vec<语句>, 脚本错误> {
    let 合并后 = 合并续行(输入);
    let 行列表: Vec<&str> = 合并后.lines().collect();
    let mut 解析器 = 脚本解析器 {
        根: Vec::new(),
        栈: Vec::new(),
        普通行: Vec::new(),
    };
    for (序号, 原文) in 行列表.iter().enumerate() {
        解析器.处理行(原文, 序号 + 1)?;
    }
    解析器.结束(行列表.len() + 1)
}

/// `\` + 换行是续行，先合并再按行切块，避免块关键字被续行拆开。
fn 合并续行(输入: &str) -> String {
    let mut 结果 = String::new();
    let mut 字符表 = 输入.chars().peekable();
    while let Some(字符) = 字符表.next() {
        if 字符 == '\\' && 字符表.peek() == Some(&'\n') {
            字符表.next();
        } else {
            结果.push(字符);
        }
    }
    结果
}

struct 块帧 {
    缩进: usize,
    类型: 块类型,
}

enum 块类型 {
    如果 {
        分支: Vec<(作业, Vec<语句>)>,
        否则体: Option<Vec<语句>>,
    },
    对于 {
        变量: String,
        取值: Vec<词>,
        体: Vec<语句>,
    },
    每当 {
        条件: 作业,
        体: Vec<语句>,
    },
    函数 {
        名称: String,
        参数: Vec<String>,
        体: Vec<语句>,
    },
}

impl 块类型 {
    fn 转语句(self) -> 语句 {
        match self {
            块类型::如果 { 分支, 否则体, .. } => 语句::如果 { 分支, 否则体 },
            块类型::对于 { 变量, 取值, 体, .. } => 语句::对于 { 变量, 取值, 体 },
            块类型::每当 { 条件, 体, .. } => 语句::每当 { 条件, 体 },
            块类型::函数 { 名称, 参数, 体, .. } => 语句::函数 { 名称, 参数, 体 },
        }
    }
}

/// 前导缩进宽度：空格记 1，制表符记 4。
fn 缩进宽度(行: &str) -> usize {
    行.chars()
        .take_while(|字符| *字符 == ' ' || *字符 == '\t')
        .map(|字符| if 字符 == '\t' { 4 } else { 1 })
        .sum()
}

struct 脚本解析器 {
    根: Vec<语句>,
    栈: Vec<块帧>,
    普通行: Vec<String>,
}

impl 脚本解析器 {
    /// 当前正在填充的语句列表：栈顶块的体，或根。
    fn 当前体_mut(&mut self) -> &mut Vec<语句> {
        if self.栈.is_empty() {
            return &mut self.根;
        }
        match &mut self.栈.last_mut().unwrap().类型 {
            块类型::对于 { 体, .. } | 块类型::每当 { 体, .. } | 块类型::函数 { 体, .. } => 体,
            块类型::如果 {
                分支, 否则体, ..
            } => {
                if let Some(体) = 否则体 {
                    体
                } else {
                    &mut 分支.last_mut().unwrap().1
                }
            }
        }
    }

    fn 加入语句(&mut self, 语句: 语句) {
        self.当前体_mut().push(语句);
    }

    fn 在循环中(&self) -> bool {
        self.栈
            .iter()
            .any(|帧| matches!(帧.类型, 块类型::对于 { .. } | 块类型::每当 { .. }))
    }

    fn 在函数定义中(&self) -> bool {
        self.栈.iter().any(|帧| matches!(帧.类型, 块类型::函数 { .. }))
    }

    /// 结束栈顶块：先把攒着的普通行放进它的体，再把它归约成语句挂到父体。
    fn 归约栈顶(&mut self, 行号: usize) -> Result<(), 脚本错误> {
        self.flush(行号)?;
        let 帧 = self.栈.pop().unwrap();
        self.当前体_mut().push(帧.类型.转语句());
        Ok(())
    }

    /// 把攒着的普通行合并解析后放进当前体。
    fn flush(&mut self, 行号: usize) -> Result<(), 脚本错误> {
        if self.普通行.is_empty() {
            return Ok(());
        }
        let 文本 = self.普通行.join("\n");
        self.普通行.clear();
        let 作业列表 = 解析命令(&文本)
            .map_err(|原因| 脚本错误 { 行: 行号, 原因: 块错误::底层(原因) })?;
        if !作业列表.is_empty() {
            self.加入语句(语句::作业列表(作业列表));
        }
        Ok(())
    }

    fn 解析条件(&self, 文本: &str, 行号: usize) -> Result<作业, 脚本错误> {
        let 作业列表 = 解析命令(文本)
            .map_err(|原因| 脚本错误 { 行: 行号, 原因: 块错误::底层(原因) })?;
        if 作业列表.len() != 1 {
            return Err(脚本错误 { 行: 行号, 原因: 块错误::如果缺少条件 });
        }
        let 作业 = 作业列表.into_iter().next().unwrap();
        if 作业.后台 {
            return Err(脚本错误 { 行: 行号, 原因: 块错误::条件不能后台 });
        }
        Ok(作业)
    }

    fn 解析对于头部(&self, 文本: &str, 行号: usize) -> Result<(String, Vec<词>), 脚本错误> {
        let 格式错 = || 脚本错误 { 行: 行号, 原因: 块错误::对于格式错误 };
        let 作业列表 = 解析命令(文本)
            .map_err(|原因| 脚本错误 { 行: 行号, 原因: 块错误::底层(原因) })?;
        if 作业列表.len() != 1 {
            return Err(格式错());
        }
        let 命令们 = &作业列表[0].管道.命令;
        if 命令们.len() != 1 {
            return Err(格式错());
        }
        let 词列表 = &命令们[0].词列表;
        // 第一个词是循环变量，其余都是取值（不再需要「在」作分隔）
        if 词列表.is_empty() || 词列表[0].文本.is_empty() {
            return Err(格式错());
        }
        Ok((词列表[0].文本.clone(), 词列表[1..].to_vec()))
    }

    fn 解析函数头部(&self, 文本: &str, 行号: usize) -> Result<(String, Vec<String>), 脚本错误> {
        let 错 = || 脚本错误 { 行: 行号, 原因: 块错误::函数缺少名称 };
        let 作业列表 = 解析命令(文本)
            .map_err(|原因| 脚本错误 { 行: 行号, 原因: 块错误::底层(原因) })?;
        if 作业列表.len() != 1 {
            return Err(错());
        }
        let 命令们 = &作业列表[0].管道.命令;
        if 命令们.len() != 1 {
            return Err(错());
        }
        let 词列表 = &命令们[0].词列表;
        if 词列表.is_empty() {
            return Err(错());
        }
        let 名称 = 词列表[0].文本.clone();
        let 参数 = 词列表[1..].iter().map(|字| 字.文本.clone()).collect();
        Ok((名称, 参数))
    }

    fn 处理行(&mut self, 原文: &str, 行号: usize) -> Result<(), 脚本错误> {
        let 去左 = 原文.trim_start();
        if 去左.trim().is_empty() || 去左.starts_with('#') {
            return Ok(());
        }
        let 缩进 = 缩进宽度(原文);
        let 行 = 去左.trim_end();
        let 首词 = 行.split_whitespace().next().unwrap_or("");
        let 余下 = 行[首词.len()..].trim();
        let 是分支 = matches!(首词, "或者" | "否则");

        // 结束所有更深的块
        while self.栈.last().is_some_and(|帧| 帧.缩进 > 缩进) {
            self.归约栈顶(行号)?;
        }
        if 是分支 {
            // 分支必须与同缩进的「如果」对齐
            let 有如果 = self.栈.last().is_some_and(|帧| {
                帧.缩进 == 缩进 && matches!(帧.类型, 块类型::如果 { .. })
            });
            if !有如果 {
                return Err(脚本错误 { 行: 行号, 原因: 块错误::或者没有匹配的如果 });
            }
        } else {
            // 同缩进的块到此结束
            while self.栈.last().is_some_and(|帧| 帧.缩进 == 缩进) {
                self.归约栈顶(行号)?;
            }
        }

        match 首词 {
            "如果" => {
                self.flush(行号)?;
                let 条件 = self.解析条件(余下, 行号)?;
                self.栈.push(块帧 {
                    缩进,
                    类型: 块类型::如果 {
                        分支: vec![(条件, Vec::new())],
                        否则体: None,
                    },
                });
            }
            "或者" => {
                self.flush(行号)?;
                let 条件 = self.解析条件(余下, 行号)?;
                if let Some(帧) = self.栈.last_mut() {
                    if let 块类型::如果 { 分支, 否则体, .. } = &mut 帧.类型 {
                        if 否则体.is_some() {
                            return Err(脚本错误 { 行: 行号, 原因: 块错误::否则之后不能再有或者 });
                        }
                        分支.push((条件, Vec::new()));
                    }
                }
            }
            "否则" => {
                self.flush(行号)?;
                if let Some(帧) = self.栈.last_mut() {
                    if let 块类型::如果 { 否则体, .. } = &mut 帧.类型 {
                        *否则体 = Some(Vec::new());
                    }
                }
            }
            "对于" => {
                self.flush(行号)?;
                let (变量, 取值) = self.解析对于头部(余下, 行号)?;
                self.栈.push(块帧 {
                    缩进,
                    类型: 块类型::对于 { 变量, 取值, 体: Vec::new() },
                });
            }
            "每当" => {
                self.flush(行号)?;
                let 条件 = self.解析条件(余下, 行号)?;
                self.栈.push(块帧 {
                    缩进,
                    类型: 块类型::每当 { 条件, 体: Vec::new() },
                });
            }
            "函数" => {
                self.flush(行号)?;
                let (名称, 参数) = self.解析函数头部(余下, 行号)?;
                self.栈.push(块帧 {
                    缩进,
                    类型: 块类型::函数 { 名称, 参数, 体: Vec::new() },
                });
            }
            "返回" => {
                self.flush(行号)?;
                if !self.在函数定义中() {
                    return Err(脚本错误 { 行: 行号, 原因: 块错误::返回不在函数中 });
                }
                let 表达式 = if 余下.is_empty() {
                    None
                } else {
                    Some(
                        值::解析表达式(余下).map_err(|原因| 脚本错误 {
                            行: 行号,
                            原因: 块错误::表达式错误(原因),
                        })?,
                    )
                };
                self.加入语句(语句::返回(表达式));
            }
            "结束" => {
                return Err(脚本错误 { 行: 行号, 原因: 块错误::不再需要结束 });
            }
            "跳出" | "继续" => {
                self.flush(行号)?;
                if !self.在循环中() {
                    return Err(脚本错误 { 行: 行号, 原因: 块错误::循环外跳出或继续 });
                }
                self.加入语句(if 首词 == "跳出" { 语句::跳出 } else { 语句::继续 });
            }
            _ => self.普通行.push(原文.to_string()),
        }
        Ok(())
    }

    fn 结束(mut self, 行号: usize) -> Result<Vec<语句>, 脚本错误> {
        self.flush(行号)?;
        while !self.栈.is_empty() {
            self.归约栈顶(行号)?;
        }
        Ok(self.根)
    }
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
        // 「异步」是后台关键字：只认命令名位置、未被引号保护的裸词。
        // 命中时把整条作业标记为后台，关键字本身不进入命令词列表。
        if 等号位置.is_none()
            && self.当前命令.词列表.is_empty()
            && 正在拼的词.文本 == "异步"
            && !正在拼的词.含双引号
            && 正在拼的词.字面量区.iter().all(|&字面| !字面)
        {
            self.当前作业.后台 = true;
            return;
        }
        if let Some(位) = 等号位置.filter(|_| self.当前命令.词列表.is_empty()) {
            // `位` 是「已打字数」（字符数），不是字节偏移；中文变量名必须按字符切分
            let 名称字符: Vec<char> = 正在拼的词.文本.chars().take(位).collect();
            let 名称合法 = 名称字符
                .first()
                .is_some_and(|首字| 首字.is_alphabetic() || *首字 == '_')
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
        // 「异步」后面必须跟命令；只写关键字或只有重定向时报错。
        if self.当前作业.后台 && self.当前管道.命令.is_empty() {
            return Err(语法错误::缺少命令);
        }
        if self.当前管道.命令.len() <= self.当前管道.连接.len() && !self.当前管道.命令.is_empty()
        {
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
                // `$(命令)`：当前词以 `$` 结尾，把整段（含嵌套括号与引号）读进词里，
                // 真正执行留给展开阶段
                '(' if self.当前词.as_ref().is_some_and(|词| 词.文本.ends_with('$')) => {
                    self.读命令替换()?;
                }
                '(' | ')' => return Err(语法错误::意外符号(字符.to_string())),
                _ => {
                    if 字符 == '=' && self.赋值等号位置.is_none() {
                        let 已打字数 = self
                            .当前词
                            .as_ref()
                            .map_or(0, |单字| 单字.文本.chars().count());
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

    /// 把 `$(...)` 整段读进当前词；支持嵌套括号与引号内的括号。
    fn 读命令替换(&mut self) -> Result<(), 语法错误> {
        let mut 深度 = 1usize;
        let mut 引号: Option<char> = None;
        self.取当前词().追加('(', false);
        self.当前位置 += 1;
        while let Some(字符) = self.预看(0) {
            self.当前位置 += 1;
            self.取当前词().追加(字符, false);
            if let Some(引) = 引号 {
                if 字符 == 引 {
                    引号 = None;
                }
            } else {
                match 字符 {
                    '\'' | '"' => 引号 = Some(字符),
                    '(' => 深度 += 1,
                    ')' => {
                        深度 -= 1;
                        if 深度 == 0 {
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
        }
        Err(语法错误::引号未闭合(')'))
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
            // 双引号内的 `$(...)`：整段读入，内部的 `"` 不再提前结束外层引号
            if !来自单引号 && 当前字符 == '$' && self.预看(0) == Some('(') {
                self.取当前词().追加('$', false);
                self.读命令替换()?;
                continue;
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
#[allow(clippy::type_complexity)]
pub fn 展开词(
    单字: &词,
    查变量: &dyn Fn(&str) -> Option<String>,
    命令替换: &dyn Fn(&str, &dyn Fn(&str) -> Option<String>) -> String,
) -> Vec<String> {
    let 文本 = 展开变量(单字, 查变量, 命令替换);
    if let Some(匹配列表) = 通配展开(单字, &文本) {
        return 匹配列表;
    }
    vec![文本]
}

/// 变量与 `~` 展开（不做通配）。
#[allow(clippy::type_complexity)]
pub fn 展开变量(
    单字: &词,
    查变量: &dyn Fn(&str) -> Option<String>,
    命令替换: &dyn Fn(&str, &dyn Fn(&str) -> Option<String>) -> String,
) -> String {
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
            // `$(命令)`：执行并把标准输出插进来（结尾换行会被去掉）
            Some('(') => {
                let 命令起点 = 游标 + 2;
                let mut 深度 = 1;
                let mut 终点 = 命令起点;
                while 终点 < 字符表.len() {
                    match 字符表[终点] {
                        '(' => 深度 += 1,
                        ')' => {
                            深度 -= 1;
                            if 深度 == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    终点 += 1;
                }
                if 深度 == 0 {
                    let 命令: String = 字符表[命令起点..终点].iter().collect();
                    结果.push_str(&命令替换(&命令, 查变量));
                    游标 = 终点 + 1;
                } else {
                    结果.push('$');
                    游标 += 1;
                }
            }
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
            Some(&首字) if 是名称字符(首字) => {
                let 名称起点 = 游标 + 1;
                let mut 名称终点 = 名称起点;
                // 名字里可以包含中文（`$名称`），但遇到 ASCII 标点（中文句号、逗号等）就结束，
                // 这样 `你好$名字。` 能把句号留给正文
                while 名称终点 < 字符表.len() && 是名称字符(字符表[名称终点]) {
                    名称终点 += 1;
                }
                let 名称: String = 字符表[名称起点..名称终点].iter().collect();
                结果.push_str(&查变量(&名称).unwrap_or_default());
                游标 = 名称终点;
            }
            // `$?` `$$` `$!` `$#` `$@` `$*`：单字符特殊变量
            Some(&特殊) if matches!(特殊, '?' | '$' | '!' | '#' | '@' | '*') => {
                结果.push_str(&查变量(&特殊.to_string()).unwrap_or_default());
                游标 += 2;
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
    let 字符: Vec<char> = 模式.chars().collect();
    if 单字.文本.chars().count() != 字符.len() {
        return None;
    }
    if !字符.iter().enumerate().any(|(序号, 字)| {
        matches!(字, '*' | '?' | '[') && !单字.字面量区.get(序号).copied().unwrap_or(false)
    }) {
        return None;
    }
    let (目录部分, 文件名模式) = match 模式.rfind('/') {
        Some(位置) => (&模式[..=位置], &模式[位置 + 1..]),
        None => ("", 模式),
    };
    let 要读的目录 = if 目录部分.is_empty() { "." } else { 目录部分 };
    // 模式只编译一次，避免对目录里每个文件重复编译
    let 匹配器 = glob::Pattern::new(&文件名模式.replace("[^", "[!")).ok()?;
    let 隐藏可见 = 文件名模式.starts_with('.');
    let 读目录 = std::fs::read_dir(要读的目录).ok()?;
    let mut 匹配列表: Vec<String> = Vec::new();
    for 条目 in 读目录.flatten() {
        let 文件名 = 条目.file_name().to_string_lossy().into_owned();
        if 文件名.starts_with('.') && !隐藏可见 {
            continue;
        }
        if 匹配器.matches(&文件名) {
            匹配列表.push(format!("{目录部分}{文件名}"));
        }
    }
    匹配列表.sort_unstable();
    if 匹配列表.is_empty() {
        None
    } else {
        Some(匹配列表)
    }
}

/// shell 风格模式匹配（仅测试用，运行时走 `通配展开` 里编译好的 Pattern）。
#[cfg(test)]
pub fn 模式匹配(模式: &str, 文本: &str) -> bool {
    // glob 只认 `[!...]` 取反；本项目兼容 `[^...]`，统一成 `[!`
    let 规范模式 = 模式.replace("[^", "[!");
    match glob::Pattern::new(&规范模式) {
        Ok(模式) => 模式.matches(文本),
        Err(_) => 模式 == 文本,
    }
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
            .flat_map(|单字| 展开词(单字, &测试变量, &|_, _| String::new()))
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
    fn 异步关键字() {
        // 「异步 <命令> <参数>」把整条作业放进后台，关键字不进入词列表
        let 作业列表 = 解析命令("异步 sleep 3; echo hi").unwrap();
        assert_eq!(作业列表.len(), 2);
        assert!(作业列表[0].后台);
        assert_eq!(展开全部(&作业列表[0].管道.命令[0]), ["sleep", "3"]);
        assert!(!作业列表[1].后台);
        assert_eq!(展开全部(&作业列表[1].管道.命令[0]), ["echo", "hi"]);

        // 整条管道一起后台；前置赋值也照常作用于子进程
        let 作业列表 = 解析命令("异步 A=1 echo hi | cat").unwrap();
        assert!(作业列表[0].后台);
        assert_eq!(作业列表[0].管道.命令.len(), 2);
        assert_eq!(作业列表[0].管道.命令[0].赋值.len(), 1);

        // 关键字后面没有命令要报错
        assert_eq!(解析命令("异步").unwrap_err(), 语法错误::缺少命令);
        assert_eq!(解析命令("异步 > 文件").unwrap_err(), 语法错误::缺少命令);

        // 被引号包住时是普通命令名，不是关键字
        for 输入 in ["'异步' arg", "\"异步\" arg"] {
            let 作业列表 = 解析命令(输入).unwrap();
            assert!(!作业列表[0].后台, "输入：{输入}");
            assert_eq!(展开全部(&作业列表[0].管道.命令[0]), ["异步", "arg"]);
        }

        // 关键字出现在参数位置时是普通词
        let 作业列表 = 解析命令("echo 异步").unwrap();
        assert!(!作业列表[0].后台);
        assert_eq!(展开全部(&作业列表[0].管道.命令[0]), ["echo", "异步"]);
    }

    #[test]
    fn 双引号内命令替换含引号() {
        // 外层的双引号里再有 $(...)，内部的引号不能提前结束外层引号
        let 作业列表 = 解析命令(r#"x="$(echo "a b")""#).unwrap();
        let 命令 = &作业列表[0].管道.命令[0];
        assert_eq!(命令.赋值.len(), 1);
        let 值词 = &命令.赋值[0].1;
        let 展开后 = 展开词(值词, &|_| None, &|命令, _| format!("CMD[{命令}]"));
        assert_eq!(展开后, vec!["CMD[echo \"a b\"]"]);
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
        assert_eq!(
            展开全部(&作业列表[0].管道.命令[0]),
            ["echo", "x\"y", r"a\tb"]
        );

        // 单个 `~` 只有不被引号包住才展开
        let 作业列表 = 解析命令(r#"echo ~ '~'"#).unwrap();
        assert_eq!(
            展开全部(&作业列表[0].管道.命令[0]),
            ["echo", "/home/u", "~"]
        );
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
        assert_eq!(
            解析命令("echo 'abc").unwrap_err(),
            语法错误::引号未闭合('\'')
        );
        assert_eq!(
            解析命令("echo \"abc").unwrap_err(),
            语法错误::引号未闭合('"')
        );
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
        assert!(模式匹配("[^a-c]*", "dog")); // `^` 取反与 `!` 等价
        assert!(!模式匹配("[^a-c]*", "cat"));
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
            assert_eq!(
                作业列表[0].输出.as_ref().unwrap().0.文本,
                期望,
                "输入：{输入}"
            );
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

    #[test]
    fn 解析脚本控制流() {
        // 如果 / 否则（缩进表示块体）
        let 语句列表 = 解析脚本("如果 正常\n    echo 是\n否则\n    echo 否").unwrap();
        assert_eq!(语句列表.len(), 1);
        match &语句列表[0] {
            语句::如果 { 分支, 否则体 } => {
                assert_eq!(分支.len(), 1);
                assert_eq!(否则体.as_ref().unwrap().len(), 1);
            }
            _ => panic!("应为如果语句"),
        }

        // 或者分支
        let 语句列表 =
            解析脚本("如果 错误\n    echo 一\n或者 正常\n    echo 二\n否则\n    echo 三").unwrap();
        match &语句列表[0] {
            语句::如果 { 分支, 否则体 } => {
                assert_eq!(分支.len(), 2);
                assert!(否则体.is_some());
            }
            _ => panic!("应为如果语句"),
        }

        // 对于 / 每当
        let 语句列表 = 解析脚本("对于 项 甲 乙\n    echo $项").unwrap();
        match &语句列表[0] {
            语句::对于 { 变量, 取值, 体 } => {
                assert_eq!(变量, "项");
                assert_eq!(取值.len(), 2);
                assert_eq!(体.len(), 1);
            }
            _ => panic!("应为对于语句"),
        }
        let 语句列表 = 解析脚本("每当 正常\n    echo 圈").unwrap();
        assert!(matches!(&语句列表[0], 语句::每当 { .. }));

        // 嵌套（缩进层级）
        let 语句列表 =
            解析脚本("对于 外 甲\n    如果 正常\n        对于 内 一\n            echo x").unwrap();
        assert_eq!(语句列表.len(), 1);

        // 「结束」已移除 / 循环外跳出 / 条件不能后台
        assert_eq!(解析脚本("结束").unwrap_err().原因, 块错误::不再需要结束);
        assert_eq!(
            解析脚本("跳出").unwrap_err().原因,
            块错误::循环外跳出或继续
        );
        assert_eq!(
            解析脚本("如果 异步 true\n    echo x").unwrap_err().原因,
            块错误::条件不能后台
        );

        // 引号包住时不当关键字
        let 语句列表 = 解析脚本("\"如果\" arg").unwrap();
        assert!(matches!(&语句列表[0], 语句::作业列表(_)));
    }

    #[test]
    fn 解析脚本函数() {
        let 语句列表 = 解析脚本("函数 问候 名字\n    echo 你好\n    返回\n问候 世界").unwrap();
        assert_eq!(语句列表.len(), 2);
        match &语句列表[0] {
            语句::函数 { 名称, 参数, 体 } => {
                assert_eq!(名称, "问候");
                assert_eq!(参数.len(), 1);
                assert_eq!(参数[0], "名字");
                assert_eq!(体.len(), 2);
                assert!(matches!(&体[1], 语句::返回(_)));
            }
            _ => panic!("应为函数语句"),
        }
        assert!(matches!(&语句列表[1], 语句::作业列表(_)));

        // 返回在函数外 / 函数缺名称
        assert_eq!(
            解析脚本("返回 1").unwrap_err().原因,
            块错误::返回不在函数中
        );
        assert_eq!(
            解析脚本("函数").unwrap_err().原因,
            块错误::函数缺少名称
        );
    }
}
