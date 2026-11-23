use libc::{c_char, c_int, c_uint};
use std::ffi::CString;
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicI32, Ordering};
use std::collections::HashMap;
use crate::sys::*;
use crate::builtin::*;

pub(crate) const STDIN: c_int = 0;

pub(crate) const STDOUT: c_int = 1;

pub(crate) const STDERR: c_int = 2;

pub(crate) static LAST_BACKGROUND: AtomicI32 = AtomicI32::new(0);

pub(crate) fn clean_reserved_env() {
    for name in ["参数", "参数个数", "参数列表", "参数对象", "脚本路径", "脚本目录", "当前目录"] {
        std::env::remove_var(name);
    }
    for (name, _) in std::env::vars_os() {
        match name.into_string() {
            Ok(name) => {
                if !valid_env_name(&name) {
                    std::env::remove_var(name);
                }
            }
            Err(name) => std::env::remove_var(name),
        }
    }
}

pub(crate) fn valid_env_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    match bytes.next() {
        Some(first) if first == b'_' || first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}

pub(crate) fn last_background_pid() -> c_int {
    LAST_BACKGROUND.load(Ordering::Relaxed)
}

pub(crate) struct SimpleCmd {
    pub(crate) arg_list: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
    
    pub(crate) builtin: bool,
}

impl SimpleCmd {
    pub(crate) fn program_name(&self) -> String {
        self.arg_list.first().cloned().unwrap_or_default()
    }
}

#[derive(Default)]
pub(crate) struct Redirect {
    pub(crate) input: Option<String>,
    pub(crate) out: Option<(String, bool)>,
    pub(crate) stderr_redirect: Option<(String, bool)>,
    
    pub(crate) merge_err: bool,
}

impl Redirect {
    pub(crate) fn is_set(&self) -> bool {
        self.input.is_some() || self.out.is_some() || self.stderr_redirect.is_some()
    }
}

pub(crate) struct PipelineSpec {
    pub(crate) commands: Vec<SimpleCmd>,
    
    pub(crate) err_in_pipe: Vec<bool>,
    pub(crate) redirect: Redirect,
    pub(crate) background: bool,
    
    pub(crate) input_from_file: bool,
}

pub(crate) fn stdin_readable() -> bool {
    let mut stat_buf = [0u8; 144];
    
    let result = unsafe { libc::fstat(0, stat_buf.as_mut_ptr().cast()) };
    if result != 0 {
        return false;
    }
    let pattern = u32::from_ne_bytes([stat_buf[24], stat_buf[25], stat_buf[26], stat_buf[27]]);
    matches!(pattern & 0o170000, 0o010000 | 0o100000 | 0o140000)
}

impl PipelineSpec {
    
    pub(crate) fn builtin(&self) -> Option<&Builtin> {
        if self.commands.len() != 1 || self.background {
            return None;
        }
        find_by_name(self.commands.first()?.arg_list.first()?)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExecError {
    #[error("创建管道失败: {0}")]
    Pipe(std::io::Error),
    #[error("复制描述符失败: {0}")]
    Dup(std::io::Error),
    #[error("{path}: {source}")]
    Open {
        path: String,
        source: std::io::Error,
    },
    #[error("{program}: {source}")]
    Exec {
        program: String,
        source: std::io::Error,
    },
    #[error("未找到命令：{0}")]
    NotFound(String),
    #[error("{0}: 权限不足")]
    PermissionDenied(String),
    #[error("等待进程失败: {0}")]
    Wait(std::io::Error),
    #[error("创建子进程失败")]
    Fork,
}

impl ExecError {
    pub(crate) fn exit_code(&self) -> c_int {
        match self {
            ExecError::NotFound(_) => 127,
            ExecError::PermissionDenied(_) => 126,
            _ => 1,
        }
    }
}

pub(crate) fn run_single(commands: &SimpleCmd) -> Result<c_int, ExecError> {
    let program_name = commands.program_name();
    let mut child = Command::new(&program_name);
    child.args(&commands.arg_list[1..]);
    for (key, val) in &commands.env {
        child.env(key, val);
    }
    clean_reserved_env();
    let status_out = child.status().map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => ExecError::NotFound(program_name.clone()),
        std::io::ErrorKind::PermissionDenied => {
            ExecError::PermissionDenied(program_name.clone())
        }
        _ => ExecError::Exec {
            program: program_name.clone(),
            source,
        },
    })?;
    Ok(exit_code(status_out))
}

pub(crate) fn exit_code(status: ExitStatus) -> c_int {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .or_else(|| status.signal().map(|sig| 128 + sig))
        .unwrap_or(1)
}

pub(crate) fn run_pipeline_ext(spec: &PipelineSpec) -> Result<c_int, ExecError> {
    let count = spec.commands.len();
    if count == 0 {
        return Ok(0);
    }

    
    let input_file = spec.redirect.input.as_deref().map(open_read).transpose()?;
    let output_file = spec
        .redirect
        .out
        .as_ref()
        .map(|(path, push_char)| open_write(path, *push_char))
        .transpose()?;
    let err_file = spec
        .redirect
        .stderr_redirect
        .as_ref()
        .map(|(path, push_char)| open_write(path, *push_char))
        .transpose()?;

    
    // 子进程一关完管道就会 execvp，否则会把含 `%%`/数字开头的变量原样交给 systemd 类服务
    clean_reserved_env();
    let mut pipe_table: Vec<[c_int; 2]> = Vec::new();
    for _ in 0..count.saturating_sub(1) {
        let mut fds: [c_int; 2] = [0, 0];
        if sys_pipe(fds.as_mut_ptr()) != 0 {
            return Err(ExecError::Pipe(
                std::io::Error::last_os_error(),
            ));
        }
        pipe_table.push(fds);
    }

    let mut child_pids: Vec<c_int> = Vec::new();
    for (idx, commands) in spec.commands.iter().enumerate() {
        let read_pipe: Option<c_int> = idx.checked_sub(1).map(|prev| pipe_table[prev][0]);
        let write_pipe: Option<c_int> = (idx + 1 < count).then(|| pipe_table[idx][1]);
        
        let err_to_pipe = write_pipe.is_some() && spec.err_in_pipe.get(idx).copied().unwrap_or(false);

        let pid = sys_fork();
        if pid < 0 {
            return Err(ExecError::Fork);
        }
        if pid == 0 {
            
            let wired = {
                
                restore_signals();
                
                
                if spec.background && sys_setpgid(0, 0) < 0 {
                    sys_exit(127);
                }
                (|| -> Result<(), ExecError> {
                    
                    for (pos, fd) in pipe_table.iter().enumerate() {
                        if Some(pos) != idx.checked_sub(1) {
                            sys_close(fd[0]);
                        }
                        let writes_this = pos == idx && pos < count - 1;
                        if !writes_this {
                            sys_close(fd[1]);
                        }
                    }
                    if let Some(fd) = input_file.or(read_pipe) {
                        dup_close(STDIN, fd)?;
                    }
                    if let Some(fd) = write_pipe.or(output_file) {
                        dup_close(STDOUT, fd)?;
                    }
                    if err_to_pipe || (err_file.is_none() && spec.redirect.merge_err) {
                        dup_to(STDERR, STDOUT)?;
                    } else if let Some(fd) = err_file {
                        dup_close(STDERR, fd)?;
                    }
                    Ok(())
                })()
            };
            if let Err(error) = wired {
                write_stderr(&format!("壳语言：{error}\n"));
                sys_exit(1);
            }
            exec_child(commands);
        }
        
        if spec.background {
            sys_setpgid(pid, pid);
        }
        child_pids.push(pid);
    }

    
    
    for fd in &pipe_table {
        sys_close(fd[0]);
        sys_close(fd[1]);
    }
    for fd in [input_file, output_file, err_file].into_iter().flatten() {
        sys_close(fd);
    }

    if spec.background {
        if let Some(&last_pid) = child_pids.last() {
            LAST_BACKGROUND.store(last_pid, Ordering::Relaxed);
        }
        for pid in &child_pids {
            println!("[{pid}] 已放入后台");
        }
        return Ok(0);
    }

    
    let mut status = 0;
    for (idx, &pid) in child_pids.iter().enumerate() {
        let (exit_code, term_sig) = wait_child(pid, true)?;
        status = exit_code;
        
        
        if term_sig.is_some() {
            return finish(status, idx, &child_pids);
        }
    }
    Ok(status)
}

pub(crate) fn exec_child(commands: &SimpleCmd) -> ! {
    let program_name = commands.program_name();

    
    
    if !commands.env.is_empty() {
        clean_reserved_env();
    }

    if commands.builtin {
        let builtin = find_by_name(&program_name).expect("释义层已确认是内置命令");
        
        
        if matches!(builtin, Builtin::Import) && !program_name.starts_with('.') {
            let message = "加载脚本: 不支持在管道/后台里执行。\n";
            unsafe {
                libc::write(2, message.as_ptr().cast(), message.len());
                libc::_exit(1);
            }
        }
        
        for (key, val) in &commands.env {
            std::env::set_var(key, val);
        }
        let mut env_table: HashMap<String, String> = std::env::vars().collect();
        
        let mut empty_dir_stack: Vec<std::path::PathBuf> = Vec::new();
        let mut empty_ret = String::new();
        let status = builtin.run(
            &commands.arg_list[1..],
            &mut Context {
                env: &mut env_table,
                last_status: 0,
                quit: false,
                dir_stack: &mut empty_dir_stack,
                ret_object: &mut empty_ret,
                stdin_is_pipe: true,
            },
        );
        sys_exit(status);
    }

    let args: Vec<CString> = commands
        .arg_list
        .iter()
        .map(|text| CString::new(text.as_str()).unwrap_or_default())
        .collect();
    let pointers: Vec<*const c_char> = args
        .iter()
        .map(|args| args.as_ptr())
        .chain(std::iter::once(std::ptr::null()))
        .collect();

    
    sys_execvp(pointers[0], pointers.as_ptr());
    let source = std::io::Error::last_os_error();
    let message = match source.kind() {
        std::io::ErrorKind::NotFound => format!("壳语言：未找到命令：{program_name}\n"),
        std::io::ErrorKind::PermissionDenied => format!("壳语言：{program_name}: 权限不足\n"),
        _ => format!("壳语言：{program_name}: {source}\n"),
    };
    write_stderr(&message);
    let exit_code = if source.kind() == std::io::ErrorKind::NotFound {
        127
    } else {
        126
    };
    sys_exit(exit_code);
}

pub(crate) fn wait_child(
    pid: c_int, track_stop: bool
) -> Result<(c_int, Option<c_int>), ExecError> {
    let mut raw_status: c_int = 0;
    let options = if track_stop {
        WUNTRACED
    } else {
        0
    };
    loop {
        let result = sys_waitpid(pid, &mut raw_status, options);
        if result == pid {
            break;
        }
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(ExecError::Wait(error));
        }
    }
    Ok(decode_status(raw_status))
}

pub(crate) fn decode_status(raw_status: c_int) -> (c_int, Option<c_int>) {
    if sys_exited(raw_status) {
        (sys_exit_status(raw_status), None)
    } else if sys_signaled(raw_status) {
        let sig = sys_term_signal(raw_status);
        (128 + sig, Some(sig))
    } else if sys_stopped(raw_status) {
        (128 + sys_stop_signal(raw_status), None)
    } else {
        (1, None)
    }
}

pub(crate) fn finish(
    status: c_int, broken_at: usize, child_pids: &[c_int]
) -> Result<c_int, ExecError> {
    let mut status = status;
    for &pid in &child_pids[broken_at + 1..] {
        status = wait_child(pid, true)?.0;
    }
    for &pid in child_pids {
        let mut raw_status = 0;
        sys_kill(pid, SIGTERM);
        sys_waitpid(pid, &mut raw_status, WNOHANG);
    }
    Ok(status)
}

pub(crate) struct RedirectGuard {
    pub(crate) saved_fds: Vec<(c_int, c_int)>,
}

impl RedirectGuard {
    pub(crate) fn apply(redirect: &Redirect) -> Result<Self, ExecError> {
        let mut guard = Self {
            saved_fds: Vec::new()
        };
        (|| -> Result<(), ExecError> {
            let mut pending: Vec<(c_int, c_int)> = Vec::new();
            if let Some(path) = redirect.input.as_deref() {
                pending.push((STDIN, open_read(path)?));
            }
            if let Some((path, push_char)) = redirect.out.as_ref() {
                pending.push((STDOUT, open_write(path, *push_char)?));
            }
            if let Some((path, push_char)) = redirect.stderr_redirect.as_ref() {
                pending.push((STDERR, open_write(path, *push_char)?));
            }
            for (target, fd) in pending {
                guard.saved_fds.push((target, sys_dup(target)));
                if sys_dup2(fd, target) < 0 {
                    sys_close(fd);
                    return Err(ExecError::Dup(
                        std::io::Error::last_os_error(),
                    ));
                }
                sys_close(fd);
            }
            if redirect.merge_err && redirect.stderr_redirect.is_none() {
                guard.saved_fds.push((STDERR, sys_dup(STDERR)));
                if sys_dup2(STDOUT, STDERR) < 0 {
                    return Err(ExecError::Dup(
                        std::io::Error::last_os_error(),
                    ));
                }
            }
            Ok(())
        })()?;
        Ok(guard)
    }
}

pub(crate) struct OutputCapture {
    pub(crate) saved_fd: c_int,
    pub(crate) reader_thread: Option<std::thread::JoinHandle<Vec<u8>>>,
}

impl OutputCapture {
    pub(crate) fn start() -> Result<Self, ExecError> {
        let mut fds: [c_int; 2] = [0, 0];
        if sys_pipe(fds.as_mut_ptr()) != 0 {
            return Err(ExecError::Pipe(std::io::Error::last_os_error()));
        }
        
        use std::os::unix::io::FromRawFd;
        let read_end = unsafe { std::fs::File::from_raw_fd(fds[0]) };
        let saved_fd = sys_dup(STDOUT);
        if sys_dup2(fds[1], STDOUT) < 0 {
            sys_close(fds[1]);
            return Err(ExecError::Dup(std::io::Error::last_os_error()));
        }
        sys_close(fds[1]);
        
        let reader = std::thread::spawn(move || {
            use std::io::Read;
            let mut file = read_end;
            let mut data = Vec::new();
            let _ = file.read_to_end(&mut data);
            data
        });
        Ok(Self {
            saved_fd,
            reader_thread: Some(reader),
        })
    }

    
    pub(crate) fn take(mut self) -> Vec<u8> {
        use std::io::Write;
        let _ = std::io::stdout().flush();
        sys_dup2(self.saved_fd, STDOUT);
        sys_close(self.saved_fd);
        self.saved_fd = -1;
        match self.reader_thread.take() {
            Some(thread) => thread.join().unwrap_or_default(),
            None => Vec::new(),
        }
    }
}

impl Drop for OutputCapture {
    fn drop(&mut self) {
        if self.saved_fd >= 0 {
            sys_dup2(self.saved_fd, STDOUT);
            sys_close(self.saved_fd);
            self.saved_fd = -1;
        }
    }
}

impl Drop for RedirectGuard {
    fn drop(&mut self) {
        
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        for &(target, saved_fd) in &self.saved_fds {
            if saved_fd >= 0 {
                sys_dup2(saved_fd, target);
                sys_close(saved_fd);
            }
        }
    }
}

pub(crate) fn dup_close(target: c_int, from_fd: c_int) -> Result<(), ExecError> {
    if from_fd != target {
        dup_to(target, from_fd)?;
        sys_close(from_fd);
    }
    Ok(())
}

pub(crate) fn dup_to(target: c_int, from_fd: c_int) -> Result<(), ExecError> {
    if from_fd == target {
        return Ok(());
    }
    if sys_dup2(from_fd, target) < 0 {
        return Err(ExecError::Dup(
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

pub(crate) fn open_write(path: &str, push_char: bool) -> Result<c_int, ExecError> {
    let extra_flags = if push_char {
        O_APPEND
    } else {
        O_TRUNC
    };
    open_fd(path, O_WRONLY | O_CREAT | extra_flags, true)
}

pub(crate) fn open_read(path: &str) -> Result<c_int, ExecError> {
    open_fd(path, O_RDONLY, false)
}

pub(crate) fn open_fd(path: &str, flags: c_int, needs_mode: bool) -> Result<c_int, ExecError> {
    let make_err = |source| ExecError::Open {
        path: path.to_string(),
        source,
    };
    let path_cstr = CString::new(path)
        .map_err(|_| make_err(std::io::Error::from_raw_os_error(EINVAL)))?;
    let fd = {
        if needs_mode {
            sys_open_mode(path_cstr.as_ptr(), flags, 0o644 as c_uint)
        } else {
            sys_open(path_cstr.as_ptr(), flags)
        }
    };
    if fd < 0 {
        return Err(make_err(std::io::Error::last_os_error()));
    }
    Ok(fd)
}

pub(crate) fn write_stderr(text: &str) {
    sys_write(STDERR, text.as_ptr().cast(), text.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_name_rules() {
        for name in ["PATH", "_x", "a1", "A_B", "1password"] {
            assert_eq!(valid_env_name(name), name != "1password", "{name}");
        }
        for name in ["BASH_FUNC_ls%%", "", "1x", "A-B", "中文", "A.B"] {
            assert_eq!(valid_env_name(name), false, "{name}");
        }
    }
}
