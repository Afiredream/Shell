//! 进程编排：管道、重定向、后台作业。
//!
//! 所有系统调用都通过 [`super::系统`] 里手写的 libc 绑定完成
//! （不引入任何第三方 crate）——一个 shell 的骨架，
//! 本质上就是分叉进程、建管道、接描述符、换程序、等进程。
//!
//! 对外接口：[`运行管道`]（管道 + 重定向 + 等待）、[`运行单个`]（无重定向的捷径），
//! 以及给内置命令用的 [`重定向守护`]（临时把标准输入/输出/错误指向文件，离开作用域自动恢复）。

use std::ffi::CString;
use std::process::{Command, ExitStatus};

use super::内置;
use super::系统;
use super::系统::整数;

// 标准流的描述符编号
const 标准输入: 整数 = 0;
const 标准输出: 整数 = 1;
const 标准错误: 整数 = 2;

use std::sync::atomic::{AtomicI32, Ordering};
static 最后后台: AtomicI32 = AtomicI32::new(0);

/// 最近一次后台作业的进程号（供 `$!` 使用）；没有则 0。
pub fn 最后后台进程() -> 整数 {
    最后后台.load(Ordering::Relaxed)
}

/// 一条准备执行的命令：程序（`参数列表[0]`）+ 参数 + 环境变量覆盖。
pub struct 简易命令 {
    pub 参数列表: Vec<String>,
    pub 环境: Vec<(String, String)>,
    /// 命令名是内置命令、且要在子进程里执行（用于管道或后台）
    pub 内置: bool,
}

impl 简易命令 {
    pub fn 程序名(&self) -> String {
        self.参数列表.first().cloned().unwrap_or_default()
    }
}

/// 重定向目标，由释义层解析好。
#[derive(Default)]
pub struct 重定向 {
    pub 输入: Option<String>,
    pub 输出: Option<(String, bool)>,
    pub 错误输出: Option<(String, bool)>,
    /// 标准错误跟随标准输出（`2>&1`）
    pub 合并错误: bool,
}

impl 重定向 {
    pub fn 是否设置(&self) -> bool {
        self.输入.is_some() || self.输出.is_some() || self.错误输出.is_some()
    }
}

/// 一段管道：`命令[0]` 的标准输出接到 `命令[1]` 的标准输入，依此类推。
pub struct 管道片段 {
    pub 命令: Vec<简易命令>,
    /// `错误进管道[i]`：`命令[i]` 的标准错误也进入第 i 段管道
    pub 错误进管道: Vec<bool>,
    pub 重定向: 重定向,
    pub 后台: bool,
    /// 标准输入来自显式 `< 文件`（内置命令据此判断能否从标准输入读）
    pub 输入来自文件: bool,
}

/// 标准输入是不是管道/文件（而不是终端）。
/// 在 shell 进程里查一次：内置命令共用 shell 的标准输入，这样 `printf x | 壳 -c ...` 也算“可读”。
pub fn 标准输入可读() -> bool {
    let mut 状态数据 = [0u8; 144];
    // fstat(0, &st)：st_mode 在 Linux x86_64/aarch64 上偏移 24，宽 4 字节
    let 结果 = unsafe { libc::fstat(0, 状态数据.as_mut_ptr().cast()) };
    if 结果 != 0 {
        return false;
    }
    let 模式 = u32::from_ne_bytes([状态数据[24], 状态数据[25], 状态数据[26], 状态数据[27]]);
    matches!(模式 & 0o170000, 0o010000 | 0o100000 | 0o140000)
}

impl 管道片段 {
    /// 单命令、非后台、且命中内置命令时，返回该内置命令（由 shell 亲自执行）。
    pub fn 内置命令(&self) -> Option<&内置::内置命令> {
        if self.命令.len() != 1 || self.后台 {
            return None;
        }
        内置::按名称查找(self.命令.first()?.参数列表.first()?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum 执行错误 {
    #[error("创建管道失败: {0}")]
    无法创建管道(std::io::Error),
    #[error("复制描述符失败: {0}")]
    无法复制描述符(std::io::Error),
    #[error("{路径}: {原因}")]
    无法打开文件 {
        路径: String,
        原因: std::io::Error,
    },
    #[error("{程序}: {原因}")]
    无法执行 {
        程序: String,
        原因: std::io::Error,
    },
    #[error("等待进程失败: {0}")]
    无法等待进程(std::io::Error),
    #[error("创建子进程失败")]
    子进程创建失败,
}

// ---------------------------------------------------------------------------
// 外部命令
// ---------------------------------------------------------------------------

/// 无管道、无重定向的命令：直接交给标准库。
pub fn 运行单个(命令: &简易命令) -> Result<整数, 执行错误> {
    let 程序名 = 命令.程序名();
    let mut 子进程 = Command::new(&程序名);
    子进程.args(&命令.参数列表[1..]);
    for (键, 值) in &命令.环境 {
        子进程.env(键, 值);
    }
    let 输出状态 =
        子进程.status().map_err(|原因| 执行错误::无法执行 {
            程序: 程序名, 原因
        })?;
    Ok(状态码(输出状态))
}

/// 把 `ExitStatus` 归一成 shell 风格状态码：退出码，或 `128 + 信号`。
pub fn 状态码(状态: ExitStatus) -> 整数 {
    use std::os::unix::process::ExitStatusExt;
    状态
        .code()
        .or_else(|| 状态.signal().map(|信号| 128 + 信号))
        .unwrap_or(1)
}

/// 运行整段管道（可带重定向、可后台），返回 waitpid 风格状态。
pub fn 运行管道(片段: &管道片段) -> Result<整数, 执行错误> {
    let 数量 = 片段.命令.len();
    if 数量 == 0 {
        return Ok(0);
    }

    // 提前在父进程打开所有重定向文件：打不开就直接报错，不留半成品进程
    let 输入文件 = 片段.重定向.输入.as_deref().map(打开读).transpose()?;
    let 输出文件 = 片段
        .重定向
        .输出
        .as_ref()
        .map(|(路径, 追加)| 打开写(路径, *追加))
        .transpose()?;
    let 错误文件 = 片段
        .重定向
        .错误输出
        .as_ref()
        .map(|(路径, 追加)| 打开写(路径, *追加))
        .transpose()?;

    // 管道
    let mut 管道表: Vec<[整数; 2]> = Vec::new();
    for _ in 0..数量.saturating_sub(1) {
        let mut 描述符对: [整数; 2] = [0, 0];
        if 系统::创建管道(描述符对.as_mut_ptr()) != 0 {
            return Err(执行错误::无法创建管道(
                std::io::Error::last_os_error(),
            ));
        }
        管道表.push(描述符对);
    }

    let mut 子进程号: Vec<整数> = Vec::new();
    for (序号, 命令) in 片段.命令.iter().enumerate() {
        let 读管道: Option<整数> = 序号.checked_sub(1).map(|前一段| 管道表[前一段][0]);
        let 写管道: Option<整数> = (序号 + 1 < 数量).then(|| 管道表[序号][1]);
        // 中间段若要求「错误进管道」，其标准错误就连到同一个管道写端
        let 错误接管道 = 写管道.is_some() && 片段.错误进管道.get(序号).copied().unwrap_or(false);

        let 进程号 = 系统::分叉进程();
        if 进程号 < 0 {
            return Err(执行错误::子进程创建失败);
        }
        if 进程号 == 0 {
            // ---------- 子进程 ----------
            let 接好结果 = {
                // 不能继承 shell 的「忽略 SIGINT/SIGTERM」设置
                系统::恢复信号默认();
                // 后台作业：自成一个进程组，否则 shell 退出后它们的标准输入仍连着终端，
                // 会收到 SIGTTIN 被挂起（或把终端输入抢走）
                if 片段.后台 && 系统::设置进程组(0, 0) < 0 {
                    系统::立即退出(127);
                }
                (|| -> Result<(), 执行错误> {
                    // 本进程要用的管道端已经记在 读管道/写管道 里，其余全部关掉
                    for (位置, 描述符) in 管道表.iter().enumerate() {
                        if Some(位置) != 序号.checked_sub(1) {
                            系统::关闭描述符(描述符[0]);
                        }
                        let 本进程要写这条 = 位置 == 序号 && 位置 < 数量 - 1;
                        if !本进程要写这条 {
                            系统::关闭描述符(描述符[1]);
                        }
                    }
                    if let Some(描述符) = 输入文件 {
                        接到标准流(标准输入, 描述符)?;
                        系统::关闭描述符(描述符);
                    } else if let Some(描述符) = 读管道 {
                        接到标准流(标准输入, 描述符)?;
                        系统::关闭描述符(描述符);
                    }
                    if let Some(描述符) = 写管道 {
                        接到标准流(标准输出, 描述符)?;
                        系统::关闭描述符(描述符);
                    } else if let Some(描述符) = 输出文件 {
                        接到标准流(标准输出, 描述符)?;
                        系统::关闭描述符(描述符);
                    }
                    if 错误接管道 {
                        // 标准输出已接在管道写端上，标准错误直接复制它即可
                        接到标准流(标准错误, 标准输出)?;
                    } else if let Some(描述符) = 错误文件 {
                        接到标准流(标准错误, 描述符)?;
                        系统::关闭描述符(描述符);
                    } else if 片段.重定向.合并错误 {
                        接到标准流(标准错误, 标准输出)?;
                    }
                    Ok(())
                })()
            };
            if let Err(错误) = 接好结果 {
                写标准错误(&format!("壳: {错误}\n"));
                系统::立即退出(1);
            }
            替换运行(命令);
        }
        // 父进程也设一次，避免与子进程自身的设置进程组竞争导致时序问题
        if 片段.后台 {
            系统::设置进程组(进程号, 进程号);
        }
        子进程号.push(进程号);
    }

    // ---------- 父进程 ----------
    // 关掉所有管道端与文件描述符，否则最后的读端永远等不到 EOF
    for 描述符 in &管道表 {
        系统::关闭描述符(描述符[0]);
        系统::关闭描述符(描述符[1]);
    }
    for 描述符 in [输入文件, 输出文件, 错误文件].into_iter().flatten() {
        系统::关闭描述符(描述符);
    }

    if 片段.后台 {
        if let Some(&最后) = 子进程号.last() {
            最后后台.store(最后, Ordering::Relaxed);
        }
        for 进程号 in &子进程号 {
            println!("[{进程号}] 已放入后台");
        }
        return Ok(0);
    }

    // 按顺序等待各段；某段被信号打断时转入收尾流程
    let mut 状态 = 0;
    for (序号, &进程号) in 子进程号.iter().enumerate() {
        let (状态码, 终止信号) = 等待进程(进程号, true)?;
        状态 = 状态码;
        // 某一段被信号带走：可能是 SIGPIPE（下游读完）、Ctrl-C 或 SIGTERM，
        // 无论哪种，剩下的段都不该再等下去。
        if 终止信号.is_some() {
            return 收尾(状态, 序号, &子进程号);
        }
    }
    Ok(状态)
}

/// 用目标程序替换当前进程（exec）；正常情况永不返回。
fn 替换运行(命令: &简易命令) -> ! {
    let 程序名 = 命令.程序名();

    // 管道/后台里的内置命令：在子进程里直接跑内置实现，天然不污染父 shell
    // （其中的 `进入`、`压栈`、`变量` 只影响这个子进程）
    if 命令.内置 {
        let 内置命令 = 内置::按名称查找(&程序名).expect("释义层已确认是内置命令");
        // `导入` 在主进程里另有高速实现；走到这里说明是管道/后台里的子进程，
        // 主进程那套回调拿不到，说明一下而不是静默失败
        if matches!(内置命令, 内置::内置命令::加载脚本) && !程序名.starts_with('.') {
            let 提示 = "导入: 不支持在管道/后台里执行。\n";
            unsafe {
                libc::write(2, 提示.as_ptr().cast(), 提示.len());
                libc::_exit(1);
            }
        }
        // 只影响本子进程（分叉之后设置变量不会传回 shell）
        for (键, 值) in &命令.环境 {
            std::env::set_var(键, 值);
        }
        let mut 环境表: Vec<(String, String)> = std::env::vars().collect();
        // 子进程里的内置命令不共享 shell 的目录栈，也不能回头加载脚本
        let mut 空目录栈: Vec<std::path::PathBuf> = Vec::new();
        let mut 空返回值 = String::new();
        let 状态 = 内置命令.运行(
            &命令.参数列表[1..],
            &mut 内置::上下文 {
                环境: &mut 环境表,
                最后状态: 0,
                退出: false,
                目录栈: &mut 空目录栈,
                执行脚本文件: &mut |_| 1,
                返回值对象: &mut 空返回值,
                标准输入是管道: true,
                命令替换: false,
            },
        );
        系统::立即退出(状态);
    }

    let 参数: Vec<CString> = 命令
        .参数列表
        .iter()
        .map(|文本| CString::new(文本.as_str()).unwrap_or_default())
        .collect();
    let 指针表: Vec<*const 系统::字符> = 参数
        .iter()
        .map(|参数| 参数.as_ptr())
        .chain(std::iter::once(std::ptr::null()))
        .collect();

    // 只有失败时才会走到下面：换程序失败要报出原因并按惯例返回 127/126
    系统::替换运行(指针表[0], 指针表.as_ptr());
    let 原因 = std::io::Error::last_os_error();
    let 提示 = match 原因.kind() {
        std::io::ErrorKind::NotFound => format!("壳: {程序名}: 未找到命令\n"),
        std::io::ErrorKind::PermissionDenied => format!("壳: {程序名}: 权限不足\n"),
        _ => format!("壳: {程序名}: {原因}\n"),
    };
    写标准错误(&提示);
    let 退出码 = if 原因.kind() == std::io::ErrorKind::NotFound {
        127
    } else {
        126
    };
    系统::立即退出(退出码);
}

/// 等一个子进程，返回 `(状态码, 终止信号)`；被 Ctrl-Z 暂停时信号位为 `None`。
/// 后台作业不跟踪 Ctrl-Z（`WUNTRACED`），否则 shell 会误以为对方已经结束。
fn 等待进程(
    进程号: 整数, 跟踪暂停: bool
) -> Result<(整数, Option<整数>), 执行错误> {
    let mut 原始状态: 整数 = 0;
    let 选项 = if 跟踪暂停 {
        系统::也报告暂停
    } else {
        0
    };
    loop {
        let 结果 = 系统::等待进程(进程号, &mut 原始状态, 选项);
        if 结果 == 进程号 {
            break;
        }
        if 结果 < 0 {
            let 错误 = std::io::Error::last_os_error();
            if 错误.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(执行错误::无法等待进程(错误));
        }
    }
    Ok(解码状态(原始状态))
}

fn 解码状态(原始状态: 整数) -> (整数, Option<整数>) {
    if 系统::正常退出(原始状态) {
        (系统::退出码(原始状态), None)
    } else if 系统::被信号终止(原始状态) {
        let 信号 = 系统::终止原因(原始状态);
        (128 + 信号, Some(信号))
    } else if 系统::被暂停(原始状态) {
        (128 + 系统::暂停信号(原始状态), None)
    } else {
        (1, None)
    }
}

/// 第 `打断位置` 段是被信号杀掉的（多半是 SIGPIPE：下游已经读完并关闭了管道）。
/// 继续等下游各段，再给上游残留的进程补一刀，最后返回最后一段的状态。
fn 收尾(
    状态: 整数, 打断位置: usize, 子进程号: &[整数]
) -> Result<整数, 执行错误> {
    let mut 状态 = 状态;
    for &进程号 in &子进程号[打断位置 + 1..] {
        状态 = 等待进程(进程号, true)?.0;
    }
    for &进程号 in 子进程号 {
        let mut 原始状态 = 0;
        系统::发送信号(进程号, 系统::终止信号);
        系统::等待进程(进程号, &mut 原始状态, 系统::不阻塞);
    }
    Ok(状态)
}

// ---------------------------------------------------------------------------
// 内置命令的重定向
// ---------------------------------------------------------------------------

/// 内置命令的重定向守护：进入时把标准流指到目标，离开（Drop）时恢复。
pub struct 重定向守护 {
    备份表: Vec<(整数, 整数)>,
}

impl 重定向守护 {
    pub fn 应用(重定向: &重定向) -> Result<Self, 执行错误> {
        let mut 守护 = Self {
            备份表: Vec::new()
        };
        (|| -> Result<(), 执行错误> {
            let mut 待改: Vec<(整数, 整数)> = Vec::new();
            if let Some(路径) = 重定向.输入.as_deref() {
                待改.push((标准输入, 打开读(路径)?));
            }
            if let Some((路径, 追加)) = 重定向.输出.as_ref() {
                待改.push((标准输出, 打开写(路径, *追加)?));
            }
            if let Some((路径, 追加)) = 重定向.错误输出.as_ref() {
                待改.push((标准错误, 打开写(路径, *追加)?));
            }
            for (目标, 描述符) in 待改 {
                守护.备份表.push((目标, 系统::复制描述符(目标)));
                if 系统::重定向描述符(描述符, 目标) < 0 {
                    系统::关闭描述符(描述符);
                    return Err(执行错误::无法复制描述符(
                        std::io::Error::last_os_error(),
                    ));
                }
                系统::关闭描述符(描述符);
            }
            if 重定向.合并错误 && 重定向.错误输出.is_none() {
                守护.备份表.push((标准错误, 系统::复制描述符(标准错误)));
                if 系统::重定向描述符(标准输出, 标准错误) < 0 {
                    return Err(执行错误::无法复制描述符(
                        std::io::Error::last_os_error(),
                    ));
                }
            }
            Ok(())
        })()?;
        Ok(守护)
    }
}

/// 把本进程标准输出接到一个内存管道上（命令替换用）：
/// 管道读端在后台线程里读空，`取走()` 等待写完并返回字节。
pub struct 输出捕获 {
    备份: 整数,
    读线程: Option<std::thread::JoinHandle<Vec<u8>>>,
}

impl 输出捕获 {
    pub fn 开始() -> Result<Self, 执行错误> {
        let mut 管道: [整数; 2] = [0, 0];
        if 系统::创建管道(管道.as_mut_ptr()) != 0 {
            return Err(执行错误::无法创建管道(std::io::Error::last_os_error()));
        }
        // 读端交给线程；写端替换 1 号描述符（替换后马上关掉，读线程用不到它）
        use std::os::unix::io::FromRawFd;
        let 读端 = unsafe { std::fs::File::from_raw_fd(管道[0]) };
        let 备份 = 系统::复制描述符(标准输出);
        if 系统::重定向描述符(管道[1], 标准输出) < 0 {
            系统::关闭描述符(管道[1]);
            return Err(执行错误::无法复制描述符(std::io::Error::last_os_error()));
        }
        系统::关闭描述符(管道[1]);
        // 读端已交给 读端（File 拥有），不能再手动关，否则 IO 安全检查会中止进程
        let 读线 = std::thread::spawn(move || {
            use std::io::Read;
            let mut 文件 = 读端;
            let mut 数据 = Vec::new();
            let _ = 文件.read_to_end(&mut 数据);
            数据
        });
        Ok(Self {
            备份,
            读线程: Some(读线),
        })
    }

    /// 恢复标准输出，等读线程读到 EOF 后取走全部字节。
    pub fn 取走(mut self) -> Vec<u8> {
        use std::io::Write;
        let _ = std::io::stdout().flush();
        系统::重定向描述符(self.备份, 标准输出);
        系统::关闭描述符(self.备份);
        self.备份 = -1;
        match self.读线程.take() {
            Some(线) => 线.join().unwrap_or_default(),
            None => Vec::new(),
        }
    }
}

impl Drop for 输出捕获 {
    fn drop(&mut self) {
        if self.备份 >= 0 {
            系统::重定向描述符(self.备份, 标准输出);
            系统::关闭描述符(self.备份);
            self.备份 = -1;
        }
    }
}

impl Drop for 重定向守护 {
    fn drop(&mut self) {
        // 恢复前先冲刷 Rust 的缓冲输出，否则内容可能落到恢复后的终端里
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        for &(目标, 备份) in &self.备份表 {
            if 备份 >= 0 {
                系统::重定向描述符(备份, 目标);
                系统::关闭描述符(备份);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 小工具
// ---------------------------------------------------------------------------

fn 接到标准流(目标: 整数, 来源: 整数) -> Result<(), 执行错误> {
    if 来源 == 目标 {
        return Ok(());
    }
    if 系统::重定向描述符(来源, 目标) < 0 {
        return Err(执行错误::无法复制描述符(
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

/// 以写方式打开重定向目标（不存在则创建）。
fn 打开写(路径: &str, 追加: bool) -> Result<整数, 执行错误> {
    let 附加标志 = if 追加 {
        系统::O_APPEND
    } else {
        系统::O_TRUNC
    };
    打开(路径, 系统::O_WRONLY | 系统::O_CREAT | 附加标志, true)
}

/// 以只读方式打开输入重定向目标。
fn 打开读(路径: &str) -> Result<整数, 执行错误> {
    打开(路径, 系统::O_RDONLY, false)
}

fn 打开(路径: &str, 标志: 整数, 需要模式: bool) -> Result<整数, 执行错误> {
    let 构造错误 = |原因| 执行错误::无法打开文件 {
        路径: 路径.to_string(),
        原因,
    };
    let 路径字符串 = CString::new(路径)
        .map_err(|_| 构造错误(std::io::Error::from_raw_os_error(系统::EINVAL)))?;
    let 描述符 = {
        if 需要模式 {
            系统::打开新文件(路径字符串.as_ptr(), 标志, 0o644 as 系统::无符号整数)
        } else {
            系统::打开文件(路径字符串.as_ptr(), 标志)
        }
    };
    if 描述符 < 0 {
        return Err(构造错误(std::io::Error::last_os_error()));
    }
    Ok(描述符)
}

fn 写标准错误(文本: &str) {
    系统::写入描述符(标准错误, 文本.as_ptr().cast(), 文本.len());
}
