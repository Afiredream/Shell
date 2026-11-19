//! 进程编排：管道、重定向、后台作业。
//!
//! 所有系统调用都通过 [`super::系统调用`] 里手写的 libc 绑定完成
//! （不引入任何第三方 crate）——一个 shell 的骨架，
//! 本质上就是 `fork` / `pipe` / `dup2` / `execvp` / `waitpid`。
//!
//! 对外接口：[`运行管道`]（管道 + 重定向 + 等待）、[`运行单个`]（无重定向的捷径），
//! 以及给内置命令用的 [`重定向守护`]（临时把 0/1/2 指向文件，离开作用域自动恢复）。

use std::ffi::CString;
use std::fmt;
use std::process::{Command, ExitStatus};

use super::系统调用 as libc;
use super::内置;

const 标准输入: i32 = 0;
const 标准输出: i32 = 1;
const 标准错误: i32 = 2;

/// 一条准备执行的命令：可执行文件（`参数列表[0]`）+ 参数 + 环境变量覆盖。
pub struct 简易命令 {
    pub 参数列表: Vec<String>,
    pub 环境: Vec<(String, String)>,
    /// 命令名是内置命令、且要在子进程里执行（用于管道或后台）
    pub 内置: bool,
}

impl 简易命令 {
    pub fn 程序(&self) -> String {
        self.参数列表.first().cloned().unwrap_or_default()
    }
}

/// 重定向目标，由释义层解析好。
#[derive(Default)]
pub struct 重定向 {
    pub 输入: Option<String>,
    pub 输出: Option<(String, bool)>,
    pub 错误输出: Option<(String, bool)>,
    /// stderr 跟随 stdout（`2>&1`）
    pub 合并错误: bool,
}

impl 重定向 {
    pub fn 有(&self) -> bool {
        self.输入.is_some() || self.输出.is_some() || self.错误输出.is_some()
    }
}

/// 一段管道：`命令[0]` 的 stdout 接到 `命令[1]` 的 stdin，依此类推。
pub struct 管道片段 {
    pub 命令: Vec<简易命令>,
    /// `管道含错误[i]`：`命令[i]` 的 stderr 也进入第 i 段管道
    pub 管道含错误: Vec<bool>,
    pub 重定向: 重定向,
    pub 后台: bool,
}

impl 管道片段 {
    /// 单命令、非后台、且命中内置命令时，返回该内置命令（由 shell 亲自执行）。
    pub fn 内置命令(&self) -> Option<&内置::内置命令> {
        if self.命令.len() != 1 || self.后台 {
            return None;
        }
        内置::查找(self.命令.first()?.参数列表.first()?)
    }
}

#[derive(Debug)]
pub enum 执行错误 {
    无法创建管道(std::io::Error),
    无法复制描述符(std::io::Error),
    无法打开文件 { 路径: String, 原因: std::io::Error },
    无法执行 { 程序: String, 原因: std::io::Error },
    无法等待(std::io::Error),
    子进程失败,
}

impl fmt::Display for 执行错误 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::无法创建管道(e) => write!(f, "pipe: {e}"),
            Self::无法复制描述符(e) => write!(f, "dup2: {e}"),
            Self::无法打开文件 { 路径, 原因 } => write!(f, "{路径}: {原因}"),
            Self::无法执行 { 程序, 原因 } => write!(f, "{程序}: {原因}"),
            Self::无法等待(e) => write!(f, "waitpid: {e}"),
            Self::子进程失败 => write!(f, "创建子进程失败"),
        }
    }
}

// ---------------------------------------------------------------------------
// 外部命令
// ---------------------------------------------------------------------------

/// 无管道、无重定向的命令：直接交给标准库。
pub fn 运行单个(命令: &简易命令) -> Result<i32, 执行错误> {
    let 程序 = 命令.程序();
    let mut 子进程 = Command::new(&程序);
    子进程.args(&命令.参数列表[1..]);
    for (键, 值) in &命令.环境 {
        子进程.env(键, 值);
    }
    let 输出 = 子进程
        .status()
        .map_err(|原因| 执行错误::无法执行 { 程序, 原因 })?;
    Ok(状态码(输出))
}

/// 把 `ExitStatus` 归一成 shell 风格状态码。
pub fn 状态码(状态: ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    状态
        .code()
        .or_else(|| 状态.signal().map(|信号| 128 + 信号))
        .unwrap_or(1)
}

/// 运行整段管道（可带重定向、可后台），返回 waitpid 风格状态。
pub fn 运行管道(片段: &管道片段) -> Result<i32, 执行错误> {
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
    let mut 管道表: Vec<[i32; 2]> = Vec::new();
    for _ in 0..数量.saturating_sub(1) {
        let mut 对 = [0i32; 2];
        if unsafe { libc::os_pipe(对.as_mut_ptr()) } != 0 {
            return Err(执行错误::无法创建管道(std::io::Error::last_os_error()));
        }
        管道表.push(对);
    }

    let mut 子进程: Vec<i32> = Vec::new();
    for (序号, 命令) in 片段.命令.iter().enumerate() {
        let 读管道: Option<i32> = 序号.checked_sub(1).map(|i| 管道表[i][0]);
        let 写管道: Option<i32> = (序号 + 1 < 数量).then(|| 管道表[序号][1]);
        // 中间段若要求「stderr 进管道」，其 stderr 就连到同一个管道写端
        let 错误进管道 = 写管道.is_some() && 片段.管道含错误.get(序号).copied().unwrap_or(false);

        let pid = unsafe { libc::os_fork() };
        if pid < 0 {
            return Err(执行错误::子进程失败);
        }
        if pid == 0 {
            // ---------- 子进程 ----------
            let 结果 = unsafe {
                // 不能继承 shell 的「忽略 SIGINT/SIGTERM」设置
                libc::恢复信号默认();
                // 后台作业：自成一个进程组，否则 shell 退出后它们的 stdin 仍连着终端，
                // 会收到 SIGTTIN 被挂起（或把终端输入抢走）
                if 片段.后台 && libc::os_setpgid(0, 0) < 0 {
                    libc::os_exit(127);
                }
                // 先 dup2，再关掉其余管道端，顺序不能反
                (|| -> Result<(), 执行错误> {
                    // 本进程要用的管道端已经记在 读管道/写管道 里，其余全部关掉
                    for (i, 对) in 管道表.iter().enumerate() {
                        if Some(i) != 序号.checked_sub(1) {
                            libc::os_close(对[0]);
                        }
                        let 本进程要写这条管道 = i == 序号 && i < 数量 - 1;
                        if !本进程要写这条管道 {
                            libc::os_close(对[1]);
                        }
                    }
                    if let Some(fd) = 输入文件 {
                        复制到(标准输入, fd)?;
                        libc::os_close(fd);
                    } else if let Some(fd) = 读管道 {
                        复制到(标准输入, fd)?;
                        libc::os_close(fd);
                    }
                    if let Some(fd) = 写管道 {
                        复制到(标准输出, fd)?;
                        libc::os_close(fd);
                    } else if let Some(fd) = 输出文件 {
                        复制到(标准输出, fd)?;
                        libc::os_close(fd);
                    }
                    if 错误进管道 {
                        // 标准输出已接在管道写端上，stderr 直接复制它即可
                        复制到(标准错误, 标准输出)?;
                    } else if let Some(fd) = 错误文件 {
                        复制到(标准错误, fd)?;
                        libc::os_close(fd);
                    } else if 片段.重定向.合并错误 {
                        复制到(标准错误, 标准输出)?;
                    }
                    Ok(())
                })()
            };
            if let Err(错误) = 结果 {
                写标准错误(&format!("shell-lite: {错误}\n"));
                unsafe { libc::os_exit(1) };
            }
            执行程序(命令);
        }
        // 父进程也设一次，避免与子进程自身的 setpgid 竞争导致时序问题
        if 片段.后台 {
            unsafe { libc::os_setpgid(pid, pid) };
        }
        子进程.push(pid);
    }

    // ---------- 父进程 ----------
    // 关掉所有管道端与文件描述符，否则最后的读端永远等不到 EOF
    for 对 in &管道表 {
        unsafe {
            libc::os_close(对[0]);
            libc::os_close(对[1]);
        }
    }
    for fd in [输入文件, 输出文件, 错误文件].into_iter().flatten() {
        unsafe { libc::os_close(fd) };
    }

    if 片段.后台 {
        for pid in &子进程 {
            println!("[{pid}] 已放入后台");
        }
        return Ok(0);
    }

    // 按顺序等待各段；某段被信号打断时转入收尾流程
    let mut 状态 = 0;
    for (序号, &pid) in 子进程.iter().enumerate() {
        let (码, 信号) = 等待(pid, true)?;
        状态 = 码;
        // 某一段被信号带走：可能是 SIGPIPE（下游读完）、Ctrl-C 或 SIGTERM，
        // 无论哪种，剩下的段都不该再等下去。
        if 信号.is_some() {
            return 收尾(状态, 序号, &子进程);
        }
    }
    Ok(状态)
}

/// 在子进程中 `execvp`；正常情况永不返回。
fn 执行程序(命令: &简易命令) -> ! {
    let 程序 = 命令.程序();
    let 参数: Vec<CString> = 命令
        .参数列表
        .iter()
        .map(|s| CString::new(s.as_str()).unwrap_or_default())
        .collect();
    let 指针: Vec<*const libc::c_char> = 参数
        .iter()
        .map(|s| s.as_ptr())
        .chain(std::iter::once(std::ptr::null()))
        .collect();
    // 管道/重定向里的内置命令：在子进程里直接跑内置实现，天然不污染父 shell
    if 命令.内置 {
        let 内置命令 = 内置::查找(&程序).expect("释义层已确认是内置命令");
        // 只影响本子进程（fork 之后 setenv 不会传回 shell）
        for (键, 值) in &命令.环境 {
            std::env::set_var(键, 值);
        }
        let mut 变量表: Vec<(String, String)> = std::env::vars().collect();
        let 状态 = 内置命令.运行(
            &命令.参数列表[1..],
            &mut 内置::上下文 { 环境: &mut 变量表, 最后状态: 0, 退出: false },
        );
        unsafe { libc::os_exit(状态) };
    }
    unsafe {
        libc::os_exec(指针[0], 指针.as_ptr());
        let 原因 = std::io::Error::last_os_error();
        let 文本 = match 原因.kind() {
            std::io::ErrorKind::NotFound => format!("shell-lite: {程序}: 未找到命令\n"),
            std::io::ErrorKind::PermissionDenied => format!("shell-lite: {程序}: 权限不足\n"),
            _ => format!("shell-lite: {程序}: {原因}\n"),
        };
        写标准错误(&文本);
        libc::os_exit(if 原因.kind() == std::io::ErrorKind::NotFound { 127 } else { 126 });
    }
}

/// 等一个子进程，返回 `(状态码, 终止信号)`；被 Ctrl-Z 暂停时信号位为 `None`。
/// 后台作业不跟踪 Ctrl-Z（`WUNTRACED`），否则 shell 会误以为对方已经结束。
fn 等待(pid: i32, 跟踪暂停: bool) -> Result<(i32, Option<i32>), 执行错误> {
    let mut 原始: i32 = 0;
    let 选项 = if 跟踪暂停 { libc::WUNTRACED } else { 0 };
    loop {
        let 结果 = unsafe { libc::os_wait(pid, &mut 原始, 选项) };
        if 结果 == pid {
            break;
        }
        if 结果 < 0 {
            let 错误 = std::io::Error::last_os_error();
            if 错误.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(执行错误::无法等待(错误));
        }
    }
    Ok(解码状态(原始))
}

fn 解码状态(原始: i32) -> (i32, Option<i32>) {
    if libc::WIFEXITED(原始) {
        (libc::WEXITSTATUS(原始), None)
    } else if libc::WIFSIGNALED(原始) {
        let 信号 = libc::WTERMSIG(原始);
        (128 + 信号, Some(信号))
    } else if libc::WIFSTOPPED(原始) {
        (128 + libc::WSTOPSIG(原始), None)
    } else {
        (1, None)
    }
}

/// 第 `打断位置` 段是被信号杀掉的（多半是 SIGPIPE：下游已经读完并关闭了管道）。
/// 继续等下游各段，再给上游残留的进程补一刀，最后返回最后一段的状态。
fn 收尾(状态: i32, 打断位置: usize, 子进程: &[i32]) -> Result<i32, 执行错误> {
    let mut 状态 = 状态;
    for &pid in &子进程[打断位置 + 1..] {
        状态 = 等待(pid, true)?.0;
    }
    for &pid in 子进程 {
        let mut 原始 = 0;
        unsafe {
            libc::os_kill(pid, libc::SIGTERM);
            libc::os_wait(pid, &mut 原始, libc::WNOHANG);
        }
    }
    Ok(状态)
}

// ---------------------------------------------------------------------------
// 内置命令的重定向
// ---------------------------------------------------------------------------

/// 内置命令的重定向守卫：进入时把 0/1/2 指到目标，离开（Drop）时恢复。
pub struct 重定向守护 {
    保存: Vec<(i32, i32)>,
}

impl 重定向守护 {
    pub fn 应用(重定向: &重定向) -> Result<Self, 执行错误> {
        let mut 守卫 = Self { 保存: Vec::new() };
        let 结果 = (|| -> Result<(), 执行错误> {
            let mut 要改: Vec<(i32, Option<i32>)> = Vec::new();
            if let Some(路径) = 重定向.输入.as_deref() {
                要改.push((标准输入, Some(打开读(路径)?)));
            }
            if let Some((路径, 追加)) = 重定向.输出.as_ref() {
                要改.push((标准输出, Some(打开写(路径, *追加)?)));
            }
            if let Some((路径, 追加)) = 重定向.错误输出.as_ref() {
                要改.push((标准错误, Some(打开写(路径, *追加)?)));
            }
            for (fd, 打开的文件) in 要改 {
                let Some(打开的文件) = 打开的文件 else {
                    continue;
                };
                守卫.保存.push((fd, unsafe { libc::os_dup(fd) }));
                if unsafe { libc::os_dup2(打开的文件, fd) } < 0 {
                    unsafe { libc::os_close(打开的文件) };
                    return Err(执行错误::无法复制描述符(std::io::Error::last_os_error()));
                }
                unsafe { libc::os_close(打开的文件) };
            }
            if 重定向.合并错误 && 重定向.错误输出.is_none() {
                守卫.保存.push((标准错误, unsafe { libc::os_dup(标准错误) }));
                if unsafe { libc::os_dup2(标准输出, 标准错误) } < 0 {
                    return Err(执行错误::无法复制描述符(std::io::Error::last_os_error()));
                }
            }
            Ok(())
        })();
        结果?;
        Ok(守卫)
    }
}

impl Drop for 重定向守护 {
    fn drop(&mut self) {
        // 恢复前先冲刷 Rust 的缓冲输出，否则内容可能落到恢复后的终端里
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        for &(fd, 备份) in &self.保存 {
            if 备份 >= 0 {
                unsafe {
                    libc::os_dup2(备份, fd);
                    libc::os_close(备份);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 小工具
// ---------------------------------------------------------------------------

unsafe fn 复制到(目标: i32, 源: i32) -> Result<(), 执行错误> {
    if 源 == 目标 {
        return Ok(());
    }
    if libc::os_dup2(源, 目标) < 0 {
        return Err(执行错误::无法复制描述符(std::io::Error::last_os_error()));
    }
    Ok(())
}

/// 以写方式打开重定向目标（不存在则创建）。
fn 打开写(路径: &str, 追加: bool) -> Result<i32, 执行错误> {
    let 附加 = if 追加 { libc::O_APPEND } else { libc::O_TRUNC };
    打开(路径, libc::O_WRONLY | libc::O_CREAT | 附加, true)
}

/// 以只读方式打开输入重定向目标。
fn 打开读(路径: &str) -> Result<i32, 执行错误> {
    打开(路径, libc::O_RDONLY, false)
}

fn 打开(路径: &str, 标志: i32, 需要模式: bool) -> Result<i32, 执行错误> {
    let 错误 = |原因| 执行错误::无法打开文件 { 路径: 路径.to_string(), 原因 };
    let 路径_c =
        CString::new(路径).map_err(|_| 错误(std::io::Error::from_raw_os_error(libc::EINVAL)))?;
    let fd = unsafe {
        if 需要模式 {
            libc::os_open(路径_c.as_ptr(), 标志, 0o644 as libc::c_uint)
        } else {
            libc::os_open(路径_c.as_ptr(), 标志)
        }
    };
    if fd < 0 {
        return Err(错误(std::io::Error::last_os_error()));
    }
    Ok(fd)
}

fn 写标准错误(文本: &str) {
    unsafe {
        libc::os_write(标准错误, 文本.as_ptr().cast(), 文本.len());
    }
}
