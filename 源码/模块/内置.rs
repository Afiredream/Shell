//! 内置命令：必须在 shell 自己进程里完成的命令。
//!
//! 内置命令直接读写标准输入/输出/错误。带重定向的内置命令会由执行层先用
//! 「重定向守护」把真实的 0/1/2 指到目标上，执行完再恢复 —— 这样内置命令
//! 完全不需要关心自己是在终端、管道还是文件里。
//!
//! `内置命令` 的变体用中文命名；`全部` 表里的字符串才是用户在命令行里敲的名字。

use std::io::{self, Write};

use super::系统;
use super::系统::整数;
use super::值::值 as 运行值;

/// 内置命令执行时能看到的上下文。
pub struct 上下文<'a> {
    pub 环境: &'a mut Vec<(String, String)>,
    /// 上一条命令的退出状态（`$?`）
    pub 最后状态: 整数,
    /// 内置命令要求退出 shell 时置位
    pub 退出: bool,
    /// 目录栈（`压栈目录` / `弹栈目录` 使用），栈顶是最后一次压入的目录
    pub 目录栈: &'a mut Vec<std::path::PathBuf>,
    /// 执行脚本文件：由 shell 调度层提供，`加载脚本` 用它逐行执行脚本。
    /// 需要可变访问（改变量、改目录），所以是 `FnMut`。
    pub 执行脚本文件: &'a mut dyn FnMut(&str) -> 整数,
    /// 最近一次 `读取文本`/`读取文件` 的解析结果，写成可再被解析的表达式文本
    pub 返回值对象: &'a mut String,
    /// 命令的标准输入是否为管道（只有这时 `读取文件` 才从标准输入读）
    pub 标准输入是管道: bool,
}

impl 上下文<'_> {
    /// 设置（或新增）一个变量：变量表与进程环境同步更新。
    pub fn 设置变量(&mut self, 名称: &str, 数据: &str) {
        match self.环境.iter_mut().find(|(已有名称, _)| 已有名称 == 名称) {
            Some((_, 旧值)) => *旧值 = 数据.to_string(),
            None => self.环境.push((名称.to_string(), 数据.to_string())),
        }
        std::env::set_var(名称, 数据);
    }

    pub fn 删除变量(&mut self, 名称: &str) {
        self.环境.retain(|(已有名称, _)| 已有名称 != 名称);
        std::env::remove_var(名称);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 内置命令 {
    进入目录,
    压栈目录,
    弹栈目录,
    退出进程,
    加载脚本,
    删除变量,
    输出文本,
    输出,
    替换运行,
    显示帮助,
    真,
    假,
    处理环境变量,
    读取文件,
    覆写文件,
}

/// 关键字 → 内置命令，全部用两字中文名。
const 全部: &[(&str, 内置命令)] = &[
    ("进入", 内置命令::进入目录),
    ("压栈", 内置命令::压栈目录),
    ("弹栈", 内置命令::弹栈目录),
    ("退出", 内置命令::退出进程),
    ("导入", 内置命令::加载脚本),
    ("导出", 内置命令::处理环境变量),
    ("删除", 内置命令::删除变量),
    ("回显", 内置命令::输出文本),
    ("输出", 内置命令::输出),
    ("运行", 内置命令::替换运行),
    ("帮助", 内置命令::显示帮助),
    ("正常", 内置命令::真),
    ("错误", 内置命令::假),
    ("读取文本", 内置命令::读取文件),
    ("读取文件", 内置命令::读取文件),
    ("覆写文件", 内置命令::覆写文件),
];

/// 按命令行里敲的名字查内置命令。
pub fn 按名称查找(名称字符串: &str) -> Option<&'static 内置命令> {
    全部
        .iter()
        .find(|(名字, _)| *名字 == 名称字符串)
        .map(|(_, 命令)| 命令)
}

impl 内置命令 {
    /// 执行，返回退出状态。
    pub fn 运行(&self, 参数: &[String], 上下文: &mut 上下文) -> 整数 {
        match self {
            // `变量` 既负责列出，也负责设置
            Self::处理环境变量 => Self::执行环境变量(参数, 上下文),
            Self::进入目录 => Self::执行进入目录(参数),
            Self::压栈目录 => Self::执行压栈目录(参数, 上下文),
            Self::弹栈目录 => Self::执行弹栈目录(上下文),
            Self::加载脚本 => Self::执行加载脚本(参数, 上下文),
            Self::退出进程 => {
                上下文.退出 = true;
                let 参数里指定的码: Option<整数> = 参数.first().and_then(|文本| 文本.parse().ok());
                参数里指定的码.unwrap_or(上下文.最后状态)
            }
            Self::删除变量 => {
                for 键 in 参数 {
                    上下文.删除变量(键);
                }
                0
            }
            Self::输出文本 => {
                输出文本(参数);
                0
            }
            Self::输出 => {
                直接输出(参数);
                0
            }
            Self::替换运行 => 替换运行命令(参数),
            Self::读取文件 => Self::执行读取文件(参数, 上下文),
            Self::覆写文件 => Self::执行覆写文件(参数, 上下文),
            Self::显示帮助 => {
                帮助();
                0
            }
            Self::真 => 0,
            Self::假 => 1,
        }
    }

    fn 执行进入目录(参数: &[String]) -> 整数 {
        let 目标 = match 参数.first().map(String::as_str) {
            None | Some("") => match dirs::home_dir() {
                Some(家目录) => 家目录.display().to_string(),
                None => return 0,
            },
            Some("-") => std::env::var("OLDPWD").unwrap_or_else(|_| ".".into()),
            Some(其它) => 其它.to_string(),
        };
        let 旧目录 = std::env::current_dir().ok();
        match std::env::set_current_dir(&目标) {
            Ok(()) => {
                if 参数.first().map(String::as_str) == Some("-") {
                    if let Ok(现在目录) = std::env::current_dir() {
                        println!("{}", 现在目录.display());
                    }
                }
                if let Some(旧) = 旧目录 {
                    std::env::set_var("OLDPWD", 旧);
                }
                0
            }
            Err(错误) => {
                let _ = writeln!(io::stderr(), "进入: {目标}: {错误}");
                1
            }
        }
    }

    /// `压栈目录 [目录]`：把目标目录压入目录栈并进入；不带参数时在当前目录与栈顶之间来回切换。
    fn 执行压栈目录(参数: &[String], 上下文: &mut 上下文) -> 整数 {
        let 旧的 = match std::env::current_dir() {
            Ok(路径) => 路径,
            Err(错误) => {
                let _ = writeln!(io::stderr(), "压栈: 无法取得当前目录: {错误}");
                return 1;
            }
        };
        let 目标 = match 参数.first().map(String::as_str) {
            None | Some("") => match 上下文.目录栈.pop() {
                Some(栈顶) => 栈顶,
                None => {
                    let _ = writeln!(io::stderr(), "压栈: 目录栈是空的");
                    return 1;
                }
            },
            Some(目录) => std::path::PathBuf::from(目录),
        };
        if let Err(错误) = std::env::set_current_dir(&目标) {
            // 进不去就把栈恢复原样，什么都不改变
            if 参数.first().is_none_or(|目录| 目录.is_empty()) {
                上下文.目录栈.push(目标.clone());
            }
            let _ = writeln!(io::stderr(), "压栈: {}: {错误}", 目标.display());
            return 1;
        }
        上下文.目录栈.push(旧的);
        if let Ok(现在目录) = std::env::current_dir() {
            std::env::set_var("PWD", &现在目录);
            std::env::set_var("OLDPWD", &目标);
            打印目录栈(&[现在目录], 上下文);
        }
        0
    }

    /// `弹栈目录`：回到目录栈顶的目录，并把它从栈里取出。
    fn 执行弹栈目录(上下文: &mut 上下文) -> 整数 {
        let Some(目标) = 上下文.目录栈.pop() else {
            let _ = writeln!(io::stderr(), "弹栈: 目录栈是空的");
            return 1;
        };
        if let Err(错误) = std::env::set_current_dir(&目标) {
            上下文.目录栈.push(目标.clone());
            let _ = writeln!(io::stderr(), "弹栈: {}: {错误}", 目标.display());
            return 1;
        }
        if let Ok(现在目录) = std::env::current_dir() {
            std::env::set_var("PWD", &现在目录);
            std::env::set_var("OLDPWD", &目标);
            打印目录栈(&[现在目录], 上下文);
        }
        0
    }

    /// `加载脚本 文件`：在当前 shell 里逐行执行脚本（变量、目录变化都会保留）。
    fn 执行加载脚本(参数: &[String], 上下文: &mut 上下文) -> 整数 {
        if 参数.is_empty() {
            let _ = writeln!(io::stderr(), "导入: 缺少脚本文件名");
            return 1;
        }
        let mut 状态 = 0;
        for 文件 in 参数 {
            状态 = (上下文.执行脚本文件)(文件);
            if 上下文.退出 {
                break;
            }
        }
        状态
    }

    /// `读取文本` / `读取文件`：同一个命令（`读取文本` 是主要名字）。
    ///
    /// - 不给 `--类型`：原样打印全文；给文件读文件，不给就读管道标准输入；
    /// - 给出 `--类型`：按该类型解析，再打印对应的 JSON 文本。
    fn 执行读取文件(参数: &[String], 上下文: &mut 上下文) -> 整数 {
        let (文件, 类型) = match 解析读取参数(参数, "读取文件") {
            Ok(结果) => 结果,
            Err(状态) => return 状态,
        };
        let 原文 = match 文件.as_deref() {
            Some(路径) => match std::fs::read_to_string(路径) {
                Ok(文本) => 文本,
                Err(错误) => {
                    let _ = writeln!(io::stderr(), "读取文件: {路径}: {错误}");
                    return 1;
                }
            },
            None if 上下文.标准输入是管道 => {
                let mut 文本 = String::new();
                if let Err(错误) = io::Read::read_to_string(&mut io::stdin().lock(), &mut 文本) {
                    let _ = writeln!(io::stderr(), "读取文件: 标准输入: {错误}");
                    return 1;
                }
                文本
            }
            None => {
                let _ = writeln!(io::stderr(), "读取文件: 缺少文件路径（标准输入不是管道）");
                return 1;
            }
        };
        match 类型 {
            None => {
                直接输出(&[原文]);
                0
            }
            Some(类型名) => match 运行值::解析文本(&原文, &类型名) {
                Ok(解析结果) => {
                    *上下文.返回值对象 = 解析结果.转文本();
                    输出文本(&[解析结果.写JSON()]);
                    0
                }
                Err(消息) => {
                    let _ = writeln!(io::stderr(), "读取文件: （{类型名}）: {消息}");
                    1
                }
            },
        }
    }

    /// `覆写文件 文件 内容... [--类型 类型]`：整份覆写文件，覆盖原有内容。
    ///
    /// - 内容按空格拼接；给了 `--类型` 时按该类型写出（JSON / INI / 文本）；
    /// - 不给 `--类型` 时按扩展名：`.json`、`.ini`、`.conf` 分别按对应格式写，其余原样写文本；
    /// - 内容可以写 `$返回值对象` 这类表达式文本，给了 `--类型` 时会被解析再按格式输出。
    fn 执行覆写文件(参数: &[String], _上下文: &mut 上下文) -> 整数 {
        if 参数.is_empty() {
            let _ = writeln!(io::stderr(), "覆写文件: 缺少文件路径");
            return 1;
        }
        let 文件 = 参数[0].as_str();
        let (指定类型, 内容) = 拆分类型参数(&参数[1..], "覆写文件");
        let 类型 = 指定类型.unwrap_or_else(|| 按扩展名推断写出类型(文件));
        let 文本 = 内容.join(" ");
        let 写出 = 运行值::按类型写出(&文本, &类型);
        match 写出.and_then(|内容| {
            std::fs::write(文件, 内容).map_err(|错误| 错误.to_string())
        }) {
            Ok(()) => 0,
            Err(消息) => {
                let _ = writeln!(io::stderr(), "覆写文件: {文件}（{类型}）: {消息}");
                1
            }
        }
    }

    /// `环境变量` 关键字：
    /// - `环境变量`：列出当前 shell 的全部环境变量
    /// - `环境变量 名称=数据`：设置并导出；数据可带引号也可不带，
    ///   带引号是因为有空格，单引号内是纯文本
    fn 执行环境变量(参数: &[String], 上下文: &mut 上下文) -> 整数 {
        if 参数.is_empty() {
            列出全部变量(上下文);
            return 0;
        }
        let mut 状态 = 0;
        for 项 in 参数 {
            let Some(等号位置) = 项.find('=') else {
                let _ = writeln!(io::stderr(), "导出: {项}: 缺少 =数据");
                状态 = 1;
                continue;
            };
            let 名称 = &项[..等号位置];
            let 数据 = &项[等号位置 + 1..];
            if !是合法变量名(名称) {
                let _ = writeln!(io::stderr(), "导出: {名称}: 不是合法的变量名");
                状态 = 1;
                continue;
            }
            // 数据为空表示删除该变量（与“设置为空”区分开）
            if 数据.is_empty() {
                上下文.删除变量(名称);
            } else {
                上下文.设置变量(名称, 数据);
            }
        }
        状态
    }
}

/// 解析 `读取文本` 的参数：`[文件] [--类型 类型]`。
/// 返回 `(文件, 类型)`；不给文件时文件为 None（只有 `读取文件` 接受），类型为 None 表示按扩展名推断。
#[allow(clippy::type_complexity)]
fn 解析读取参数(参数: &[String], 命令名: &str) -> Result<(Option<String>, Option<String>), 整数> {
    // 第一项不以 `--` 开头就是文件；以 `--` 开头时，只有「后面还带类型选项、且本项不是已知类型名」
    // 才当作文件名（`--配置文件` 这种写法）。
    let 带类型 = 参数.iter().any(|项| 项.starts_with("--"));
    let 文件 = 参数
        .first()
        .filter(|项| {
            if !项.starts_with("--") {
                return true;
            }
            // 以 `--` 开头：`--JSON` 这类已知类型不是文件；其余按文件名处理
            带类型 && !是已知类型(项)
        })
        .cloned();
    // 不给文件时由调用方决定是否读标准输入
    let (指定类型, 多余) = 拆分类型参数(&参数[文件.as_ref().map_or(0, |_| 1)..], 命令名);
    if !多余.is_empty() {
        let _ = writeln!(io::stderr(), "{命令名}: 多余的内容：{}", 多余.join(" "));
        return Err(1);
    }
    let 类型 = match 指定类型 {
        Some(类型) => Some(类型),
        None => 文件.as_deref().map(按扩展名推断类型),
    };
    Ok((文件, 类型))
}

/// 已知类型名才算选项（`--JSON` 是类型，`--配置` 当作文件名）。
fn 是已知类型(项: &str) -> bool {
    let 类型 = 项.trim_start_matches('-').to_ascii_uppercase();
    matches!(类型.as_str(), "JSON" | "INI" | "TEXT" | "文本")
}

/// 从参数里拆出类型选项：`--类型 X`、`--类型=X`、`--X`；返回（类型, 其余参数）。
fn 拆分类型参数(参数: &[String], 命令名: &str) -> (Option<String>, Vec<String>) {
    let mut 指定类型: Option<String> = None;
    let mut 其余 = Vec::new();
    let mut 序 = 0;
    while 序 < 参数.len() {
        let 项 = 参数[序].as_str();
        序 += 1;
        if let Some(类型) = 项.strip_prefix("--类型=") {
            指定类型 = Some(类型.to_string());
        } else if 项 == "--类型" {
            match 参数.get(序) {
                Some(类型) => {
                    指定类型 = Some(类型.clone());
                    序 += 1;
                }
                None => {
                    let _ = writeln!(io::stderr(), "{命令名}: --类型 后面缺少类型名");
                }
            }
        } else if let Some(类型) = 项.strip_prefix("--") {
            指定类型 = Some(类型.to_string());
        } else {
            其余.push(参数[序 - 1].clone());
        }
    }
    (指定类型, 其余)
}

/// 没有 `--类型` 时按扩展名推断：`.ini` → INI，其余（含 `.json`）→ JSON。
fn 按扩展名推断类型(文件: &str) -> String {
    match std::path::Path::new(文件).extension().and_then(|扩展| 扩展.to_str()) {
        Some(扩展) if 扩展.eq_ignore_ascii_case("ini") || 扩展.eq_ignore_ascii_case("conf") => {
            "INI".to_string()
        }
        _ => "JSON".to_string(),
    }
}

/// 写出时的默认类型：`.json` → JSON，`.ini`/`.conf` → INI，其余 → 文本。
fn 按扩展名推断写出类型(文件: &str) -> String {
    match std::path::Path::new(文件).extension().and_then(|扩展| 扩展.to_str()) {
        Some(扩展) if 扩展.eq_ignore_ascii_case("json") => "JSON".to_string(),
        Some(扩展) if 扩展.eq_ignore_ascii_case("ini") || 扩展.eq_ignore_ascii_case("conf") => {
            "INI".to_string()
        }
        _ => "文本".to_string(),
    }
}

/// 变量名：首字符为字母或 `_`，其后为字母、数字、`_`（与赋值规则一致）。
fn 是合法变量名(名称: &str) -> bool {
    let mut 字符迭代 = 名称.chars();
    字符迭代
        .next()
        .is_some_and(|首字| 首字.is_alphabetic() || 首字 == '_')
        && 字符迭代.all(|字符| 字符.is_alphanumeric() || 字符 == '_')
}

/// 打印目录栈：先当前目录，再从栈顶到栈底列出（与 bash 的 `dirs` 顺序一致）。
fn 打印目录栈(当前: &[std::path::PathBuf], 上下文: &上下文) {
    let mut 输出 = String::new();
    for 路径 in 当前 {
        输出.push_str(&format!(" {}", 短路径(路径)));
    }
    for 路径 in 上下文.目录栈.iter().rev() {
        输出.push_str(&format!(" {}", 短路径(路径)));
    }
    println!("{输出}");
}

/// 家目录显示成 `~`，与提示符一致。
fn 短路径(路径: &std::path::Path) -> String {
    let 显示 = 路径.display().to_string();
    match dirs::home_dir() {
        Some(家目录) => {
            let 家 = 家目录.display().to_string();
            if 显示 == 家 {
                "~".to_string()
            } else if 显示.starts_with(&format!("{家}/")) {
                format!("~{}", &显示[家.len()..])
            } else {
                显示
            }
        }
        None => 显示,
    }
}

/// 列出全部变量，每行 `名称=数据`，按名称排序。
fn 列出全部变量(上下文: &mut 上下文) {
    let mut 列表: Vec<&(String, String)> = 上下文.环境.iter().collect();
    列表.sort_by(|甲, 乙| 甲.0.cmp(&乙.0));
    let mut 输出端 = io::stdout().lock();
    for (键, 值) in 列表 {
        let _ = writeln!(输出端, "{键}={值}");
    }
}

/// `替换运行`（旧名 exec）：用给定程序替换当前 shell 进程；无参数时什么都不做。
fn 替换运行命令(参数: &[String]) -> 整数 {
    use std::ffi::CString;

    if 参数.is_empty() {
        return 0;
    }
    // 内置命令已把变量表的改动写进进程环境，exec 会原样继承
    let 参数表: Vec<CString> = 参数
        .iter()
        .map(|文本| CString::new(文本.as_str()).unwrap_or_default())
        .collect();
    let 指针表: Vec<*const 系统::字符> = 参数表
        .iter()
        .map(|参数| 参数.as_ptr())
        .chain(std::iter::once(std::ptr::null()))
        .collect();

    系统::替换运行(指针表[0], 指针表.as_ptr());
    // 只有失败才会走到这里
    let 原因 = io::Error::last_os_error();
    let 提示 = match 原因.kind() {
        io::ErrorKind::NotFound => format!("运行: {}: 未找到命令\n", 参数[0]),
        io::ErrorKind::PermissionDenied => format!("运行: {}: 权限不足\n", 参数[0]),
        _ => format!("运行: {}: {原因}\n", 参数[0]),
    };
    let _ = io::stderr().write_all(提示.as_bytes());
    if 原因.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    }
}

/// `输出文本`（旧名 echo）：支持 `-n`（结尾不换行）与 `-e`（解释 `\n` `\t` 等转义）。
/// `输出`：拼接参数直接写出，不追加回车、不解释转义。
fn 直接输出(参数: &[String]) {
    let 文本 = 参数.join(" ");
    let 标准输出 = io::stdout();
    let mut 输出端 = 标准输出.lock();
    let _ = 输出端.write_all(文本.as_bytes());
    let _ = 输出端.flush();
}

fn 输出文本(参数: &[String]) {
    let mut 要换行 = true;
    let mut 要转义 = false;
    let mut 起点 = 0;
    while let Some(选项) = 参数.get(起点) {
        match 选项.as_str() {
            "-n" => 要换行 = false,
            "-e" => 要转义 = true,
            "-ne" | "-en" => {
                要换行 = false;
                要转义 = true;
            }
            _ => break,
        }
        起点 += 1;
    }
    let mut 文本 = 参数[起点..].join(" ");
    if 要转义 {
        文本 = 解释转义(&文本);
    }
    if 要换行 {
        文本.push('\n');
    }
    let 标准输出 = io::stdout();
    let mut 输出端 = 标准输出.lock();
    let _ = 输出端.write_all(文本.as_bytes());
    let _ = 输出端.flush();
}

fn 解释转义(原文: &str) -> String {
    let mut 结果 = String::with_capacity(原文.len());
    let mut 字符迭代 = 原文.chars();
    while let Some(字符) = 字符迭代.next() {
        if 字符 != '\\' {
            结果.push(字符);
            continue;
        }
        match 字符迭代.next() {
            Some('n') => 结果.push('\n'),
            Some('t') => 结果.push('\t'),
            Some('r') => 结果.push('\r'),
            Some('\\') => 结果.push('\\'),
            Some('e') => 结果.push('\x1b'),
            Some(其它) => {
                结果.push('\\');
                结果.push(其它);
            }
            None => 结果.push('\\'),
        }
    }
    结果
}

pub fn 帮助() {
    println!(
        r##"壳 —— 一个几百行的极简 shell（壳语言）

用法： 壳            进入交互模式
       壳 -c "命令"  执行一段命令后退出（可用换行写控制流）
       壳 脚本 [参数] 运行脚本文件，参数用 $1、$2…… 取到（$0 是脚本名）

语法：
  控制流    如果 条件                              条件退出码 0 为真
            或者 条件 / 否则                        与「如果」同缩进
            对于 变量 词...                         逐个取值赋予变量
            每当 条件                               条件为真时循环
            跳出 / 继续                             退出循环 / 进入下一轮
  函数      函数 名称 参数...                      定义函数（参数用 $参数名 或 $1 取到）
            返回 [表达式]                           从函数返回；整数/文本/数组/对象均可
            数组 (a b)  对象 {{键=值 ...}}          可嵌套；$返回值 取回文本形式
            块体用缩进表示：块头下缩进，回退即结束；交互模式下空行结束当前块
  管道      cmd1 | cmd2          把前一个命令的标准输出接到后一个
            cmd1 2>&1 | cmd2     标准错误也一起进入管道（同 |&）
  重定向    cmd > 文件  >> 文件   覆写 / 追加标准输出
            cmd 2> 文件          重定向标准错误
            cmd < 文件           标准输入来自文件
            cmd &> 文件          标准输出与标准错误一起重定向
  后台      异步 cmd 参数     放到后台执行（旧写法 cmd & 仍可用）
  分隔      cmd1 ; cmd2          依次执行（换行同理）
  引号      单引号内原样；双引号内仅 $ 与 \ 可转义
  变量      $名称 ${{名称}} $0 $1…  $? $# $@ $* $$ $!
            默认参数变量：$参数（第一个）$参数列表 $参数个数 $参数对象（解析后的 JSON）
            全局变量：$当前目录；脚本内：$脚本路径 $脚本目录
  展开      $(命令)（命令替换）、~/路径、* ? [abc]、# 注释

内置命令：
  进入 [目录]        切换目录（不带参数回家，进入 - 回上一次）
  压栈 [目录]        把目录压入目录栈并进入；不带参数时在当前目录与栈顶之间切换
  弹栈               回到目录栈顶的目录，并把它弹出
  导出               列出当前 shell 的全部环境变量
  导出 名=数据        设置并导出环境变量；名= 留空则删除该变量
  删除 名...          删除环境变量
  导入 文件...        在当前 shell 里逐行执行脚本（变量、目录变化都保留）
  输出 文本...        直接输出，不追加回车
  回显 文本...        输出并回车（-n 不回车，-e 解释 \n \t 等转义）
  运行 程序 [参数]   用程序替换当前进程；无参数时什么都不做
  读取文本 [文件] [--类型 类型]  读取文件内容
                     不给 --类型：原样打印全文（给文件则读文件，否则读管道标准输入）
                     给 --类型：按 JSON / INI / 文本 解析，打印 JSON 文本，结果放进 $返回值对象
                     默认类型按扩展名：.ini/.conf 用 INI，其余 JSON
  覆写文件 文件 内容... [--类型 类型]  整份覆写文件
                     不给 --类型：按扩展名写（.json 写 JSON、.ini/.conf 写 INI，其余原样文本）
                     内容可写 $返回值对象；类型不认识或解析失败时不写文件
  正常 / 错误         退出码 0 / 1
  退出 [码]           退出 shell

启动文件： ~/.壳语言rc 会在启动时逐行执行。
其它：     Ctrl-C 中断当前行，Ctrl-D 退出。"##
    );
}

#[cfg(test)]
mod 测试 {
    use super::*;

    /// 测试用上下文；目录栈与脚本回调由调用方持有，保证借用生命周期够长。
    fn 新上下文<'a>(
        环境: &'a mut Vec<(String, String)>,
        目录栈: &'a mut Vec<std::path::PathBuf>,
        脚本: &'a mut dyn FnMut(&str) -> 整数,
    ) -> 上下文<'a> {
        let 返回值 = Box::leak(Box::new(String::new()));
        上下文 {
            环境,
            最后状态: 0,
            退出: false,
            目录栈,
            执行脚本文件: 脚本,
            返回值对象: 返回值,
            标准输入是管道: false,
        }
    }

    #[test]
    fn 读取文件与覆写文件() {
        let 根 = std::env::temp_dir().join("壳读写文件测试");
        let _ = std::fs::create_dir_all(&根);
        let 源 = 根.join("源.json");
        let 出 = 根.join("出.ini");
        std::fs::write(&源, r#"{"名称":"壳","端口":8080}"#).unwrap();
        let mut 环境: Vec<(String, String)> = Vec::new();
        let mut 目录栈: Vec<std::path::PathBuf> = Vec::new();
        let mut 脚本 = |_: &str| 0;

        {
            let 参数 = vec![源.display().to_string()];
            let mut 上下文 = 新上下文(&mut 环境, &mut 目录栈, &mut 脚本);
            assert_eq!(内置命令::读取文件.运行(&参数, &mut 上下文), 0);
            assert_eq!(*上下文.返回值对象, "{名称=壳 端口=8080}");
        }
        {
            let 参数 = vec![出.display().to_string(), "{名称=壳 端口=8080}".into()];
            let mut 上下文 = 新上下文(&mut 环境, &mut 目录栈, &mut 脚本);
            assert_eq!(内置命令::覆写文件.运行(&参数, &mut 上下文), 0);
            let 写回 = std::fs::read_to_string(&出).unwrap();
            assert!(写回.contains("[全局]"));
            assert!(写回.contains("名称=壳"));
        }
        {
            let 原文 = std::fs::read_to_string(&源).unwrap();
            let 参数 = vec![源.display().to_string(), "--类型".into(), "文本".into()];
            let mut 上下文 = 新上下文(&mut 环境, &mut 目录栈, &mut 脚本);
            assert_eq!(内置命令::读取文件.运行(&参数, &mut 上下文), 0);
            assert_eq!(*上下文.返回值对象, 原文);
        }
        {
            let 参数 = vec![源.display().to_string(), "--类型".into(), "不认识".into()];
            let mut 上下文 = 新上下文(&mut 环境, &mut 目录栈, &mut 脚本);
            assert_eq!(内置命令::读取文件.运行(&参数, &mut 上下文), 1);
            assert!(上下文.返回值对象.is_empty());
        }
        {
            // 类型不认识：不写文件
            let 坏 = 根.join("坏.ini");
            let 参数 = vec![坏.display().to_string(), "--类型".into(), "XML".into()];
            let mut 上下文 = 新上下文(&mut 环境, &mut 目录栈, &mut 脚本);
            assert_eq!(内置命令::覆写文件.运行(&参数, &mut 上下文), 1);
            assert!(!坏.exists());
        }
        {
            // 不接管道时无文件应报错
            let mut 上下文 = 新上下文(&mut 环境, &mut 目录栈, &mut 脚本);
            assert_eq!(内置命令::读取文件.运行(&[], &mut 上下文), 1);
        }
        let _ = std::fs::remove_dir_all(&根);
    }

    #[test]
    fn 环境变量列出与设置() {
        let mut 环境: Vec<(String, String)> =
            vec![("乙".into(), "2".into()), ("甲".into(), "1".into())];
        let 命令 = 内置命令::处理环境变量;
        // 无参数：列出，不动内容
        assert_eq!(
            命令.运行(&[], &mut 新上下文(&mut 环境, &mut Vec::new(), &mut |_| 0)),
            0
        );
        // 设置：新增与覆盖
        let 参数 = vec!["甲=覆盖".to_string(), "丙=新".to_string()];
        assert_eq!(
            命令.运行(&参数, &mut 新上下文(&mut 环境, &mut Vec::new(), &mut |_| 0)),
            0
        );
        assert_eq!(环境.iter().find(|(键, _)| 键 == "甲").unwrap().1, "覆盖");
        assert_eq!(环境.iter().find(|(键, _)| 键 == "丙").unwrap().1, "新");
        // 参数展开后不含引号，空格原样保存在数据里
        let 参数 = vec!["带空格的=你好 世界".to_string()];
        assert_eq!(
            命令.运行(&参数, &mut 新上下文(&mut 环境, &mut Vec::new(), &mut |_| 0)),
            0
        );
        assert_eq!(
            环境.iter().find(|(键, _)| 键 == "带空格的").unwrap().1,
            "你好 世界"
        );
        // 空数据表示删除该变量
        assert_eq!(
            命令.运行(
                &["甲=".to_string()],
                &mut 新上下文(&mut 环境, &mut Vec::new(), &mut |_| 0)
            ),
            0
        );
        assert!(环境.iter().all(|(键, _)| 键 != "甲"));
        // 非法参数
        assert_eq!(
            命令.运行(
                &["没有等号".to_string()],
                &mut 新上下文(&mut 环境, &mut Vec::new(), &mut |_| 0)
            ),
            1
        );
        assert_eq!(
            命令.运行(
                &["1坏名=x".to_string()],
                &mut 新上下文(&mut 环境, &mut Vec::new(), &mut |_| 0)
            ),
            1
        );
    }

    #[test]
    fn 中文变量名能传给子进程() {
        // 环境变量只是内存里的字符串表，名字是中文也照样随 exec 传给子进程
        let 命令 = 内置命令::处理环境变量;
        let 参数 = vec!["跨进程名字=壳语言".to_string()];
        let mut 环境: Vec<(String, String)> = Vec::new();
        assert_eq!(
            命令.运行(&参数, &mut 新上下文(&mut 环境, &mut Vec::new(), &mut |_| 0)),
            0
        );
        let 子进程 = std::process::Command::new("sh")
            .arg("-c")
            // sh 的变量名只认 ASCII，所以这里用 env 读，而不是 `$跨进程名字`
            .arg("env | grep ^跨进程名字=")
            .output()
            .expect("无法启动 sh");
        assert_eq!(
            String::from_utf8_lossy(&子进程.stdout),
            "跨进程名字=壳语言\n"
        );
        std::env::remove_var("跨进程名字");
    }

    #[test]
    fn 目录栈压入与弹出() {
        let 原始 = std::env::current_dir().unwrap();
        let 临时 = std::env::temp_dir();
        let mut 环境: Vec<(String, String)> = Vec::new();
        let mut 目录栈: Vec<std::path::PathBuf> = Vec::new();
        let mut 脚本 = |_: &str| 0;
        let 参数 = vec![临时.display().to_string()];

        assert_eq!(
            内置命令::压栈目录.运行(&参数, &mut 新上下文(&mut 环境, &mut 目录栈, &mut 脚本)),
            0
        );
        assert_eq!(std::env::current_dir().unwrap(), 临时);
        assert_eq!(目录栈, vec![原始.clone()]);

        // 不带参数：在当前目录与栈顶之间切换
        assert_eq!(
            内置命令::压栈目录.运行(&[], &mut 新上下文(&mut 环境, &mut 目录栈, &mut 脚本)),
            0
        );
        assert_eq!(std::env::current_dir().unwrap(), 原始);
        assert_eq!(目录栈, vec![临时.clone()]);

        assert_eq!(
            内置命令::弹栈目录.运行(&[], &mut 新上下文(&mut 环境, &mut 目录栈, &mut 脚本)),
            0
        );
        assert_eq!(std::env::current_dir().unwrap(), 临时);
        assert!(目录栈.is_empty());

        // 空栈与不存在的目录
        assert_eq!(
            内置命令::弹栈目录.运行(&[], &mut 新上下文(&mut 环境, &mut 目录栈, &mut 脚本)),
            1
        );
        assert_eq!(
            内置命令::压栈目录.运行(
                &["/不存在的目录".into()],
                &mut 新上下文(&mut 环境, &mut 目录栈, &mut 脚本)
            ),
            1
        );
        assert!(目录栈.is_empty());
        assert_eq!(std::env::current_dir().unwrap(), 临时);

        std::env::set_current_dir(原始).unwrap();
    }

    #[test]
    fn 导入逐行执行() {
        let mut 环境: Vec<(String, String)> = Vec::new();
        let mut 目录栈: Vec<std::path::PathBuf> = Vec::new();
        let mut 已执行: Vec<String> = Vec::new();
        {
            let mut 脚本 = |文件: &str| {
                已执行.push(文件.to_string());
                0
            };
            let 参数 = vec!["一.壳".to_string(), "二.壳".to_string()];
            assert_eq!(
                内置命令::加载脚本.运行(&参数, &mut 新上下文(&mut 环境, &mut 目录栈, &mut 脚本)),
                0
            );
        }
        assert_eq!(已执行, vec!["一.壳", "二.壳"]);
        assert_eq!(
            内置命令::加载脚本.运行(&[], &mut 新上下文(&mut 环境, &mut 目录栈, &mut |_| 0)),
            1
        );
    }

    #[test]
    fn 合法变量名() {
        assert!(是合法变量名("名字"));
        assert!(是合法变量名("_x1"));
        assert!(!是合法变量名(""));
        assert!(!是合法变量名("1名"));
        assert!(!是合法变量名("名 字"));
    }
}
