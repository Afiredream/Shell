//! 极简 libc 绑定：只声明 shell 真正用到的几个函数。
//!
//! 不依赖 `libc` crate。Rust 侧一律加 `os_` 前缀，再用 `#[cfg_attr]` 显式给出
//! 真正的 libc 符号名 —— 否则 `open`/`write`/`dup2` 这类名字会与 std 内部
//! 的绑定混淆，链接阶段会悄悄解析到错误的目标。
#![allow(non_camel_case_types, non_snake_case, clippy::missing_safety_doc)]

use std::ffi::c_int;

pub use std::os::raw::{c_char, c_uint, c_void};

unsafe extern "C" {
    #[cfg_attr(target_os = "linux", link_name = "fork")]
    pub fn os_fork() -> c_int;

    #[cfg_attr(target_os = "linux", link_name = "execvp")]
    pub fn os_exec(文件: *const c_char, 参数: *const *const c_char) -> c_int;

    #[cfg_attr(target_os = "linux", link_name = "waitpid")]
    pub fn os_wait(pid: c_int, 状态: *mut c_int, 选项: c_int) -> c_int;

    #[cfg_attr(target_os = "linux", link_name = "pipe")]
    pub fn os_pipe(fds: *mut c_int) -> c_int;

    #[cfg_attr(target_os = "linux", link_name = "dup")]
    pub fn os_dup(fd: c_int) -> c_int;

    #[cfg_attr(target_os = "linux", link_name = "dup2")]
    pub fn os_dup2(旧: c_int, 新: c_int) -> c_int;

    #[cfg_attr(target_os = "linux", link_name = "close")]
    pub fn os_close(fd: c_int) -> c_int;

    /// `open` 在 glibc 中是可变参数：第三个 mode 只在带 `O_CREAT` 时读取。
    ///
    /// Linux 上必须声明成 `...`（与 glibc 原型一致），否则会与 std 内部
    /// 的 `open` 绑定冲突而编译失败。
    #[cfg_attr(target_os = "linux", link_name = "open")]
    pub fn os_open(路径: *const c_char, 标志: c_int, ...) -> c_int;

    #[cfg_attr(target_os = "linux", link_name = "write")]
    pub fn os_write(fd: c_int, 缓冲: *const c_void, 长度: usize) -> isize;

    #[cfg_attr(target_os = "linux", link_name = "_exit")]
    pub fn os_exit(码: c_int) -> !;

    #[cfg_attr(target_os = "linux", link_name = "signal")]
    pub fn os_signal(信号: c_int, 处理: usize) -> usize;

    #[cfg_attr(target_os = "linux", link_name = "setpgid")]
    pub fn os_setpgid(pid: c_int, 组: c_int) -> c_int;

    #[cfg_attr(target_os = "linux", link_name = "kill")]
    pub fn os_kill(pid: c_int, 信号: c_int) -> c_int;
}

// 信号编号（Linux）
pub const SIGINT: c_int = 2;
pub const SIGTERM: c_int = 15;
pub const SIGCHLD: c_int = 17;
/// waitpid 选项
pub const WNOHANG: c_int = 1;
pub const WUNTRACED: c_int = 2;
/// `signal()` 的「恢复默认行为」/「忽略」两种处理方式
pub const SIG_DFL: usize = 0;
pub const SIG_IGN: usize = 1;

/// 把 SIGINT / SIGTERM 交还给默认处理：后台子进程不能继承 shell 的 `忽略` 设置，
/// 否则 `Ctrl-C` 与 `kill` 都杀不掉它们。
pub fn 恢复信号默认() {
    unsafe {
        os_signal(SIGINT, SIG_DFL);
        os_signal(SIGTERM, SIG_DFL);
        // 后台作业的子进程由 shell 显式回收，不必让它们收到「子进程退出」通知
        os_signal(SIGCHLD, SIG_DFL);
    }
}

// 文件访问模式（与 libc 一致）
pub const O_RDONLY: c_int = 0;
pub const O_WRONLY: c_int = 0o1;
// open 标志位
pub const O_CREAT: c_int = 0o100;
pub const O_APPEND: c_int = 0o2000;
pub const O_TRUNC: c_int = 0o1000;
pub const EINVAL: c_int = 22;

// wait 状态判定宏在 libc 里是宏，这里按 POSIX 语义展开为函数
pub fn WIFEXITED(状态: c_int) -> bool {
    状态 & 0x7f == 0
}
pub fn WIFSIGNALED(状态: c_int) -> bool {
    (((状态 & 0x7f) + 1) as i8) >> 1 > 0
}
pub fn WEXITSTATUS(状态: c_int) -> c_int {
    (状态 >> 8) & 0xff
}
pub fn WTERMSIG(状态: c_int) -> c_int {
    状态 & 0x7f
}
pub fn WIFSTOPPED(状态: c_int) -> bool {
    状态 & 0xff == 0x7f
}
pub fn WSTOPSIG(状态: c_int) -> c_int {
    (状态 >> 8) & 0xff
}
