//! 极简 C 库绑定：只声明 shell 真正用到的几个进程/描述符接口。
//!
//! 不依赖 `libc` crate，全部用 `extern "C"` 手写声明。
//!
//! 为什么 `extern` 块里还是英文函数名：Rust 禁止 `extern` 块使用非 ASCII 标识符
//! （它们同时充当 C 符号名）。因此这里对每个 C 函数做两层包装：
//! 里面一层是 `extern` 声明（用 `r#` 原始标识符避开关键字冲突），
//! 外面一层是同名的中文 `pub fn` 安全封装，其余模块只用中文层。
//!
//! 顶层常量与 wait 状态判断使用中文名；C 库 ABI 里的标志常量（`O_*` 等）保持原名。
#![allow(non_camel_case_types, non_snake_case, clippy::missing_safety_doc)]

use std::ffi::c_void;

pub use std::os::raw::{c_char, c_int, c_uint};

/// C 的 `int`
pub type 整数 = c_int;
/// C 的 `unsigned int`
pub type 无符号整数 = c_uint;
/// C 的 `char`
pub type 字符 = c_char;
/// C 的 `void`
pub type 空 = c_void;

// 真正的 C 符号声明只在本模块内部使用：
#[allow(dead_code)]
mod c接口 {
    use super::{字符, 整数, 空};

    unsafe extern "C" {
        #[cfg_attr(target_os = "linux", link_name = "fork")]
        pub fn fork() -> 整数;

        #[cfg_attr(target_os = "linux", link_name = "execvp")]
        pub unsafe fn r#execvp(文件名: *const 字符, 参数表: *const *const 字符)
            -> 整数;

        #[cfg_attr(target_os = "linux", link_name = "waitpid")]
        pub fn waitpid(进程号: 整数, 状态: *mut 整数, 选项: 整数) -> 整数;

        #[cfg_attr(target_os = "linux", link_name = "pipe")]
        pub fn pipe(描述符对: *mut 整数) -> 整数;

        #[cfg_attr(target_os = "linux", link_name = "dup")]
        pub fn dup(描述符: 整数) -> 整数;

        #[cfg_attr(target_os = "linux", link_name = "dup2")]
        pub fn dup2(旧描述符: 整数, 新描述符: 整数) -> 整数;

        #[cfg_attr(target_os = "linux", link_name = "close")]
        pub fn close(描述符: 整数) -> 整数;

        /// `open` 在 glibc 中是可变参数：第三个 mode 只在带 `O_CREAT` 时读取。
        /// Linux 上必须声明成 `...`（与 glibc 原型一致），否则会与 std 内部的
        /// `open` 绑定冲突而编译失败。
        #[cfg_attr(target_os = "linux", link_name = "open")]
        pub unsafe fn r#open(路径: *const 字符, 标志: 整数, ...) -> 整数;

        #[cfg_attr(target_os = "linux", link_name = "write")]
        pub fn write(描述符: 整数, 缓冲: *const 空, 长度: usize) -> isize;

        #[cfg_attr(target_os = "linux", link_name = "_exit")]
        pub fn _exit(状态码: 整数) -> !;

        #[cfg_attr(target_os = "linux", link_name = "signal")]
        pub fn signal(信号: 整数, 处理方式: usize) -> usize;

        #[cfg_attr(target_os = "linux", link_name = "setpgid")]
        pub fn setpgid(进程号: 整数, 组号: 整数) -> 整数;

        #[cfg_attr(target_os = "linux", link_name = "kill")]
        pub fn kill(进程号: 整数, 信号: 整数) -> 整数;
    }
}

use c接口 as c;

// ---------------------------------------------------------------------------
// 进程与描述符：中文安全封装
// ---------------------------------------------------------------------------

/// 创建子进程，父进程得到子进程号，子进程得到 0；失败返回负数。
pub fn 分叉进程() -> 整数 {
    unsafe { c::fork() }
}

/// 用 `文件名` 与 `argv` 替换当前进程映像（exec）；只有失败才返回。
pub fn 替换运行(文件名: *const 字符, 参数表: *const *const 字符) -> 整数 {
    unsafe { c::r#execvp(文件名, 参数表) }
}

/// 等待子进程；`状态` 里写回原始 wait 状态。
pub fn 等待进程(进程号: 整数, 状态: *mut 整数, 选项: 整数) -> 整数 {
    unsafe { c::waitpid(进程号, 状态, 选项) }
}

/// 创建管道，`描述符对` 写回 `[读端, 写端]`。
pub fn 创建管道(描述符对: *mut 整数) -> 整数 {
    unsafe { c::pipe(描述符对) }
}

/// 复制一个描述符，返回新描述符。
pub fn 复制描述符(描述符: 整数) -> 整数 {
    unsafe { c::dup(描述符) }
}

/// 把 `旧描述符` 复制到 `新描述符`（覆盖原有内容）。
pub fn 重定向描述符(旧描述符: 整数, 新描述符: 整数) -> 整数 {
    unsafe { c::dup2(旧描述符, 新描述符) }
}

/// 关闭描述符。
pub fn 关闭描述符(描述符: 整数) -> 整数 {
    unsafe { c::close(描述符) }
}

/// 打开文件（只带两个参数的形式，用于不带 `O_CREAT` 的标志）。
pub fn 打开文件(路径: *const 字符, 标志: 整数) -> 整数 {
    unsafe { c::r#open(路径, 标志) }
}

/// 打开文件并指定权限（用于带 `O_CREAT` 的标志）。
pub fn 打开新文件(路径: *const 字符, 标志: 整数, 权限: 无符号整数) -> 整数 {
    unsafe { c::r#open(路径, 标志, 权限) }
}

/// 往描述符里写字节，返回写入长度（可能短写）。
pub fn 写入描述符(描述符: 整数, 缓冲: *const 空, 长度: usize) -> isize {
    unsafe { c::write(描述符, 缓冲, 长度) }
}

/// 立即结束当前进程（不跑析构、不刷缓冲）。
pub fn 立即退出(状态码: 整数) -> ! {
    unsafe { c::_exit(状态码) }
}

/// 设置信号的处理方式（`SIG_IGN` / `SIG_DFL`）。
pub fn 设置信号处理(信号: 整数, 处理方式: usize) -> usize {
    unsafe { c::signal(信号, 处理方式) }
}

/// 把进程放进（或新建）一个进程组。
pub fn 设置进程组(进程号: 整数, 组号: 整数) -> 整数 {
    unsafe { c::setpgid(进程号, 组号) }
}

/// 向进程（组）发送信号；`进程号` 为负数时表示整个进程组。
pub fn 发送信号(进程号: 整数, 信号: 整数) -> 整数 {
    unsafe { c::kill(进程号, 信号) }
}

// ---------------------------------------------------------------------------
// 常量与 wait 状态判定
// ---------------------------------------------------------------------------

// 信号编号（Linux 的 ABI 数值）
pub const 中断信号: 整数 = 2;
pub const 终止信号: 整数 = 15;
pub const 子进程变化信号: 整数 = 17;

/// `signal()` 的「恢复默认行为」/「忽略」两种处理方式
pub const 默认处理: usize = 0;
pub const 忽略信号: usize = 1;

/// 等待选项
pub const 不阻塞: 整数 = 1;
pub const 也报告暂停: 整数 = 2;

// 文件访问模式（与 C 库一致，属于 ABI 常量）
pub const O_RDONLY: 整数 = 0;
pub const O_WRONLY: 整数 = 0o1;
// 打开文件时的标志位
pub const O_CREAT: 整数 = 0o100;
pub const O_APPEND: 整数 = 0o2000;
pub const O_TRUNC: 整数 = 0o1000;
pub const EINVAL: 整数 = 22;

// wait 状态判定在 C 库中是宏，这里按 POSIX 语义展开为函数
pub fn 正常退出(原始状态: 整数) -> bool {
    原始状态 & 0x7f == 0
}
pub fn 被信号终止(原始状态: 整数) -> bool {
    (((原始状态 & 0x7f) + 1) as i8) >> 1 > 0
}
pub fn 退出码(原始状态: 整数) -> 整数 {
    (原始状态 >> 8) & 0xff
}
pub fn 终止原因(原始状态: 整数) -> 整数 {
    原始状态 & 0x7f
}
pub fn 被暂停(原始状态: 整数) -> bool {
    原始状态 & 0xff == 0x7f
}
pub fn 暂停信号(原始状态: 整数) -> 整数 {
    (原始状态 >> 8) & 0xff
}

/// 把中断与终止信号交还给默认处理：后台子进程不能继承 shell 的「忽略」设置，
/// 否则 `Ctrl-C` 与 `kill` 都杀不掉它们。
pub fn 恢复信号默认() {
    设置信号处理(中断信号, 默认处理);
    设置信号处理(终止信号, 默认处理);
    // 后台作业的子进程由 shell 显式回收，不必让它们收到「子进程退出」通知
    设置信号处理(子进程变化信号, 默认处理);
}
