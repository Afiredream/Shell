//! C 库绑定：通过 `libc` crate 调用，只暴露 shell 真正用到的几个接口。
//!
//! 其余模块只用中文安全封装；C 符号、ABI 常量与 wait 状态判断都来自 libc，
//! 不再手写 `extern "C"` 声明，避免与标准库的绑定冲突。

pub use libc::{c_char, c_int, c_uint, c_void};

/// C 的 `int`
pub type 整数 = c_int;
/// C 的 `unsigned int`
pub type 无符号整数 = c_uint;
/// C 的 `char`
pub type 字符 = c_char;
/// C 的 `void`
pub type 空 = c_void;

// ---------------------------------------------------------------------------
// 进程与描述符：中文安全封装
// ---------------------------------------------------------------------------

/// 创建子进程，父进程得到子进程号，子进程得到 0；失败返回负数。
pub fn 分叉进程() -> 整数 {
    unsafe { libc::fork() }
}

/// 用 `文件名` 与 `argv` 替换当前进程映像（exec）；只有失败才返回。
pub fn 替换运行(文件名: *const 字符, 参数表: *const *const 字符) -> 整数 {
    unsafe { libc::execvp(文件名, 参数表) }
}

/// 等待子进程；`状态` 里写回原始 wait 状态。
pub fn 等待进程(进程号: 整数, 状态: *mut 整数, 选项: 整数) -> 整数 {
    unsafe { libc::waitpid(进程号, 状态, 选项) }
}

/// 创建管道，`描述符对` 写回 `[读端, 写端]`。
pub fn 创建管道(描述符对: *mut 整数) -> 整数 {
    unsafe { libc::pipe(描述符对) }
}

/// 复制一个描述符，返回新描述符。
pub fn 复制描述符(描述符: 整数) -> 整数 {
    unsafe { libc::dup(描述符) }
}

/// 把 `旧描述符` 复制到 `新描述符`（覆盖原有内容）。
pub fn 重定向描述符(旧描述符: 整数, 新描述符: 整数) -> 整数 {
    unsafe { libc::dup2(旧描述符, 新描述符) }
}

/// 关闭描述符。
pub fn 关闭描述符(描述符: 整数) -> 整数 {
    unsafe { libc::close(描述符) }
}

/// 打开文件（只带两个参数的形式，用于不带 `O_CREAT` 的标志）。
pub fn 打开文件(路径: *const 字符, 标志: 整数) -> 整数 {
    unsafe { libc::open(路径, 标志) }
}

/// 打开文件并指定权限（用于带 `O_CREAT` 的标志）。
pub fn 打开新文件(路径: *const 字符, 标志: 整数, 权限: 无符号整数) -> 整数 {
    unsafe { libc::open(路径, 标志, 权限) }
}

/// 往描述符里写字节，返回写入长度（可能短写）。
pub fn 写入描述符(描述符: 整数, 缓冲: *const 空, 长度: usize) -> isize {
    unsafe { libc::write(描述符, 缓冲, 长度) }
}

/// 立即结束当前进程（不跑析构、不刷缓冲）。
pub fn 立即退出(状态码: 整数) -> ! {
    unsafe { libc::_exit(状态码) }
}

/// 设置信号的处理方式（`SIG_IGN` / `SIG_DFL`）。
pub fn 设置信号处理(信号: 整数, 处理方式: usize) -> usize {
    unsafe { libc::signal(信号, 处理方式) }
}

/// 把进程放进（或新建）一个进程组。
pub fn 设置进程组(进程号: 整数, 组号: 整数) -> 整数 {
    unsafe { libc::setpgid(进程号, 组号) }
}

/// 向进程（组）发送信号；`进程号` 为负数时表示整个进程组。
pub fn 发送信号(进程号: 整数, 信号: 整数) -> 整数 {
    unsafe { libc::kill(进程号, 信号) }
}

// ---------------------------------------------------------------------------
// 常量与 wait 状态判定（全部取自 libc）
// ---------------------------------------------------------------------------

pub const 中断信号: 整数 = libc::SIGINT;
pub const 终止信号: 整数 = libc::SIGTERM;
pub const 子进程变化信号: 整数 = libc::SIGCHLD;

/// `signal()` 的「恢复默认行为」/「忽略」两种处理方式
pub const 默认处理: usize = libc::SIG_DFL;
pub const 忽略信号: usize = libc::SIG_IGN;

/// 等待选项
pub const 不阻塞: 整数 = libc::WNOHANG;
pub const 也报告暂停: 整数 = libc::WUNTRACED;

// 文件访问模式与标志位
pub const O_RDONLY: 整数 = libc::O_RDONLY;
pub const O_WRONLY: 整数 = libc::O_WRONLY;
pub const O_CREAT: 整数 = libc::O_CREAT;
pub const O_APPEND: 整数 = libc::O_APPEND;
pub const O_TRUNC: 整数 = libc::O_TRUNC;
pub const EINVAL: 整数 = libc::EINVAL;

// wait 状态判定：保留按 POSIX 语义展开的位运算，避免 libc 宏的 unsafe 调用
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
