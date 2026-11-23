use libc::c_int;
use std::io::{self, Write};
use std::collections::HashMap;
use crate::value::*;
use crate::lexer::*;
use crate::plan::*;
use crate::exec::*;
use crate::builtin::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Flow {
    Normal(c_int),
    Break(c_int),
    Continue(c_int),
    Return(c_int),
}

impl Flow {
    pub(crate) fn code(self) -> c_int {
        match self {
            Flow::Normal(code) | Flow::Break(code) | Flow::Continue(code) | Flow::Return(code) => code,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Shell {
    pub(crate) variables: HashMap<String, String>,
    
    pub(crate) dir_stack: Vec<std::path::PathBuf>,
    pub(crate) last_status: c_int,
    pub(crate) quit: bool,
    
    pub(crate) exit_code: c_int,
    
    
    pub(crate) functions: HashMap<String, (Vec<String>, Vec<Stmt>)>,
    
    pub(crate) return_value: Value,
    
    pub(crate) ret_object: String,
}

impl Shell {
    pub(crate) fn new() -> Self {
        let mut variables: HashMap<String, String> = std::env::vars().collect();
        variables.entry("PWD".into()).or_insert_with(|| {
            std::env::current_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_default()
        });
        Self {
            variables,
            dir_stack: Vec::new(),
            last_status: 0,
            quit: false,
            exit_code: 0,
            functions: HashMap::new(),
            return_value: Value::Int(0),
            ret_object: String::new(),
        }
    }

    
    
    
    
    pub(crate) fn load_rc(&mut self) {
        let Some(home_dir) = dirs::home_dir() else {
            return;
        };
        let path = home_dir.join(".壳语言rc");
        if !path.exists() {
            return;
        }
        self.run_script_file(&path.display().to_string());
        
        self.last_status = 0;
    }

    
    
    
    
    pub(crate) fn run_script_file(&mut self, file_name: &str) -> c_int {
        let content = match std::fs::read_to_string(file_name) {
            Ok(content) => content,
            Err(error) => {
                use std::io::Write;
                let _ = writeln!(std::io::stderr(), "加载脚本: {file_name}: {error}");
                return 1;
            }
        };
        let stmt_list = match parse_script(&content) {
            Ok(entries) => entries,
            Err(error) => {
                eprintln!("语法错误：{error}");
                return 2;
            }
        };

        let saved_path = self.variables.get("脚本路径").cloned();
        let saved_dir = self.variables.get("脚本目录").cloned();
        let path = std::path::Path::new(file_name);
        let abs_path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let dir = abs_path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.variables.insert("脚本路径".into(), file_name.to_string());
        self.variables.insert("脚本目录".into(), dir);

        let result = self.exec_stmts(&stmt_list);

        restore_var(&mut self.variables, "脚本路径", saved_path);
        restore_var(&mut self.variables, "脚本目录", saved_dir);

        result.code()
    }

    
    
    pub(crate) fn set_positional(&mut self, script_path: &str, args: &[String]) {
        self.variables.insert("0".into(), script_path.into());
        for (idx, value) in args.iter().enumerate() {
            self.variables.insert((idx + 1).to_string(), value.clone());
        }
    }

    
    
    
    
    
    
    pub(crate) fn run_jobs(&mut self, jobs: &[Job]) -> c_int {
        for job in jobs {
            if self.quit {
                break;
            }
            
            
            let kernel = std::cell::RefCell::new(None);
            let plan_result = {
                let planner = Planner {
                    env_table: &self.variables,
                    last_status: self.last_status,
                    cmdsub: &|commands, lookup| self.run_cmdsub(&kernel, commands, lookup),
                };
                planner.plan(job)
            };
            if let Some(shell) = kernel.into_inner() {
                self.adopt(shell);
            }
            match plan_result {
                Err(message_s) => {
                    eprintln!("壳语言：{message_s}");
                    self.last_status = 2;
                }
                Ok(Plan::Empty) => {}
                Ok(Plan::Assign(assigns)) => {
                    for (name, val) in assigns {
                        
                        
                        std::env::set_var(&name, &val);
                        self.variables.insert(name, val);
                    }
                    self.last_status = 0;
                }
                Ok(Plan::Pipeline(spec)) => {
                    self.last_status = self.run_pipeline(spec);
                }
            }
        }
        self.last_status
    }

    
    pub(crate) fn kernel_shell(&self) -> Shell {
        let mut shell = self.clone();
        shell.quit = false;
        shell.exit_code = 0;
        shell
    }

    
    pub(crate) fn run_cmdsub(
        &self,
        slot: &std::cell::RefCell<Option<Shell>>,
        commands: &str,
        lookup: &dyn Fn(&str) -> Option<String>,
    ) -> String {
        let text = expand_cmd_text(commands, lookup);
        slot.borrow_mut().get_or_insert_with(|| self.kernel_shell()).capture_output(&text)
    }

    pub(crate) fn adopt(&mut self, kernel: Shell) {
        self.variables = kernel.variables;
    }

    
    pub(crate) fn exec_stmts(&mut self, stmts: &[Stmt]) -> Flow {
        let mut status = 0;
        for stmt in stmts {
            if self.quit {
                return Flow::Normal(self.exit_code);
            }
            match self.exec_stmt(stmt) {
                Flow::Normal(code) => status = code,
                other => return other,
            }
        }
        Flow::Normal(status)
    }

    pub(crate) fn exec_stmt(&mut self, stmt: &Stmt) -> Flow {
        match stmt {
            Stmt::Jobs(jobs) => Flow::Normal(self.run_jobs(jobs)),
            Stmt::If { branches, else_body } => {
                for (cond, body) in branches {
                    if self.run_jobs(std::slice::from_ref(cond)) == 0 {
                        return self.exec_stmts(body);
                    }
                }
                if let Some(body) = else_body {
                    return self.exec_stmts(body);
                }
                Flow::Normal(0)
            }
            Stmt::For { variables, values, body } => {
                let mut status = 0;
                for ch in values {
                    let values = {
                        let planner = Planner {
                            env_table: &self.variables,
                            last_status: self.last_status,
                            cmdsub: &empty_cmdsub,
                        };
                        planner.expand(ch)
                    };
                    for value in values {
                        
                        self.variables.insert(variables.clone(), value);
                        match self.exec_stmts(body) {
                            Flow::Normal(code) => status = code,
                            Flow::Return(code) => return Flow::Return(code),
                            Flow::Break(code) => return Flow::Normal(code),
                            Flow::Continue(code) => {
                                status = code;
                                continue;
                            }
                        }
                        if self.quit {
                            return Flow::Normal(self.exit_code);
                        }
                    }
                }
                Flow::Normal(status)
            }
            Stmt::While { cond, body } => {
                let mut status = 0;
                loop {
                    if self.quit {
                        return Flow::Normal(self.exit_code);
                    }
                    if self.run_jobs(std::slice::from_ref(cond)) != 0 {
                        break;
                    }
                    match self.exec_stmts(body) {
                        Flow::Normal(code) => status = code,
                        Flow::Return(code) => return Flow::Return(code),
                        Flow::Break(code) => {
                            status = code;
                            break;
                        }
                        Flow::Continue(code) => status = code,
                    }
                }
                Flow::Normal(status)
            }
            Stmt::Break => Flow::Break(self.last_status),
            Stmt::Continue => Flow::Continue(self.last_status),
            Stmt::Function { name, args, body } => {
                self.functions.insert(name.clone(), (args.clone(), body.clone()));
                Flow::Normal(0)
            }
            Stmt::Return(expr) => {
                let return_value = match expr {
                    Some(expr) => eval(expr, &|var_name| {
                        self.variables
                            .get(var_name)
                            .cloned()
                            .or_else(|| std::env::var(var_name).ok())
                    }),
                    None => Value::Int(self.last_status as i64),
                };
                let exit_code = return_value.to_exit_code() as c_int;
                self.return_value = return_value.clone();
                self.variables.insert("返回值".into(), return_value.to_text());
                self.variables.insert("返回值JSON".into(), return_value.json());
                
                if let Some(object) = Value::read_json(&return_value.json()) {
                    if let Value::Object(_) = object {
                        self.variables.insert("返回值对象".into(), object.to_text());
                    }
                }
                self.last_status = exit_code;
                Flow::Return(exit_code)
            }
        }
    }

    
    
    pub(crate) fn run_text(&mut self, text: &str) -> c_int {
        let stmt_list = match parse_script(text) {
            Ok(entries) => entries,
            Err(error) => {
                eprintln!("语法错误：{error}");
                return 2;
            }
        };
        self.exec_stmts(&stmt_list).code()
    }

    pub(crate) fn run_pipeline(&mut self, spec: PipelineSpec) -> c_int {
        
        if spec.commands.len() == 1 && !spec.background {
            let program_name = spec.commands[0].program_name();
            let define = self.functions.get(&program_name).cloned();
            if let Some((param_names, body)) = define {
                let actual = spec.commands[0].arg_list[1..].to_vec();
                return self.call_function(&program_name, &param_names, &actual, &body, &spec.redirect);
            }
        }

        
        if let Some(builtin) = spec.builtin() {
            let _guard = if spec.redirect.is_set() || spec.redirect.merge_err {
                match RedirectGuard::apply(&spec.redirect) {
                    Ok(guard) => Some(guard),
                    Err(error) => {
                        eprintln!("壳语言：{error}");
                        return 1;
                    }
                }
            } else {
                None
            };
            let args = &spec.commands[0].arg_list[1..];
            if matches!(builtin, Builtin::Import) {
                return self.import_files(args);
            }
            let status = {
                let mut ctx = Context {
                    env: &mut self.variables,
                    last_status: self.last_status,
                    quit: false,
                    dir_stack: &mut self.dir_stack,
                    ret_object: &mut self.ret_object,
                    stdin_is_pipe: matches!(builtin, Builtin::ReadFile)
                        && (spec.commands.len() > 1
                            || spec.input_from_file
                            || stdin_readable()),
                };
                let status = builtin.run(args, &mut ctx);
                if ctx.quit {
                    self.quit = true;
                    self.exit_code = status;
                }
                status
            };
            if matches!(builtin, Builtin::ReadFile) {
                self.sync_return_vars();
            }
            return status;
        }

        
        let single = &spec.commands[0];

        let result = if spec.commands.len() == 1
            && !spec.redirect.is_set()
            && !spec.redirect.merge_err
            && !spec.background
            && single.env.is_empty()
        {
            run_single(single)
        } else {
            run_pipeline_ext(&spec)
        };
        match result {
            Ok(status) => status,
            Err(error) => {
                eprintln!("壳语言：{error}");
                error.exit_code()
            }
        }
    }

    
    pub(crate) fn call_function(
        &mut self,
        name: &str,
        param_names: &[String],
        actual: &[String],
        body: &[Stmt],
        redirect: &Redirect,
    ) -> c_int {
        let _guard = if redirect.is_set() || redirect.merge_err {
            match RedirectGuard::apply(redirect) {
                Ok(guard) => Some(guard),
                Err(error) => {
                    eprintln!("壳语言：{error}");
                    return 1;
                }
            }
        } else {
            None
        };

        let saved_positional = self.snapshot_positional();
        let saved_named: Vec<(String, Option<String>)> = param_names
            .iter()
            .map(|entry_name| (entry_name.clone(), self.variables.get(entry_name).cloned()))
            .collect();

        self.variables.insert("0".into(), name.to_string());
        for (idx, entry_name) in param_names.iter().enumerate() {
            let val = actual.get(idx).cloned().unwrap_or_default();
            self.variables.insert(entry_name.clone(), val);
        }
        for (idx, value) in actual.iter().enumerate() {
            self.variables.insert((idx + 1).to_string(), value.clone());
        }

        let code = self.exec_stmts(body).code();

        self.restore_positional(saved_positional);
        for (entry_name, raw) in saved_named {
            if let Some(val) = raw {
                self.variables.insert(entry_name, val);
            } else {
                self.variables.remove(&entry_name);
            }
        }
        code
    }

    pub(crate) fn snapshot_positional(&self) -> Vec<(String, Option<String>)> {
        self.variables
            .keys()
            .filter(|key| !key.is_empty() && key.chars().all(|ch| ch.is_ascii_digit()))
            .map(|key| (key.clone(), self.variables.get(key).cloned()))
            .collect()
    }

    pub(crate) fn restore_positional(&mut self, snapshot: Vec<(String, Option<String>)>) {
        let digit_keys: Vec<String> = self
            .variables
            .keys()
            .filter(|key| !key.is_empty() && key.chars().all(|ch| ch.is_ascii_digit()))
            .cloned()
            .collect();
        for key in digit_keys {
            self.variables.remove(&key);
        }
        for (key, val) in snapshot {
            if let Some(val) = val {
                self.variables.insert(key, val);
            }
        }
    }

    pub(crate) fn import_files(&mut self, args: &[String]) -> c_int {
        if args.is_empty() {
            let _ = writeln!(io::stderr(), "加载脚本: 缺少脚本文件名");
            return 1;
        }
        let mut status = 0;
        for file in args {
            status = self.run_script_file(file);
            if self.quit {
                break;
            }
        }
        status
    }

    
    pub(crate) fn sync_return_vars(&mut self) {
        if self.ret_object.is_empty() {
            self.variables.remove("返回值对象");
            self.variables.remove("返回值JSON");
            return;
        }
        self.variables.insert("返回值对象".into(), self.ret_object.clone());
        match Value::read_json(&self.ret_object) {
            Some(parse_result) => {
                self.variables.insert("返回值JSON".into(), parse_result.to_json());
            }
            None => {
                self.variables.remove("返回值JSON");
            }
        }
    }

}

impl Shell {
    pub(crate) fn capture_output(&mut self, commands: &str) -> String {
        let stmt_list = match parse_script(commands) {
            Ok(entries) => entries,
            Err(error) => {
                eprintln!("壳语言：{error}");
                return String::new();
            }
        };
        let capture = match OutputCapture::start() {
            Ok(capture) => capture,
            Err(error) => {
                eprintln!("壳语言：{error}");
                return String::new();
            }
        };
        let result = self.exec_stmts(&stmt_list);
        let out = capture.take();
        self.last_status = result.code();
        let orig_text = String::from_utf8_lossy(&out).trim_end_matches('\n').to_string();
        
        
        match Value::from_value_text(&orig_text) {
            Some(restored) => restored.to_text(),
            None => orig_text,
        }
    }
}

pub(crate) fn restore_var(variables: &mut HashMap<String, String>, var_name: &str, raw: Option<String>) {
    match raw {
        Some(val) => {
            variables.insert(var_name.to_string(), val);
        }
        None => {
            variables.remove(var_name);
        }
    }
    std::env::remove_var(var_name);
}

pub(crate) fn expand_cmd_text(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    let mut single_quote = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            single_quote = !single_quote;
            result.push(c);
            i += 1;
            continue;
        }
        if !single_quote && c == '$' {
            match chars.get(i + 1) {
                Some('{') => {
                    if let Some(offset) = chars[i + 2..].iter().position(|&x| x == '}') {
                        let var_name: String = chars[i + 2..i + 2 + offset].iter().collect();
                        match resolve_var(&var_name, lookup) {
                            Some(val) => result.push_str(&val),
                            None => {
                                for ch in &chars[i..i + 3 + offset] {
                                    result.push(*ch);
                                }
                            }
                        }
                        i = i + 3 + offset;
                        continue;
                    }
                }
                Some(&first) if first.is_alphanumeric() || first == '_' => {
                    let start_pos = i + 1;
                    let mut end = start_pos;
                    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                        end += 1;
                    }
                    let var_name: String = chars[start_pos..end].iter().collect();
                    match lookup(&var_name) {
                        Some(val) => result.push_str(&val),
                        None => {
                            for ch in &chars[i..end] {
                                result.push(*ch);
                            }
                        }
                    }
                    i = end;
                    continue;
                }
                _ => {}
            }
        }
        result.push(c);
        i += 1;
    }
    result
}

pub(crate) fn empty_cmdsub(_cmd: &str, _lookup: &dyn Fn(&str) -> Option<String>) -> String {
    String::new()
}
