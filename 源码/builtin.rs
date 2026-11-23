use libc::{c_char, c_int};
use std::io::{self, Write};
use std::collections::HashMap;
use crate::sys::*;
use crate::value::*;

pub(crate) struct Context<'a> {
    pub(crate) env: &'a mut HashMap<String, String>,
    pub(crate) last_status: c_int,
    pub(crate) quit: bool,
    pub(crate) dir_stack: &'a mut Vec<std::path::PathBuf>,
    pub(crate) ret_object: &'a mut String,
    
    pub(crate) stdin_is_pipe: bool,
}

impl Context<'_> {
    
    pub(crate) fn set_var(&mut self, name: &str, data: &str) {
        self.env.insert(name.to_string(), data.to_string());
        std::env::set_var(name, data);
    }

    pub(crate) fn delete_var(&mut self, name: &str) {
        self.env.remove(name);
        std::env::remove_var(name);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Builtin {
    EnterDir,
    PushDir,
    PopDir,
    Exit,
    Import,
    DeleteVar,
    Echo,
    Print,
    Exec,
    True,
    False,
    Export,
    ReadFile,
    WriteFile,
}

pub(crate) const ALL: &[(&str, Builtin)] = &[
    ("进入", Builtin::EnterDir),
    ("压栈", Builtin::PushDir),
    ("弹栈", Builtin::PopDir),
    ("退出", Builtin::Exit),
    ("导入", Builtin::Import),
    ("导出", Builtin::Export),
    ("删除", Builtin::DeleteVar),
    ("回显", Builtin::Echo),
    ("输出", Builtin::Print),
    ("运行", Builtin::Exec),
    ("正常", Builtin::True),
    ("错误", Builtin::False),
    ("读取文本", Builtin::ReadFile),
    ("读取文件", Builtin::ReadFile),
    ("覆写文件", Builtin::WriteFile),
];

pub(crate) fn find_by_name(name_str: &str) -> Option<&'static Builtin> {
    ALL
        .iter()
        .find(|(entry_name, _)| *entry_name == name_str)
        .map(|(_, commands)| commands)
}

impl Builtin {
    
    pub(crate) fn run(&self, args: &[String], ctx: &mut Context) -> c_int {
        match self {
            
            Self::Export => Self::run_export(args, ctx),
            Self::EnterDir => Self::run_cd(args),
            Self::PushDir => Self::run_push(args, ctx),
            Self::PopDir => Self::run_pop(ctx),
            Self::Import => 1,
            Self::Exit => {
                ctx.quit = true;
                let requested_code: Option<c_int> = args.first().and_then(|text| text.parse().ok());
                requested_code.unwrap_or(ctx.last_status)
            }
            Self::DeleteVar => {
                for key in args {
                    ctx.delete_var(key);
                }
                0
            }
            Self::Echo => {
                echo(args);
                0
            }
            Self::Print => {
                print_raw(args);
                0
            }
            Self::Exec => exec_replace(args),
            Self::ReadFile => Self::run_read_file(args, ctx),
            Self::WriteFile => Self::run_write_file(args, ctx),
            Self::True => 0,
            Self::False => 1,
        }
    }

    pub(crate) fn run_cd(args: &[String]) -> c_int {
        let target = match args.first().map(String::as_str) {
            None | Some("") => match dirs::home_dir() {
                Some(home_dir) => home_dir.display().to_string(),
                None => return 0,
            },
            Some("-") => std::env::var("OLDPWD").unwrap_or_else(|_| ".".into()),
            Some(other) => other.to_string(),
        };
        let old_dir = std::env::current_dir().ok();
        match std::env::set_current_dir(&target) {
            Ok(()) => {
                if args.first().map(String::as_str) == Some("-") {
                    if let Ok(now_dir) = std::env::current_dir() {
                        println!("{}", now_dir.display());
                    }
                }
                if let Some(old) = old_dir {
                    std::env::set_var("OLDPWD", old);
                }
                0
            }
            Err(error) => {
                let _ = writeln!(io::stderr(), "进入: {target}: {error}");
                1
            }
        }
    }

    
    pub(crate) fn run_push(args: &[String], ctx: &mut Context) -> c_int {
        let old_dir = match std::env::current_dir() {
            Ok(path) => path,
            Err(error) => {
                let _ = writeln!(io::stderr(), "压栈: 无法取得当前目录: {error}");
                return 1;
            }
        };
        let target = match args.first().map(String::as_str) {
            None | Some("") => match ctx.dir_stack.pop() {
                Some(top) => top,
                None => {
                    let _ = writeln!(io::stderr(), "压栈: 目录栈是空的");
                    return 1;
                }
            },
            Some(dir) => std::path::PathBuf::from(dir),
        };
        if let Err(error) = std::env::set_current_dir(&target) {
            
            if args.first().is_none_or(|dir| dir.is_empty()) {
                ctx.dir_stack.push(target.clone());
            }
            let _ = writeln!(io::stderr(), "压栈: {}: {error}", target.display());
            return 1;
        }
        ctx.dir_stack.push(old_dir);
        if let Ok(now_dir) = std::env::current_dir() {
            std::env::set_var("PWD", &now_dir);
            std::env::set_var("OLDPWD", &target);
            print_stack(&[now_dir], ctx);
        }
        0
    }

    
    pub(crate) fn run_pop(ctx: &mut Context) -> c_int {
        let Some(target) = ctx.dir_stack.pop() else {
            let _ = writeln!(io::stderr(), "弹栈: 目录栈是空的");
            return 1;
        };
        if let Err(error) = std::env::set_current_dir(&target) {
            ctx.dir_stack.push(target.clone());
            let _ = writeln!(io::stderr(), "弹栈: {}: {error}", target.display());
            return 1;
        }
        if let Ok(now_dir) = std::env::current_dir() {
            std::env::set_var("PWD", &now_dir);
            std::env::set_var("OLDPWD", &target);
            print_stack(&[now_dir], ctx);
        }
        0
    }

    
    
    
    
    
    pub(crate) fn run_read_file(args: &[String], ctx: &mut Context) -> c_int {
        
        let dot_escape = args.first().is_some_and(|item| item.starts_with('.') && item != "." && item != "..");
        let (file, kind) = if dot_escape {
            let file = args[0].clone();
            let kind = infer_type(&file);
            (Some(file), Some(kind))
        } else {
            match parse_read_args(args, "读取文本") {
                Ok(result) => result,
                Err(status) => return status,
            }
        };
        
        let read_stdin = ctx.stdin_is_pipe;
        let orig_text = match file.as_deref() {
            Some(path) => match std::fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) => {
                    let _ = writeln!(io::stderr(), "读取文本: {path}: {error}");
                    return 1;
                }
            },
            None if read_stdin => {
                let mut text = String::new();
                if let Err(error) = io::Read::read_to_string(&mut io::stdin().lock(), &mut text) {
                    let _ = writeln!(io::stderr(), "读取文本: 标准输入: {error}");
                    return 1;
                }
                text
            }
            None => {
                let _ = writeln!(io::stderr(), "读取文本: 缺少文件路径（标准输入不是管道）");
                return 1;
            }
        };
        match kind {
            None => {
                print_raw(&[orig_text]);
                0
            }
            Some(type_name) => match Value::parse_typed(&orig_text, &type_name) {
                Ok(parsed) => {
                    *ctx.ret_object = parsed.to_text();
                    echo(&[parsed.to_text()]);
                    0
                }
                Err(message_s) => {
                    let _ = writeln!(io::stderr(), "读取文本: （{type_name}）: {message_s}");
                    1
                }
            },
        }
    }

    
    
    
    
    
    pub(crate) fn run_write_file(args: &[String], _ctx: &mut Context) -> c_int {
        if args.is_empty() {
            let _ = writeln!(io::stderr(), "覆写文件: 缺少文件路径");
            return 1;
        }
        let file = args[0].as_str();
        let (explicit_type, content) = split_type_arg(&args[1..], "覆写文件");
        let kind = explicit_type.unwrap_or_else(|| infer_write_type(file));
        let text = content.join(" ");
        let rendered = Value::write_typed(&text, &kind);
        match rendered.and_then(|content| {
            std::fs::write(file, content).map_err(|error| error.to_string())
        }) {
            Ok(()) => 0,
            Err(message_s) => {
                let _ = writeln!(io::stderr(), "覆写文件: {file}（{kind}）: {message_s}");
                1
            }
        }
    }

    
    
    
    
    pub(crate) fn run_export(args: &[String], ctx: &mut Context) -> c_int {
        if args.is_empty() {
            list_vars(ctx);
            return 0;
        }
        let mut status = 0;
        for item in args {
            let Some(eq_pos_field) = item.find('=') else {
                let _ = writeln!(io::stderr(), "导出: {item}: 缺少 =数据");
                status = 1;
                continue;
            };
            let name = &item[..eq_pos_field];
            let data = &item[eq_pos_field + 1..];
            if !is_valid_var(name) {
                let _ = writeln!(io::stderr(), "导出: {name}: 不是合法的变量名");
                status = 1;
                continue;
            }
            
            if data.is_empty() {
                ctx.delete_var(name);
            } else {
                ctx.set_var(name, data);
            }
        }
        status
    }
}

#[allow(clippy::type_complexity)]
pub(crate) fn parse_read_args(args: &[String], cmd_name: &str) -> Result<(Option<String>, Option<String>), c_int> {
    
    
    let has_type = args.iter().any(|item| item.starts_with("--"));
    let file = args
        .first()
        .filter(|item| {
            if !item.starts_with("--") {
                return true;
            }
            
            has_type && !is_known_type(item)
        })
        .cloned();
    
    let (explicit_type, extra) = split_type_arg(&args[file.as_ref().map_or(0, |_| 1)..], cmd_name);
    if !extra.is_empty() {
        let _ = writeln!(io::stderr(), "{cmd_name}: 多余的内容：{}", extra.join(" "));
        return Err(1);
    }
    let kind = match explicit_type {
        Some(kind) => Some(kind),
        None => file.as_deref().map(infer_type),
    };
    Ok((file, kind))
}

pub(crate) fn is_known_type(item: &str) -> bool {
    let kind = item.trim_start_matches('-').to_ascii_uppercase();
    matches!(kind.as_str(), "JSON" | "INI" | "TEXT" | "文本")
}

pub(crate) fn split_type_arg(args: &[String], cmd_name: &str) -> (Option<String>, Vec<String>) {
    let mut explicit_type: Option<String> = None;
    let mut rest_args = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let item = args[index].as_str();
        index += 1;
        if let Some(kind) = item.strip_prefix("--类型=") {
            explicit_type = Some(kind.to_string());
        } else if item == "--类型" {
            match args.get(index) {
                Some(kind) => {
                    explicit_type = Some(kind.clone());
                    index += 1;
                }
                None => {
                    let _ = writeln!(io::stderr(), "{cmd_name}: --类型 后面缺少类型名");
                }
            }
        } else if let Some(kind) = item.strip_prefix("--") {
            explicit_type = Some(kind.to_string());
        } else {
            rest_args.push(args[index - 1].clone());
        }
    }
    (explicit_type, rest_args)
}

pub(crate) fn infer_type(file: &str) -> String {
    match std::path::Path::new(file).extension().and_then(|ext| ext.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("ini") || ext.eq_ignore_ascii_case("conf") => {
            "INI".to_string()
        }
        _ => "JSON".to_string(),
    }
}

pub(crate) fn infer_write_type(file: &str) -> String {
    match std::path::Path::new(file).extension().and_then(|ext| ext.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("json") => "JSON".to_string(),
        Some(ext) if ext.eq_ignore_ascii_case("ini") || ext.eq_ignore_ascii_case("conf") => {
            "INI".to_string()
        }
        _ => "文本".to_string(),
    }
}

pub(crate) fn is_valid_var(name: &str) -> bool {
    let mut chars_iter = name.chars();
    chars_iter
        .next()
        .is_some_and(|first_char| first_char.is_alphabetic() || first_char == '_')
        && chars_iter.all(|ch| ch.is_alphanumeric() || ch == '_')
}

pub(crate) fn print_stack(current: &[std::path::PathBuf], ctx: &Context) {
    let mut out = String::new();
    for path in current {
        out.push_str(&format!(" {}", short_path(path)));
    }
    for path in ctx.dir_stack.iter().rev() {
        out.push_str(&format!(" {}", short_path(path)));
    }
    println!("{out}");
}

pub(crate) fn short_path(path: &std::path::Path) -> String {
    let shown = path.display().to_string();
    match dirs::home_dir() {
        Some(home_dir) => {
            let home = home_dir.display().to_string();
            if shown == home {
                "~".to_string()
            } else if shown.starts_with(&format!("{home}/")) {
                format!("~{}", &shown[home.len()..])
            } else {
                shown
            }
        }
        None => shown,
    }
}

pub(crate) fn list_vars(ctx: &mut Context) {
    let mut entries: Vec<(&String, &String)> = ctx.env.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = io::stdout().lock();
    for (key, val) in entries {
        let _ = writeln!(out, "{key}={val}");
    }
}

pub(crate) fn exec_replace(args: &[String]) -> c_int {
    use std::ffi::CString;

    if args.is_empty() {
        return 0;
    }
    
    let argv: Vec<CString> = args
        .iter()
        .map(|text| CString::new(text.as_str()).unwrap_or_default())
        .collect();
    let pointers: Vec<*const c_char> = argv
        .iter()
        .map(|args| args.as_ptr())
        .chain(std::iter::once(std::ptr::null()))
        .collect();

    sys_execvp(pointers[0], pointers.as_ptr());
    
    let source = io::Error::last_os_error();
    let message = match source.kind() {
        io::ErrorKind::NotFound => format!("运行: {}: 未找到命令\n", args[0]),
        io::ErrorKind::PermissionDenied => format!("运行: {}: 权限不足\n", args[0]),
        _ => format!("运行: {}: {source}\n", args[0]),
    };
    let _ = io::stderr().write_all(message.as_bytes());
    if source.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    }
}

pub(crate) fn print_raw(args: &[String]) {
    let text = args.join(" ");
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

pub(crate) fn echo(args: &[String]) {
    let mut newline = true;
    let mut escapes = false;
    let mut start_pos = 0;
    while let Some(options) = args.get(start_pos) {
        match options.as_str() {
            "-n" => newline = false,
            "-e" => escapes = true,
            "-ne" | "-en" => {
                newline = false;
                escapes = true;
            }
            _ => break,
        }
        start_pos += 1;
    }
    let mut text = args[start_pos..].join(" ");
    if escapes {
        text = interpret_escapes(&text);
    }
    if newline {
        text.push('\n');
    }
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

pub(crate) fn interpret_escapes(orig_text: &str) -> String {
    let mut result = String::with_capacity(orig_text.len());
    let mut chars_iter = orig_text.chars();
    while let Some(ch) = chars_iter.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        match chars_iter.next() {
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some('r') => result.push('\r'),
            Some('\\') => result.push('\\'),
            Some('e') => result.push('\x1b'),
            Some(other) => {
                result.push('\\');
                result.push(other);
            }
            None => result.push('\\'),
        }
    }
    result
}

#[cfg(test)]
mod builtin_tests {
    use super::*;
    use crate::shell::Shell;

    
    fn new_context<'a>(
        env: &'a mut HashMap<String, String>,
        dir_stack: &'a mut Vec<std::path::PathBuf>,
    ) -> Context<'a> {
        Context {
            env,
            last_status: 0,
            quit: false,
            dir_stack,
            ret_object: Box::leak(Box::new(String::new())),
            stdin_is_pipe: false,
        }
    }

    #[test]
    fn read_and_write_file() {
        let root = std::env::temp_dir().join("壳读写文件测试");
        let _ = std::fs::create_dir_all(&root);
        let src = root.join("源.json");
        let out_path = root.join("出.ini");
        std::fs::write(&src, r#"{"名称":"壳","端口":8080}"#).unwrap();
        let mut env: HashMap<String, String> = HashMap::new();
        let mut dir_stack: Vec<std::path::PathBuf> = Vec::new();

        {
            let args = vec![src.display().to_string()];
            let mut ctx = new_context(&mut env, &mut dir_stack);
            assert_eq!(Builtin::ReadFile.run(&args, &mut ctx), 0);
            assert_eq!(*ctx.ret_object, "{名称=壳 端口=8080}");
        }
        {
            let args = vec![out_path.display().to_string(), "{名称=壳 端口=8080}".into()];
            let mut ctx = new_context(&mut env, &mut dir_stack);
            assert_eq!(Builtin::WriteFile.run(&args, &mut ctx), 0);
            let written = std::fs::read_to_string(&out_path).unwrap();
            assert!(written.contains("[全局]"));
            assert!(written.contains("名称=壳"));
        }
        {
            let orig_text = std::fs::read_to_string(&src).unwrap();
            let args = vec![src.display().to_string(), "--类型".into(), "文本".into()];
            let mut ctx = new_context(&mut env, &mut dir_stack);
            assert_eq!(Builtin::ReadFile.run(&args, &mut ctx), 0);
            assert_eq!(*ctx.ret_object, orig_text);
        }
        {
            let args = vec![src.display().to_string(), "--类型".into(), "不认识".into()];
            let mut ctx = new_context(&mut env, &mut dir_stack);
            assert_eq!(Builtin::ReadFile.run(&args, &mut ctx), 1);
            assert!(ctx.ret_object.is_empty());
        }
        {
            
            let bad_path = root.join("坏.ini");
            let args = vec![bad_path.display().to_string(), "--类型".into(), "XML".into()];
            let mut ctx = new_context(&mut env, &mut dir_stack);
            assert_eq!(Builtin::WriteFile.run(&args, &mut ctx), 1);
            assert!(!bad_path.exists());
        }
        {
            
            let mut ctx = new_context(&mut env, &mut dir_stack);
            assert_eq!(Builtin::ReadFile.run(&[], &mut ctx), 1);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn env_list_and_set() {
        let mut env: HashMap<String, String> =
            [("乙".to_string(), "2".to_string()), ("甲".to_string(), "1".to_string())]
                .into_iter()
                .collect();
        let commands = Builtin::Export;
        
        assert_eq!(
            commands.run(&[], &mut new_context(&mut env, &mut Vec::new())),
            0
        );
        
        let args = vec!["甲=覆盖".to_string(), "丙=新".to_string()];
        assert_eq!(
            commands.run(&args, &mut new_context(&mut env, &mut Vec::new())),
            0
        );
        assert_eq!(env.get("甲").map(String::as_str), Some("覆盖"));
        assert_eq!(env.get("丙").map(String::as_str), Some("新"));
        
        let args = vec!["带空格的=你好 世界".to_string()];
        assert_eq!(
            commands.run(&args, &mut new_context(&mut env, &mut Vec::new())),
            0
        );
        assert_eq!(env.get("带空格的").map(String::as_str), Some("你好 世界"));
        
        assert_eq!(
            commands.run(
                &["甲=".to_string()],
                &mut new_context(&mut env, &mut Vec::new())
            ),
            0
        );
        assert!(!env.contains_key("甲"));
        
        assert_eq!(
            commands.run(
                &["没有等号".to_string()],
                &mut new_context(&mut env, &mut Vec::new())
            ),
            1
        );
        assert_eq!(
            commands.run(
                &["1坏名=x".to_string()],
                &mut new_context(&mut env, &mut Vec::new())
            ),
            1
        );
    }

    #[test]
    fn chinese_var_to_child() {
        
        let commands = Builtin::Export;
        let args = vec!["跨进程名字=壳语言".to_string()];
        let mut env: HashMap<String, String> = HashMap::new();
        assert_eq!(
            commands.run(&args, &mut new_context(&mut env, &mut Vec::new())),
            0
        );
        let child = std::process::Command::new("sh")
            .arg("-c")
            
            .arg("env | grep ^跨进程名字=")
            .output()
            .expect("无法启动 sh");
        assert_eq!(
            String::from_utf8_lossy(&child.stdout),
            "跨进程名字=壳语言\n"
        );
        std::env::remove_var("跨进程名字");
    }

    #[test]
    fn dir_stack_push_pop() {
        let orig = std::env::current_dir().unwrap();
        let tmp_path = std::env::temp_dir();
        let mut env: HashMap<String, String> = HashMap::new();
        let mut dir_stack: Vec<std::path::PathBuf> = Vec::new();
        let args = vec![tmp_path.display().to_string()];

        assert_eq!(
            Builtin::PushDir.run(&args, &mut new_context(&mut env, &mut dir_stack)),
            0
        );
        assert_eq!(std::env::current_dir().unwrap(), tmp_path);
        assert_eq!(dir_stack, vec![orig.clone()]);

        
        assert_eq!(
            Builtin::PushDir.run(&[], &mut new_context(&mut env, &mut dir_stack)),
            0
        );
        assert_eq!(std::env::current_dir().unwrap(), orig);
        assert_eq!(dir_stack, vec![tmp_path.clone()]);

        assert_eq!(
            Builtin::PopDir.run(&[], &mut new_context(&mut env, &mut dir_stack)),
            0
        );
        assert_eq!(std::env::current_dir().unwrap(), tmp_path);
        assert!(dir_stack.is_empty());

        
        assert_eq!(
            Builtin::PopDir.run(&[], &mut new_context(&mut env, &mut dir_stack)),
            1
        );
        assert_eq!(
            Builtin::PushDir.run(
                &["/不存在的目录".into()],
                &mut new_context(&mut env, &mut dir_stack)
            ),
            1
        );
        assert!(dir_stack.is_empty());
        assert_eq!(std::env::current_dir().unwrap(), tmp_path);

        std::env::set_current_dir(orig).unwrap();
    }

    #[test]
    fn import_each_line() {
        let root = std::env::temp_dir().join("壳导入测试");
        let _ = std::fs::create_dir_all(&root);
        let first = root.join("一.壳");
        let second = root.join("二.壳");
        std::fs::write(&first, "导入甲=一\n").unwrap();
        std::fs::write(&second, "导入乙=二\n").unwrap();
        let mut shell = Shell::new();
        assert_eq!(
            shell.import_files(&[
                first.display().to_string(),
                second.display().to_string()
            ]),
            0
        );
        assert_eq!(shell.variables.get("导入甲").map(String::as_str), Some("一"));
        assert_eq!(shell.variables.get("导入乙").map(String::as_str), Some("二"));
        assert_eq!(shell.import_files(&[]), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn var_path() {
        let mut shell = Shell::new();
        shell.run_text(r#"对象="{甲=1 乙={丙=深} 列表=(x y)}""#);
        shell.run_text("甲=${对象.甲}; 丙=${对象.乙.丙}; 列=${对象.列表.1}; 无=[${对象.没有}]");
        shell.run_text(r#"节="{桌面={甲=一 乙=二}}""#);
        shell.run_text("直=${节.甲}; 链=${节.桌面.乙}");
        assert_eq!(shell.variables.get("甲").map(String::as_str), Some("1"));
        assert_eq!(shell.variables.get("丙").map(String::as_str), Some("深"));
        assert_eq!(shell.variables.get("列").map(String::as_str), Some("y"));
        assert_eq!(shell.variables.get("无").map(String::as_str), Some("[]"));
        assert_eq!(shell.variables.get("直").map(String::as_str), Some("一"));
        assert_eq!(shell.variables.get("链").map(String::as_str), Some("二"));
    }

    #[test]
    fn read_file_alias() {
        assert_eq!(find_by_name("读取文件"), find_by_name("读取文本"));
        assert!(find_by_name("读取文件").is_some());
    }

    #[test]
    fn valid_var_name() {
        assert!(is_valid_var("名字"));
        assert!(is_valid_var("_x1"));
        assert!(!is_valid_var(""));
        assert!(!is_valid_var("1名"));
        assert!(!is_valid_var("名 字"));
    }
}
