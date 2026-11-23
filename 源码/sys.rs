pub(crate) use libc::{c_char, c_int, c_uint, c_void};

pub(crate) fn sys_fork() -> c_int {
    unsafe { libc::fork() }
}

pub(crate) fn sys_execvp(file_name: *const c_char, argv: *const *const c_char) -> c_int {
    unsafe { libc::execvp(file_name, argv) }
}

pub(crate) fn sys_waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int {
    unsafe { libc::waitpid(pid, status, options) }
}

pub(crate) fn sys_pipe(fds: *mut c_int) -> c_int {
    unsafe { libc::pipe(fds) }
}

pub(crate) fn sys_dup(fd: c_int) -> c_int {
    unsafe { libc::dup(fd) }
}

pub(crate) fn sys_dup2(old_fd: c_int, new_fd: c_int) -> c_int {
    unsafe { libc::dup2(old_fd, new_fd) }
}

pub(crate) fn sys_close(fd: c_int) -> c_int {
    unsafe { libc::close(fd) }
}

pub(crate) fn sys_open(path: *const c_char, flags: c_int) -> c_int {
    unsafe { libc::open(path, flags) }
}

pub(crate) fn sys_open_mode(path: *const c_char, flags: c_int, mode: c_uint) -> c_int {
    unsafe { libc::open(path, flags, mode) }
}

pub(crate) fn sys_write(fd: c_int, buf: *const c_void, len: usize) -> isize {
    unsafe { libc::write(fd, buf, len) }
}

pub(crate) fn sys_exit(exit_code: c_int) -> ! {
    unsafe { libc::_exit(exit_code) }
}

pub(crate) fn sys_signal(sig: c_int, handler: usize) -> usize {
    unsafe { libc::signal(sig, handler) }
}

pub(crate) fn sys_setpgid(pid: c_int, pgid: c_int) -> c_int {
    unsafe { libc::setpgid(pid, pgid) }
}

pub(crate) fn sys_kill(pid: c_int, sig: c_int) -> c_int {
    unsafe { libc::kill(pid, sig) }
}

pub(crate) const SIGINT: c_int = libc::SIGINT;

pub(crate) const SIGTERM: c_int = libc::SIGTERM;

pub(crate) const SIGCHLD: c_int = libc::SIGCHLD;

pub(crate) const SIG_DFL: usize = libc::SIG_DFL;

pub(crate) const WNOHANG: c_int = libc::WNOHANG;

pub(crate) const WUNTRACED: c_int = libc::WUNTRACED;

pub(crate) const O_RDONLY: c_int = libc::O_RDONLY;

pub(crate) const O_WRONLY: c_int = libc::O_WRONLY;

pub(crate) const O_CREAT: c_int = libc::O_CREAT;

pub(crate) const O_APPEND: c_int = libc::O_APPEND;

pub(crate) const O_TRUNC: c_int = libc::O_TRUNC;

pub(crate) const EINVAL: c_int = libc::EINVAL;

pub(crate) fn sys_exited(raw_status: c_int) -> bool {
    raw_status & 0x7f == 0
}

pub(crate) fn sys_signaled(raw_status: c_int) -> bool {
    (((raw_status & 0x7f) + 1) as i8) >> 1 > 0
}

pub(crate) fn sys_exit_status(raw_status: c_int) -> c_int {
    (raw_status >> 8) & 0xff
}

pub(crate) fn sys_term_signal(raw_status: c_int) -> c_int {
    raw_status & 0x7f
}

pub(crate) fn sys_stopped(raw_status: c_int) -> bool {
    raw_status & 0xff == 0x7f
}

pub(crate) fn sys_stop_signal(raw_status: c_int) -> c_int {
    (raw_status >> 8) & 0xff
}

pub(crate) fn restore_signals() {
    sys_signal(SIGINT, SIG_DFL);
    sys_signal(SIGTERM, SIG_DFL);
    
    sys_signal(SIGCHLD, SIG_DFL);
}
