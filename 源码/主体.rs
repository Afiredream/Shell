use libc::{c_char, c_int, c_uint, c_void};

fn sys_fork() -> c_int {
    unsafe { libc::fork() }
}

fn sys_execvp(file_name: *const c_char, argv: *const *const c_char) -> c_int {
    unsafe { libc::execvp(file_name, argv) }
}

fn sys_waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int {
    unsafe { libc::waitpid(pid, status, options) }
}

fn sys_pipe(fds: *mut c_int) -> c_int {
    unsafe { libc::pipe(fds) }
}

fn sys_dup(fd: c_int) -> c_int {
    unsafe { libc::dup(fd) }
}

fn sys_dup2(old_fd: c_int, new_fd: c_int) -> c_int {
    unsafe { libc::dup2(old_fd, new_fd) }
}

fn sys_close(fd: c_int) -> c_int {
    unsafe { libc::close(fd) }
}

fn sys_open(path: *const c_char, flags: c_int) -> c_int {
    unsafe { libc::open(path, flags) }
}

fn sys_open_mode(path: *const c_char, flags: c_int, mode: c_uint) -> c_int {
    unsafe { libc::open(path, flags, mode) }
}

fn sys_write(fd: c_int, buf: *const c_void, len: usize) -> isize {
    unsafe { libc::write(fd, buf, len) }
}

fn sys_exit(exit_code: c_int) -> ! {
    unsafe { libc::_exit(exit_code) }
}

fn sys_signal(sig: c_int, handler: usize) -> usize {
    unsafe { libc::signal(sig, handler) }
}

fn sys_setpgid(pid: c_int, pgid: c_int) -> c_int {
    unsafe { libc::setpgid(pid, pgid) }
}

fn sys_kill(pid: c_int, sig: c_int) -> c_int {
    unsafe { libc::kill(pid, sig) }
}

const SIGINT: c_int = libc::SIGINT;
const SIGTERM: c_int = libc::SIGTERM;
const SIGCHLD: c_int = libc::SIGCHLD;

const SIG_DFL: usize = libc::SIG_DFL;
const SIG_IGN: usize = libc::SIG_IGN;

const WNOHANG: c_int = libc::WNOHANG;
const WUNTRACED: c_int = libc::WUNTRACED;

const O_RDONLY: c_int = libc::O_RDONLY;
const O_WRONLY: c_int = libc::O_WRONLY;
const O_CREAT: c_int = libc::O_CREAT;
const O_APPEND: c_int = libc::O_APPEND;
const O_TRUNC: c_int = libc::O_TRUNC;
const EINVAL: c_int = libc::EINVAL;

fn sys_exited(raw_status: c_int) -> bool {
    raw_status & 0x7f == 0
}
fn sys_signaled(raw_status: c_int) -> bool {
    (((raw_status & 0x7f) + 1) as i8) >> 1 > 0
}
fn sys_exit_status(raw_status: c_int) -> c_int {
    (raw_status >> 8) & 0xff
}
fn sys_term_signal(raw_status: c_int) -> c_int {
    raw_status & 0x7f
}
fn sys_stopped(raw_status: c_int) -> bool {
    raw_status & 0xff == 0x7f
}
fn sys_stop_signal(raw_status: c_int) -> c_int {
    (raw_status >> 8) & 0xff
}

fn restore_signals() {
    sys_signal(SIGINT, SIG_DFL);
    sys_signal(SIGTERM, SIG_DFL);
    
    sys_signal(SIGCHLD, SIG_DFL);
}

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Int(i64),
    Text(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    
    fn to_text(&self) -> String {
        match self {
            Value::Int(n) => n.to_string(),
            Value::Text(text) => text.clone(),
            Value::Array(items) => {
                let content: Vec<String> = items.iter().map(|item| item.to_text()).collect();
                format!("({})", content.join(" "))
            }
            Value::Object(items) => {
                let content: Vec<String> = items
                    .iter()
                    .map(|(key, item)| format!("{key}={}", item.to_text()))
                    .collect();
                format!("{{{}}}", content.join(" "))
            }
        }
    }

    
    #[allow(non_snake_case)]
    fn json(&self) -> String {
        self.to_json()
    }

    
    
    fn from_value_text(text: &str) -> Option<Value> {
        let first = text.chars().next()?;
        if !matches!(first, '(' | '{' | '-' | '+' | '0'..='9') {
            return None;
        }
        let expr = parse_expr(text).ok()?;
        Some(eval(&expr, &|_| None))
    }

    
    fn to_exit_code(&self) -> i64 {
        match self {
            Value::Int(n) => *n,
            Value::Text(text) => text.parse().unwrap_or(0),
            _ => 0,
        }
    }
}

impl Value {
    
    
    fn read_json(text: &str) -> Option<Value> {
        let bytes = text.trim().as_bytes();
        let mut pos = 0;
        let result = read_json_value(bytes, &mut pos)?;
        skip_space(bytes, &mut pos);
        if pos == bytes.len() {
            Some(result)
        } else {
            None
        }
    }

    
    fn to_json(&self) -> String {
        let mut out = String::new();
        write_json_value(self, &mut out);
        out
    }
}

fn skip_space(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && matches!(bytes[*pos], b' ' | b'\t' | b'\r' | b'\n') {
        *pos += 1;
    }
}

fn read_json_value(bytes: &[u8], pos: &mut usize) -> Option<Value> {
    skip_space(bytes, pos);
    match *bytes.get(*pos)? {
        b'{' => {
            *pos += 1;
            let mut items = Vec::new();
            skip_space(bytes, pos);
            if bytes.get(*pos) == Some(&b'}') {
                *pos += 1;
                return Some(Value::Object(items));
            }
            loop {
                skip_space(bytes, pos);
                let key = read_json_string(bytes, pos)?;
                skip_space(bytes, pos);
                if bytes.get(*pos) != Some(&b':') {
                    return None;
                }
                *pos += 1;
                items.push((key, read_json_value(bytes, pos)?));
                skip_space(bytes, pos);
                match bytes.get(*pos)? {
                    b',' => *pos += 1,
                    b'}' => {
                        *pos += 1;
                        return Some(Value::Object(items));
                    }
                    _ => return None,
                }
            }
        }
        b'[' => {
            *pos += 1;
            let mut items = Vec::new();
            skip_space(bytes, pos);
            if bytes.get(*pos) == Some(&b']') {
                *pos += 1;
                return Some(Value::Array(items));
            }
            loop {
                items.push(read_json_value(bytes, pos)?);
                skip_space(bytes, pos);
                match bytes.get(*pos)? {
                    b',' => *pos += 1,
                    b']' => {
                        *pos += 1;
                        return Some(Value::Array(items));
                    }
                    _ => return None,
                }
            }
        }
        b'"' => Some(Value::Text(read_json_string(bytes, pos)?)),
        b't' if bytes[*pos..].starts_with(b"true") => {
            *pos += 4;
            Some(Value::Text("true".into()))
        }
        b'f' if bytes[*pos..].starts_with(b"false") => {
            *pos += 5;
            Some(Value::Text("false".into()))
        }
        b'n' if bytes[*pos..].starts_with(b"null") => {
            *pos += 4;
            Some(Value::Text(String::new()))
        }
        _ => {
            let start_pos = *pos;
            while *pos < bytes.len() && matches!(bytes[*pos], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                *pos += 1;
            }
            if start_pos == *pos {
                return None;
            }
            let digits = std::str::from_utf8(&bytes[start_pos..*pos]).ok()?;
            match digits.parse::<i64>() {
                Ok(c_int) => Some(Value::Int(c_int)),
                Err(_) => Some(Value::Text(digits.to_string())),
            }
        }
    }
}

fn read_json_string(bytes: &[u8], pos: &mut usize) -> Option<String> {
    if bytes.get(*pos) != Some(&b'"') {
        return None;
    }
    *pos += 1;
    let mut out = String::new();
    loop {
        let byte = *bytes.get(*pos)?;
        *pos += 1;
        match byte {
            b'"' => return Some(out),
            b'\\' => {
                let esc = *bytes.get(*pos)?;
                *pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{8}'),
                    b'f' => out.push('\u{c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let hex = std::str::from_utf8(bytes.get(*pos..*pos + 4)?).ok()?;
                        *pos += 4;
                        let code = u32::from_str_radix(hex, 16).ok()?;
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                    }
                    _ => return None,
                }
            }
            ctrl if ctrl < b' ' => return None,
            _ => {
                
                let start_pos = *pos - 1;
                let len = match byte {
                    0x00..=0x7f => 1,
                    0xc0..=0xdf => 2,
                    0xe0..=0xef => 3,
                    _ => 4,
                };
                let seg = bytes.get(start_pos..start_pos + len)?;
                *pos = start_pos + len;
                out.push_str(std::str::from_utf8(seg).ok()?);
            }
        }
    }
}

impl Value {
    
    fn parse_typed(text: &str, kind: &str) -> Result<Value, String> {
        match kind.to_ascii_uppercase().as_str() {
            "JSON" => Value::read_json(text).ok_or_else(|| "不是合法的 JSON".to_string()),
            "INI" => Ok(read_ini(text)),
            "文本" | "TEXT" => Ok(Value::Text(text.to_string())),
            other => Err(format!("不支持的类型 `{other}`（可用 JSON、INI、文本）")),
        }
    }
}

impl Value {
    
    
    fn write_typed(text: &str, kind: &str) -> Result<String, String> {
        let type_name = kind.to_ascii_uppercase();
        if type_name == "文本" || type_name == "TEXT" {
            return Ok(text.to_string());
        }
        
        let content = match parse_expr(text) {
            
            Ok(Expr::Value(Value::Text(raw_text))) => match Value::parse_typed(&raw_text, &type_name) {
                Ok(parsed) => parsed,
                Err(_) => Value::Text(raw_text),
            },
            Ok(expr) => eval(&expr, &|_| None),
            Err(_) => Value::parse_typed(text, &type_name)?,
        };
        match type_name.as_str() {
            "JSON" => Ok(indent_json(&content, 0)),
            "INI" => Ok(write_ini(&content)),
            other => Err(format!("不支持的类型 `{other}`（可用 JSON、INI、文本）")),
        }
    }
}

fn indent_json(item: &Value, indent: usize) -> String {
    let indent_prefix = "  ".repeat(indent);
    match item {
        Value::Object(items) if !items.is_empty() => {
            let children: Vec<String> = items
                .iter()
                .map(|(key, child_item)| {
                    let mut key_text = String::new();
                    write_json_string(key, &mut key_text);
                    format!("{}  {}: {}", indent_prefix, key_text, indent_json(child_item, indent + 1))
                })
                .collect();
            format!("{{\n{}\n{}}}", children.join(",\n"), indent_prefix)
        }
        Value::Array(items) if !items.is_empty() => {
            let all_simple = items.iter().all(|child_item| !matches!(child_item, Value::Object(_) | Value::Array(_)));
            if all_simple {
                item.to_json()
            } else {
                let children: Vec<String> =
                    items.iter().map(|child_item| format!("{}  {}", indent_prefix, indent_json(child_item, indent + 1))).collect();
                format!("[\n{}\n{}]", children.join(",\n"), indent_prefix)
            }
        }
        _ => item.to_json(),
    }
}

fn write_ini(item: &Value) -> String {
    fn value_text(child_item: &Value) -> String {
        match child_item {
            Value::Array(items) => items.iter().map(value_text).collect::<Vec<_>>().join(","),
            Value::Object(_) => child_item.to_json(),
            _ => {
                let text = child_item.to_text();
                if text.contains([' ', ',', '\'', '=', '"']) || text.is_empty() {
                    format!("\"{text}\"")
                } else {
                    text
                }
            }
        }
    }
    let mut out = String::new();
    let mut global_lines = Vec::new();
    let Value::Object(items) = item else {
        return value_text(item);
    };
    for (key, child_item) in items {
        match child_item {
            Value::Object(section) => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&format!("[{key}]\n"));
                for (seg_key, seg_value) in section {
                    out.push_str(&format!("{seg_key}={}\n", value_text(seg_value)));
                }
            }
            _ => global_lines.push(format!("{key}={}", value_text(child_item))),
        }
    }
    if !global_lines.is_empty() {
        let global_section = format!("[全局]\n{}\n", global_lines.join("\n"));
        out = global_section + &out;
    }
    out
}

fn read_ini(text: &str) -> Value {
    
    let mut section_name = String::from("全局");
    let mut sections: Vec<(String, Vec<(String, Value)>)> = vec![(section_name.clone(), Vec::new())];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(seg) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            section_name = seg.trim().to_string();
            if !sections.iter().any(|(entry_name, _)| *entry_name == section_name) {
                sections.push((section_name.clone(), Vec::new()));
            }
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_string();
        let val = ini_value(strip_trailing_comment(raw.trim()));
        let seg = sections
            .iter_mut()
            .find(|(entry_name, _)| *entry_name == section_name)
            .expect("段表中必有当前段");
        match seg.1.iter_mut().find(|(existing_key, _)| *existing_key == key) {
            Some((_, old_value)) => *old_value = val,
            None => seg.1.push((key, val)),
        }
    }
    
    Value::Object(
        sections
            .into_iter()
            .filter(|(_, key_values)| !key_values.is_empty())
            .map(|(entry_name, key_values)| (entry_name, Value::Object(key_values)))
            .collect(),
    )
}

fn strip_trailing_comment(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    let mut closing_byte: Option<u8> = None;
    for (index, &ch) in bytes.iter().enumerate() {
        match ch {
            b'"' | b'\'' => match closing_byte {
                None => closing_byte = Some(ch),
                Some(closing) if closing == ch => closing_byte = None,
                _ => {}
            },
            b'#' | b';' if closing_byte.is_none() && index > 0 && bytes[index - 1].is_ascii_whitespace() => {
                return raw[..index].trim_end()
            }
            _ => {}
        }
    }
    raw
}

fn ini_value(raw: &str) -> Value {
    
    if raw.len() >= 2
        && ((raw.starts_with('"') && raw.ends_with('"'))
            || (raw.starts_with('\'') && raw.ends_with('\'')))
    {
        return Value::Text(raw[1..raw.len() - 1].to_string());
    }
    let items: Vec<&str> = raw.split(',').map(str::trim).filter(|item| !item.is_empty()).collect();
    let parse_scalar = |text: &str| match text.parse::<i64>() {
        Ok(c_int) => Value::Int(c_int),
        Err(_) => match text.to_ascii_lowercase().as_str() {
            "true" => Value::Text("true".into()),
            "false" => Value::Text("false".into()),
            _ => Value::Text(text.to_string()),
        },
    };
    if items.len() > 1 {
        Value::Array(items.into_iter().map(parse_scalar).collect())
    } else {
        parse_scalar(raw)
    }
}

fn write_json_value(item: &Value, out: &mut String) {
    match item {
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Text(text) => write_json_string(text, out),
        Value::Array(items) => {
            out.push('[');
            for (index, child_item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_json_value(child_item, out);
            }
            out.push(']');
        }
        Value::Object(items) => {
            out.push('{');
            for (index, (key, child_item)) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_json_string(key, out);
                out.push(':');
                write_json_value(child_item, out);
            }
            out.push('}');
        }
    }
}

fn write_json_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ctrl if (ctrl as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ctrl as u32)),
            other => out.push(other),
        }
    }
    out.push('"');
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_text())
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Value(Value),
    Var(String),
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
enum ExprError {
    #[error("{0} 没有闭合")]
    UnclosedParen(char),
    #[error("多余的符号 `{0}`")]
    ExtraSymbol(char),
    #[error("对象项必须写成 `键=值`")]
    BadObjectItem,
    #[error("表达式为空")]
    EmptyExpr,
}

fn eval(expr: &Expr, lookup: &dyn Fn(&str) -> Option<String>) -> Value {
    match expr {
        Expr::Value(v) => v.clone(),
        Expr::Var(var_name) => Value::Text(lookup(var_name).unwrap_or_default()),
        Expr::Array(items) => Value::Array(items.iter().map(|item| eval(item, lookup)).collect()),
        Expr::Object(items) => Value::Object(
            items
                .iter()
                .map(|(key, item)| (key.clone(), eval(item, lookup)))
                .collect(),
        ),
    }
}

fn parse_expr(text: &str) -> Result<Expr, ExprError> {
    let chars: Vec<char> = text.chars().collect();
    let mut parser = ExprParser {
        chars: &chars,
        pos: 0,
    };
    parser.skip_space();
    let result = parser.parse_value()?;
    parser.skip_space();
    if let Some(symbol) = parser.current() {
        return Err(ExprError::ExtraSymbol(symbol));
    }
    Ok(result)
}

struct ExprParser<'a> {
    chars: &'a [char],
    pos: usize,
}

impl ExprParser<'_> {
    fn current(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_space(&mut self) {
        while self.current().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    fn parse_value(&mut self) -> Result<Expr, ExprError> {
        self.skip_space();
        match self.current() {
            None => Err(ExprError::EmptyExpr),
            Some('(') => self.parse_array(),
            Some('{') => self.parse_object(),
            Some('$') => self.parse_var(),
            Some('\'') | Some('"') => self.parse_quoted(),
            _ => self.parse_word(),
        }
    }

    fn parse_array(&mut self) -> Result<Expr, ExprError> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_space();
            match self.current() {
                None => return Err(ExprError::UnclosedParen('(')),
                Some(')') => {
                    self.pos += 1;
                    break;
                }
                _ => items.push(self.parse_value()?),
            }
        }
        Ok(Expr::Array(items))
    }

    fn parse_object(&mut self) -> Result<Expr, ExprError> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_space();
            match self.current() {
                None => return Err(ExprError::UnclosedParen('{')),
                Some('}') => {
                    self.pos += 1;
                    break;
                }
                _ => {
                    let key = self.read_word()?;
                    self.skip_space();
                    if self.current() != Some('=') {
                        return Err(ExprError::BadObjectItem);
                    }
                    self.pos += 1;
                    let item = self.parse_value()?;
                    items.push((key, item));
                }
            }
        }
        Ok(Expr::Object(items))
    }

    fn parse_var(&mut self) -> Result<Expr, ExprError> {
        self.pos += 1;
        if self.current() == Some('{') {
            self.pos += 1;
            let start_pos = self.pos;
            while self.current().is_some_and(|ch| ch != '}') {
                self.pos += 1;
            }
            if self.current().is_none() {
                return Err(ExprError::UnclosedParen('{'));
            }
            let var_name: String = self.chars[start_pos..self.pos].iter().collect();
            self.pos += 1;
            return Ok(Expr::Var(var_name));
        }
        let start_pos = self.pos;
        while self
             .current()
             .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
        {
            self.pos += 1;
        }
        let var_name: String = self.chars[start_pos..self.pos].iter().collect();
        Ok(Expr::Var(var_name))
    }

    fn parse_quoted(&mut self) -> Result<Expr, ExprError> {
        let quote = self.current().unwrap();
        self.pos += 1;
        let mut text = String::new();
        while let Some(ch) = self.current() {
            if ch == quote {
                self.pos += 1;
                return Ok(Expr::Value(Value::Text(text)));
            }
            text.push(ch);
            self.pos += 1;
        }
        Err(ExprError::UnclosedParen(quote))
    }

    fn parse_word(&mut self) -> Result<Expr, ExprError> {
        let word = self.read_word()?;
        if let Ok(n) = word.parse::<i64>() {
            Ok(Expr::Value(Value::Int(n)))
        } else {
            Ok(Expr::Value(Value::Text(word)))
        }
    }

    fn read_word(&mut self) -> Result<String, ExprError> {
        let start_pos = self.pos;
        while let Some(ch) = self.current() {
            if ch.is_whitespace() || matches!(ch, '(' | ')' | '{' | '}' | '=') {
                break;
            }
            self.pos += 1;
        }
        if self.pos == start_pos {
            return Err(ExprError::EmptyExpr);
        }
        Ok(self.chars[start_pos..self.pos].iter().collect())
    }
}

#[cfg(test)]
mod value_tests {
    use super::*;

    fn eval_text(text: &str) -> Value {
        eval(&parse_expr(text).unwrap(), &|_| None)
    }

    #[test]
    fn basic_types() {
        assert_eq!(eval_text("42"), Value::Int(42));
        assert_eq!(eval_text("\"你好\""), Value::Text("你好".into()));
        assert_eq!(eval_text("甲"), Value::Text("甲".into()));
    }

    #[test]
    fn arrays_and_objects() {
        assert_eq!(
            eval_text("(1 2 三)"),
            Value::Array(vec![Value::Int(1), Value::Int(2), Value::Text("三".into())])
        );
        assert_eq!(
            eval_text("{甲=1 乙=(2 3)}"),
            Value::Object(vec![
                ("甲".into(), Value::Int(1)),
                ("乙".into(), Value::Array(vec![Value::Int(2), Value::Int(3)])),
            ])
        );
        
        let val = eval_text("{甲=1}");
        assert_eq!(eval_text(&val.to_text()), val);
    }

    #[test]
    fn eval_vars() {
        let expr = parse_expr("($甲 $乙)").unwrap();
        let val = eval(&expr, &|var_name| match var_name {
            "甲" => Some("1".into()),
            "乙" => Some("你好".into()),
            _ => None,
        });
        assert_eq!(
            val,
            Value::Array(vec![Value::Text("1".into()), Value::Text("你好".into())])
        );
    }

    #[test]
    fn json_roundtrip() {
        for orig_text in [r#"{"a":"x","n":3}"#, r#"[1,2,"三"]"#, r#""文本""#, r#"7"#] {
            let val = Value::read_json(orig_text).unwrap();
            assert_eq!(Value::read_json(&val.to_json()).unwrap(), val);
        }
        assert_eq!(
            Value::read_json(r#"{"n":3}"#).unwrap(),
            Value::Object(vec![("n".into(), Value::Int(3))])
        );
        assert!(Value::read_json("{不是JSON}").is_none());
    }

    #[test]
    fn value_text_roundtrip() {
        for orig_text in ["(1 2 三)", "{甲=1 乙=(2 3)}", "42", "{单}"] {
            let val = Value::from_value_text(orig_text).unwrap_or_else(|| Value::Text(orig_text.into()));
            assert_eq!(val.to_text(), orig_text);
        }
        assert_eq!(Value::from_value_text("(1 2 三)"), Some(Value::Array(vec![Value::Int(1), Value::Int(2), Value::Text("三".into())])));
        assert_eq!(Value::from_value_text("普通文本"), None);
    }

    #[test]
    fn write_json_and_ini() {
        let object = Value::read_json(r#"{"名称":"壳","端口":8080,"标签":["甲","乙"],"服务":{"地址":"127.0.0.1:80"}}"#).unwrap();
        let json = Value::write_typed(&object.to_json(), "JSON").unwrap();
        assert!(json.contains("\n  \"名称\": \"壳\""));
        assert_eq!(Value::read_json(&json).unwrap(), object);

        let ini = Value::write_typed(&object.to_text(), "INI").unwrap();
        let read_back = read_ini(&ini);
        assert_eq!(
            read_back,
            Value::Object(vec![
                (
                    "全局".into(),
                    Value::Object(vec![
                        ("名称".into(), Value::Text("壳".into())),
                        ("端口".into(), Value::Int(8080)),
                        (
                            "标签".into(),
                            Value::Array(vec![Value::Text("甲".into()), Value::Text("乙".into())])
                        ),
                    ])
                ),
                (
                    "服务".into(),
                    Value::Object(vec![("地址".into(), Value::Text("127.0.0.1:80".into()))])
                ),
            ])
        );
        assert_eq!(Value::write_typed("原样", "文本").unwrap(), "原样");
        assert!(Value::write_typed("x", "XML").is_err());
    }

    #[test]
    fn parse_ini() {
        let text = "[全局]\n名称=壳\n# 注释\n[服务]\n端口=8080\n地址=\"127.0.0.1:80\"\n标签=甲, 乙 ";
        let result = Value::parse_typed(text, "INI").unwrap();
        assert_eq!(
            result,
            Value::Object(vec![
                (
                    "全局".into(),
                    Value::Object(vec![("名称".into(), Value::Text("壳".into()))])
                ),
                (
                    "服务".into(),
                    Value::Object(vec![
                        ("端口".into(), Value::Int(8080)),
                        ("地址".into(), Value::Text("127.0.0.1:80".into())),
                        (
                            "标签".into(),
                            Value::Array(vec![Value::Text("甲".into()), Value::Text("乙".into())])
                        ),
                    ])
                ),
            ])
        );
        assert_eq!(result.to_json(), r#"{"全局":{"名称":"壳"},"服务":{"端口":8080,"地址":"127.0.0.1:80","标签":["甲","乙"]}}"#);
        
        assert!(Value::parse_typed("{}", "json").is_ok());
        assert!(Value::parse_typed("{}", "INI").is_ok());
        assert!(Value::parse_typed("x", "YAML").is_err());
        assert!(Value::parse_typed("{坏", "JSON").is_err());
        assert_eq!(Value::parse_typed("原样 # 不处理", "文本").unwrap(), Value::Text("原样 # 不处理".into()));
    }

    #[test]
    fn expr_error() {
        assert_eq!(parse_expr("(1 2"), Err(ExprError::UnclosedParen('(')));
        assert_eq!(parse_expr("{a}"), Err(ExprError::BadObjectItem));
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Word {
    text: String,
    literal_flags: Vec<bool>,
    
    has_dquote: bool,
}

impl Word {
    
    fn push_char(&mut self, ch: char, is_literal: bool) {
        self.text.push(ch);
        self.literal_flags.push(is_literal);
    }

    
    fn drop_prefix(&mut self, count: usize) {
        self.text = self.text.chars().skip(count).collect();
        self.literal_flags.drain(..count.min(self.literal_flags.len()));
    }
}

#[derive(Debug, Clone, Default)]
struct SimpleCommand {
    
    assigns: Vec<(String, Word)>,
    words: Vec<Word>,
}

impl SimpleCommand {
    
    fn name(&self) -> Option<&str> {
        self.words.first().map(|word| word.text.as_str())
    }

    fn is_empty(&self) -> bool {
        self.assigns.is_empty() && self.words.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Connector {
    Pipeline,
    
    PipeWithErr,
}

#[derive(Debug, Clone, Default)]
struct Pipeline {
    commands: Vec<SimpleCommand>,
    
    connectors: Vec<Connector>,
}

#[derive(Debug, Clone, Default)]
struct Job {
    pipeline: Pipeline,
    
    out: Option<(Word, bool)>,
    
    stderr_redirect: Option<(Word, bool)>,
    
    input: Option<Word>,
    
    merge_err: bool,
    background: bool,
}

#[derive(Debug, Clone)]
enum Stmt {
    
    Jobs(Vec<Job>),
    
    If {
        branches: Vec<(Job, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
    },
    
    For {
        variables: String,
        values: Vec<Word>,
        body: Vec<Stmt>,
    },
    
    While { cond: Job, body: Vec<Stmt> },
    
    Break,
    
    Continue,
    
    Function {
        name: String,
        args: Vec<String>,
        body: Vec<Stmt>,
    },
    
    Return(Option<Expr>),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
enum BlockError {
    #[error("{0}")]
    Inner(SyntaxError),
    #[error("「结束」已移除：块改用缩进表示")]
    NoMoreEnd,
    #[error("「如果」缺少条件")]
    MissingIfCond,
    #[error("「对于」格式应为「对于 变量 词...」")]
    BadFor,
    #[error("「或者」没有匹配的「如果」")]
    OrWithoutIf,
    #[error("「否则」之后不能再有「或者」")]
    ElseAfterOr,
    #[error("「跳出」/「继续」只能用在循环中")]
    JumpOutsideLoop,
    #[error("条件不能使用后台（异步 / &）")]
    CondBackground,
    #[error("「函数」缺少名称")]
    MissingFuncName,
    #[error("「返回」只能用在函数中")]
    ReturnOutsideFunc,
    #[error("{0}")]
    ExprError(ExprError),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("第 {line} 行：{source}")]
struct ScriptError {
    line: usize,
    source: BlockError,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
enum SyntaxError {
    #[error("引号 {0} 未闭合")]
    UnclosedQuote(char),
    #[error("行尾反斜杠没有可转义的字符")]
    TrailingBackslash,
    #[error("缺少命令")]
    MissingCommand,
    #[error("重定向缺少目标文件")]
    MissingRedirectTarget,
    #[error("意外的符号 `{0}`")]
    UnexpectedSymbol(String),
}

fn is_name_char(ch: char) -> bool {
    if ch == '_' {
        return true;
    }
    ch.is_alphanumeric() && !is_cjk_punct(ch)
}

fn is_cjk_punct(ch: char) -> bool {
    matches!(
        ch,
        '\u{3000}'..='\u{303F}'   
        | '\u{FF00}'..='\u{FF0F}' 
        | '\u{FF1A}'..='\u{FF20}'
        | '\u{FF3B}'..='\u{FF40}'
        | '\u{FF5B}'..='\u{FF65}'
    )
}

fn is_space(ch: char) -> bool {
    ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n'
}

fn is_operator(ch: char) -> bool {
    matches!(ch, '|' | '&' | ';' | '<' | '>' | '(' | ')')
}

struct Lexer<'a> {
    chars: &'a [char],
    cursor: usize,
    current_word: Option<Word>,
    
    eq_index: Option<usize>,
    current_cmd: SimpleCommand,
    current_pipeline: Pipeline,
    jobs: Vec<Job>,
    current_job: Job,
}

fn parse_command(input: &str) -> Result<Vec<Job>, SyntaxError> {
    let chars: Vec<char> = input.chars().collect();
    let mut lexer = Lexer {
        chars: &chars,
        cursor: 0,
        current_word: None,
        eq_index: None,
        current_cmd: SimpleCommand::default(),
        current_pipeline: Pipeline::default(),
        jobs: Vec::new(),
        current_job: Job::default(),
    };
    lexer.run_lexer()?;
    Ok(lexer.jobs)
}

fn parse_script(input: &str) -> Result<Vec<Stmt>, ScriptError> {
    let merged = merge_continuations(input);
    let lines: Vec<&str> = merged.lines().collect();
    let mut sp = ScriptParser {
        root: Vec::new(),
        stack: Vec::new(),
        plain_lines: Vec::new(),
    };
    for (idx, orig_text) in lines.iter().enumerate() {
        sp.handle_line(orig_text, idx + 1)?;
    }
    sp.finish_parse(lines.len() + 1)
}

fn merge_continuations(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'\n') {
            chars.next();
        } else {
            result.push(ch);
        }
    }
    result
}

struct BlockFrame {
    indent: usize,
    kind: BlockKind,
}

enum BlockKind {
    If {
        branches: Vec<(Job, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
    },
    For {
        variables: String,
        values: Vec<Word>,
        body: Vec<Stmt>,
    },
    While {
        cond: Job,
        body: Vec<Stmt>,
    },
    Function {
        name: String,
        args: Vec<String>,
        body: Vec<Stmt>,
    },
}

impl BlockKind {
    fn into_stmt(self) -> Stmt {
        match self {
            BlockKind::If { branches, else_body, .. } => Stmt::If { branches, else_body },
            BlockKind::For { variables, values, body, .. } => Stmt::For { variables, values, body },
            BlockKind::While { cond, body, .. } => Stmt::While { cond, body },
            BlockKind::Function { name, args, body, .. } => Stmt::Function { name, args, body },
        }
    }
}

fn indent_width(line: &str) -> usize {
    line.chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .map(|ch| if ch == '\t' { 4 } else { 1 })
        .sum()
}

struct ScriptParser {
    root: Vec<Stmt>,
    stack: Vec<BlockFrame>,
    plain_lines: Vec<String>,
}

impl ScriptParser {
    
    fn body_mut(&mut self) -> &mut Vec<Stmt> {
        if self.stack.is_empty() {
            return &mut self.root;
        }
        match &mut self.stack.last_mut().unwrap().kind {
            BlockKind::For { body, .. } | BlockKind::While { body, .. } | BlockKind::Function { body, .. } => body,
            BlockKind::If {
                branches, else_body, ..
            } => {
                if let Some(body) = else_body {
                    body
                } else {
                    &mut branches.last_mut().unwrap().1
                }
            }
        }
    }

    fn push_stmt(&mut self, stmt: Stmt) {
        self.body_mut().push(stmt);
    }

    fn in_loop(&self) -> bool {
        self.stack
            .iter()
            .any(|frame| matches!(frame.kind, BlockKind::For { .. } | BlockKind::While { .. }))
    }

    fn in_function(&self) -> bool {
        self.stack.iter().any(|frame| matches!(frame.kind, BlockKind::Function { .. }))
    }

    
    fn reduce_top(&mut self, line_no: usize) -> Result<(), ScriptError> {
        self.flush(line_no)?;
        let frame = self.stack.pop().unwrap();
        self.body_mut().push(frame.kind.into_stmt());
        Ok(())
    }

    
    fn flush(&mut self, line_no: usize) -> Result<(), ScriptError> {
        if self.plain_lines.is_empty() {
            return Ok(());
        }
        let text = self.plain_lines.join("\n");
        self.plain_lines.clear();
        let jobs = parse_command(&text)
            .map_err(|source| ScriptError { line: line_no, source: BlockError::Inner(source) })?;
        if !jobs.is_empty() {
            self.push_stmt(Stmt::Jobs(jobs));
        }
        Ok(())
    }

    fn parse_cond(&self, text: &str, line_no: usize) -> Result<Job, ScriptError> {
        let jobs = parse_command(text)
            .map_err(|source| ScriptError { line: line_no, source: BlockError::Inner(source) })?;
        if jobs.len() != 1 {
            return Err(ScriptError { line: line_no, source: BlockError::MissingIfCond });
        }
        let job = jobs.into_iter().next().unwrap();
        if job.background {
            return Err(ScriptError { line: line_no, source: BlockError::CondBackground });
        }
        Ok(job)
    }

    fn parse_for_header(&self, text: &str, line_no: usize) -> Result<(String, Vec<Word>), ScriptError> {
        let bad_for = || ScriptError { line: line_no, source: BlockError::BadFor };
        let jobs = parse_command(text)
            .map_err(|source| ScriptError { line: line_no, source: BlockError::Inner(source) })?;
        if jobs.len() != 1 {
            return Err(bad_for());
        }
        let cmds = &jobs[0].pipeline.commands;
        if cmds.len() != 1 {
            return Err(bad_for());
        }
        let words = &cmds[0].words;
        
        if words.is_empty() || words[0].text.is_empty() {
            return Err(bad_for());
        }
        Ok((words[0].text.clone(), words[1..].to_vec()))
    }

    fn parse_func_header(&self, text: &str, line_no: usize) -> Result<(String, Vec<String>), ScriptError> {
        let bad_func = || ScriptError { line: line_no, source: BlockError::MissingFuncName };
        let jobs = parse_command(text)
            .map_err(|source| ScriptError { line: line_no, source: BlockError::Inner(source) })?;
        if jobs.len() != 1 {
            return Err(bad_func());
        }
        let cmds = &jobs[0].pipeline.commands;
        if cmds.len() != 1 {
            return Err(bad_func());
        }
        let words = &cmds[0].words;
        if words.is_empty() {
            return Err(bad_func());
        }
        let name = words[0].text.clone();
        let args = words[1..].iter().map(|ch| ch.text.clone()).collect();
        Ok((name, args))
    }

    fn handle_line(&mut self, orig_text: &str, line_no: usize) -> Result<(), ScriptError> {
        let left = orig_text.trim_start();
        if left.trim().is_empty() || left.starts_with('#') {
            return Ok(());
        }
        let indent = indent_width(orig_text);
        let line = left.trim_end();
        let first_word = line.split_whitespace().next().unwrap_or("");
        let rest = line[first_word.len()..].trim();
        let is_branch = matches!(first_word, "或者" | "否则");

        
        while self.stack.last().is_some_and(|frame| frame.indent > indent) {
            self.reduce_top(line_no)?;
        }
        if is_branch {
            
            let has_if = self.stack.last().is_some_and(|frame| {
                frame.indent == indent && matches!(frame.kind, BlockKind::If { .. })
            });
            if !has_if {
                return Err(ScriptError { line: line_no, source: BlockError::OrWithoutIf });
            }
        } else {
            
            while self.stack.last().is_some_and(|frame| frame.indent == indent) {
                self.reduce_top(line_no)?;
            }
        }

        match first_word {
            "如果" => {
                self.flush(line_no)?;
                let cond = self.parse_cond(rest, line_no)?;
                self.stack.push(BlockFrame {
                    indent,
                    kind: BlockKind::If {
                        branches: vec![(cond, Vec::new())],
                        else_body: None,
                    },
                });
            }
            "或者" => {
                self.flush(line_no)?;
                let cond = self.parse_cond(rest, line_no)?;
                if let Some(frame) = self.stack.last_mut() {
                    if let BlockKind::If { branches, else_body, .. } = &mut frame.kind {
                        if else_body.is_some() {
                            return Err(ScriptError { line: line_no, source: BlockError::ElseAfterOr });
                        }
                        branches.push((cond, Vec::new()));
                    }
                }
            }
            "否则" => {
                self.flush(line_no)?;
                if let Some(frame) = self.stack.last_mut() {
                    if let BlockKind::If { else_body, .. } = &mut frame.kind {
                        *else_body = Some(Vec::new());
                    }
                }
            }
            "对于" => {
                self.flush(line_no)?;
                let (variables, values) = self.parse_for_header(rest, line_no)?;
                self.stack.push(BlockFrame {
                    indent,
                    kind: BlockKind::For { variables, values, body: Vec::new() },
                });
            }
            "每当" => {
                self.flush(line_no)?;
                let cond = self.parse_cond(rest, line_no)?;
                self.stack.push(BlockFrame {
                    indent,
                    kind: BlockKind::While { cond, body: Vec::new() },
                });
            }
            "函数" => {
                self.flush(line_no)?;
                let (name, args) = self.parse_func_header(rest, line_no)?;
                self.stack.push(BlockFrame {
                    indent,
                    kind: BlockKind::Function { name, args, body: Vec::new() },
                });
            }
            "返回" => {
                self.flush(line_no)?;
                if !self.in_function() {
                    return Err(ScriptError { line: line_no, source: BlockError::ReturnOutsideFunc });
                }
                let expr = if rest.is_empty() {
                    None
                } else {
                    Some(
                        parse_expr(rest).map_err(|source| ScriptError {
                            line: line_no,
                            source: BlockError::ExprError(source),
                        })?,
                    )
                };
                self.push_stmt(Stmt::Return(expr));
            }
            "结束" => {
                return Err(ScriptError { line: line_no, source: BlockError::NoMoreEnd });
            }
            "跳出" | "继续" => {
                self.flush(line_no)?;
                if !self.in_loop() {
                    return Err(ScriptError { line: line_no, source: BlockError::JumpOutsideLoop });
                }
                self.push_stmt(if first_word == "跳出" { Stmt::Break } else { Stmt::Continue });
            }
            _ => self.plain_lines.push(orig_text.to_string()),
        }
        Ok(())
    }

    fn finish_parse(mut self, line_no: usize) -> Result<Vec<Stmt>, ScriptError> {
        self.flush(line_no)?;
        while !self.stack.is_empty() {
            self.reduce_top(line_no)?;
        }
        Ok(self.root)
    }
}

impl<'a> Lexer<'a> {
    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.cursor + offset).copied()
    }

    fn current_word_mut(&mut self) -> &mut Word {
        self.current_word.get_or_insert_with(Word::default)
    }

    
    fn collect_word(&mut self) {
        let eq_pos_field = self.eq_index.take();
        let Some(mut word_in_progress) = self.current_word.take() else {
            return;
        };
        
        
        if eq_pos_field.is_none()
            && self.current_cmd.words.is_empty()
            && word_in_progress.text == "异步"
            && !word_in_progress.has_dquote
            && word_in_progress.literal_flags.iter().all(|&literal| !literal)
        {
            self.current_job.background = true;
            return;
        }
        if let Some(eq_pos) = eq_pos_field.filter(|_| self.current_cmd.words.is_empty()) {
            
            let name_chars: Vec<char> = word_in_progress.text.chars().take(eq_pos).collect();
            let valid_name = name_chars
                .first()
                .is_some_and(|first_char| first_char.is_alphabetic() || *first_char == '_')
                && name_chars
                    .iter()
                    .all(|ch| ch.is_alphanumeric() || *ch == '_');
            if valid_name {
                
                word_in_progress.drop_prefix(eq_pos + 1);
                let name: String = name_chars.into_iter().collect();
                self.current_cmd.assigns.push((name, word_in_progress));
                return;
            }
        }
        self.current_cmd.words.push(word_in_progress);
    }

    fn end_cmd(&mut self) {
        self.collect_word();
        if self.current_cmd.is_empty() {
            return;
        }
        self.current_pipeline.commands.push(std::mem::take(&mut self.current_cmd));
    }

    
    fn end_job(&mut self) -> Result<(), SyntaxError> {
        self.end_cmd();
        
        if self.current_job.background && self.current_pipeline.commands.is_empty() {
            return Err(SyntaxError::MissingCommand);
        }
        if self.current_pipeline.commands.len() <= self.current_pipeline.connectors.len() && !self.current_pipeline.commands.is_empty()
        {
            self.current_pipeline = Pipeline::default();
            return Err(SyntaxError::MissingCommand);
        }
        if !self.current_pipeline.commands.is_empty() {
            self.current_job.pipeline = std::mem::take(&mut self.current_pipeline);
            let finished = std::mem::take(&mut self.current_job);
            self.jobs.push(finished);
        }
        Ok(())
    }

    
    fn read_redirect_target(&mut self) -> Result<Word, SyntaxError> {
        while self.peek(0).is_some_and(is_space) {
            self.cursor += 1;
        }
        let mut target = Word::default();
        while let Some(ch) = self.peek(0) {
            match ch {
                '\\' => {
                    self.cursor += 1;
                    match self.peek(0) {
                        Some(next_byte) => {
                            target.push_char(next_byte, true);
                            self.cursor += 1;
                        }
                        None => return Err(SyntaxError::TrailingBackslash),
                    }
                }
                '\'' | '"' => {
                    self.read_quote_into(&mut target, ch)?;
                }
                _ if is_space(ch) || is_operator(ch) => break,
                _ => {
                    target.push_char(ch, false);
                    self.cursor += 1;
                }
            }
        }
        if target.text.is_empty() {
            return Err(SyntaxError::MissingRedirectTarget);
        }
        Ok(target)
    }

    
    fn read_quote_into(&mut self, target: &mut Word, quote: char) -> Result<(), SyntaxError> {
        let from_single = quote == '\'';
        if !from_single {
            target.has_dquote = true;
        }
        self.cursor += 1;
        while let Some(inner) = self.peek(0) {
            if inner == quote {
                self.cursor += 1;
                return Ok(());
            }
            if inner == '\\' {
                self.cursor += 1;
                match self.peek(0) {
                    Some(next_byte) => {
                        target.push_char(next_byte, from_single);
                        self.cursor += 1;
                    }
                    None => return Err(SyntaxError::TrailingBackslash),
                }
                continue;
            }
            target.push_char(inner, from_single);
            self.cursor += 1;
        }
        Err(SyntaxError::UnclosedQuote(quote))
    }

    fn run_lexer(&mut self) -> Result<(), SyntaxError> {
        while let Some(ch) = self.peek(0) {
            match ch {
                
                '\\' => {
                    self.cursor += 1;
                    match self.peek(0) {
                        None => return Err(SyntaxError::TrailingBackslash),
                        Some('\n') => self.cursor += 1,
                        Some(next_byte) => {
                            self.current_word_mut().push_char(next_byte, false);
                            self.cursor += 1;
                        }
                    }
                }
                '\'' | '"' => self.read_quote(ch)?,
                '\n' => {
                    self.cursor += 1;
                    self.end_job()?;
                }
                c if is_space(c) => {
                    self.collect_word();
                    self.cursor += 1;
                }
                
                '#' if self.current_word.is_none() => {
                    while self.peek(0).is_some_and(|next_byte| next_byte != '\n') {
                        self.cursor += 1;
                    }
                }
                ';' => {
                    self.cursor += 1;
                    self.end_job()?;
                }
                '|' => self.read_pipe()?,
                
                '&' if self.peek(1) == Some('>') => {
                    let push_char = self.peek(2) == Some('>');
                    self.cursor += if push_char { 3 } else { 2 };
                    let target = self.read_redirect_target()?;
                    self.current_job.out = Some((target.clone(), push_char));
                    self.current_job.stderr_redirect = Some((target, push_char));
                }
                
                '&' if self.current_pipeline.commands.len() > self.current_pipeline.connectors.len() => {}
                '&' => {
                    self.collect_word();
                    if self.current_cmd.is_empty() {
                        return Err(SyntaxError::MissingCommand);
                    }
                    self.cursor += 1;
                    self.end_cmd();
                    self.current_job.background = true;
                    self.end_job()?;
                }
                '<' => {
                    self.collect_word();
                    self.cursor += 1;
                    let target = self.read_redirect_target()?;
                    self.current_job.input = Some(target);
                }
                '>' => {
                    self.collect_word();
                    let push_char = self.peek(1) == Some('>');
                    self.cursor += if push_char { 2 } else { 1 };
                    let target = self.read_redirect_target()?;
                    self.current_job.out = Some((target, push_char));
                }
                
                '2' if self.current_word.is_none() && self.peek(1) == Some('>') => {
                    self.read_err_redirect()?;
                }
                
                
                '(' if self.current_word.as_ref().is_some_and(|word| word.text.ends_with('$')) => {
                    self.read_cmdsub()?;
                }
                '(' | ')' => return Err(SyntaxError::UnexpectedSymbol(ch.to_string())),
                _ => {
                    if ch == '=' && self.eq_index.is_none() {
                        let typed = self
                            .current_word
                            .as_ref()
                            .map_or(0, |word| word.text.chars().count());
                        self.eq_index = Some(typed);
                    }
                    self.current_word_mut().text.push(ch);
                    self.cursor += 1;
                }
            }
        }
        self.end_job()
    }

    
    fn read_err_redirect(&mut self) -> Result<(), SyntaxError> {
        if self.peek(2) == Some('&') && self.peek(3) == Some('1') {
            self.cursor += 4;
            self.merge_stderr();
            return Ok(());
        }
        let push_char = self.peek(2) == Some('>');
        self.cursor += if push_char { 3 } else { 2 };
        let target = self.read_redirect_target()?;
        self.current_job.stderr_redirect = Some((target, push_char));
        Ok(())
    }

    
    fn read_cmdsub(&mut self) -> Result<(), SyntaxError> {
        let mut depth = 1usize;
        let mut quote: Option<char> = None;
        self.current_word_mut().push_char('(', false);
        self.cursor += 1;
        while let Some(ch) = self.peek(0) {
            self.cursor += 1;
            self.current_word_mut().push_char(ch, false);
            if let Some(close_quote) = quote {
                if ch == close_quote {
                    quote = None;
                }
            } else {
                match ch {
                    '\'' | '"' => quote = Some(ch),
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
        }
        Err(SyntaxError::UnclosedQuote(')'))
    }

    
    fn read_pipe(&mut self) -> Result<(), SyntaxError> {
        self.collect_word();
        if self.current_cmd.is_empty() && self.current_pipeline.commands.is_empty() {
            return Err(SyntaxError::MissingCommand);
        }
        self.current_pipeline.commands.push(std::mem::take(&mut self.current_cmd));

        self.cursor += 1;
        let connectors = if self.peek(0) == Some('&') {
            self.cursor += 1;
            Connector::PipeWithErr
        } else {
            Connector::Pipeline
        };
        self.current_pipeline.connectors.push(connectors);
        Ok(())
    }

    
    fn merge_stderr(&mut self) {
        self.current_job.merge_err = true;
    }

    fn read_quote(&mut self, quote: char) -> Result<(), SyntaxError> {
        self.cursor += 1;
        let from_single = quote == '\'';
        while let Some(ch) = self.peek(0) {
            self.cursor += 1;
            if ch == quote {
                if quote == '"' {
                    self.current_word_mut().has_dquote = true;
                }
                return Ok(());
            }
            
            if ch == '\\' {
                match self.peek(0) {
                    Some(next_byte) if from_single && next_byte == '\'' => {
                        self.cursor += 1;
                        self.current_word_mut().push_char('\'', true);
                        continue;
                    }
                    Some(next_byte @ ('$' | '`' | '"' | '\\')) if !from_single => {
                        self.cursor += 1;
                        self.current_word_mut().push_char(next_byte, false);
                        continue;
                    }
                    Some('\n') => {
                        self.cursor += 1;
                        continue;
                    }
                    Some(other) if !from_single => {
                        let current_word = self.current_word_mut();
                        current_word.push_char('\\', false);
                        current_word.push_char(other, false);
                        self.cursor += 1;
                        continue;
                    }
                    Some(other) => {
                        
                        self.current_word_mut().push_char('\\', true);
                        let _ = other;
                        continue;
                    }
                    None => return Err(SyntaxError::TrailingBackslash),
                }
            }
            
            if !from_single && ch == '$' && self.peek(0) == Some('(') {
                self.current_word_mut().push_char('$', false);
                self.read_cmdsub()?;
                continue;
            }
            self.current_word_mut().push_char(ch, from_single);
        }
        Err(SyntaxError::UnclosedQuote(quote))
    }
}

#[allow(clippy::type_complexity)]
fn expand_word(
    word: &Word,
    lookup: &dyn Fn(&str) -> Option<String>,
    cmdsub: &dyn Fn(&str, &dyn Fn(&str) -> Option<String>) -> String,
) -> Vec<String> {
    let text = expand_vars(word, lookup, cmdsub);
    if let Some(matches) = expand_wildcard(word, &text) {
        return matches;
    }
    vec![text]
}

#[allow(clippy::type_complexity)]
fn expand_vars(
    word: &Word,
    lookup: &dyn Fn(&str) -> Option<String>,
    cmdsub: &dyn Fn(&str, &dyn Fn(&str) -> Option<String>) -> String,
) -> String {
    let chars: Vec<char> = word.text.chars().collect();
    let mut result = String::new();
    let mut cursor = 0;
    while cursor < chars.len() {
        let is_lit = word.literal_flags.get(cursor).copied().unwrap_or(false);
        if chars[cursor] != '$' || is_lit {
            result.push(chars[cursor]);
            cursor += 1;
            continue;
        }
        match chars.get(cursor + 1) {
            
            Some('(') => {
                let cmd_start = cursor + 2;
                let mut depth = 1;
                let mut end = cmd_start;
                while end < chars.len() {
                    match chars[end] {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    end += 1;
                }
                if depth == 0 {
                    let commands: String = chars[cmd_start..end].iter().collect();
                    result.push_str(&cmdsub(&commands, lookup));
                    cursor = end + 1;
                } else {
                    result.push('$');
                    cursor += 1;
                }
            }
            
            Some('{') => {
                let name_start = cursor + 2;
                match chars[name_start..].iter().position(|&ch| ch == '}') {
                    Some(offset) => {
                        let name: String = chars[name_start..name_start + offset].iter().collect();
                        result.push_str(&lookup(&name).unwrap_or_default());
                        cursor = name_start + offset + 1;
                    }
                    
                    None => {
                        result.push('$');
                        cursor += 1;
                    }
                }
            }
            
            Some(&first_char) if is_name_char(first_char) => {
                let name_start = cursor + 1;
                let mut name_end = name_start;
                
                
                while name_end < chars.len()
                    && is_name_char(chars[name_end])
                    && chars[name_end] != '('
                    && chars[name_end] != ')'  {
                    name_end += 1;
                }
                let name: String = chars[name_start..name_end].iter().collect();
                result.push_str(&lookup(&name).unwrap_or_default());
                cursor = name_end;
            }
            
            Some(&special) if matches!(special, '?' | '$' | '!' | '#' | '@' | '*') => {
                result.push_str(&lookup(&special.to_string()).unwrap_or_default());
                cursor += 2;
            }
            _ => {
                result.push('$');
                cursor += 1;
            }
        }
    }

    
    let first_is_literal = word.literal_flags.first().copied().unwrap_or(false);
    if !first_is_literal && !word.has_dquote {
        if let Some(home_dir) = lookup("HOME") {
            if result == "~" {
                return home_dir;
            }
            if let Some(rest) = result.strip_prefix("~/") {
                return format!("{home_dir}/{rest}");
            }
        }
    }
    result
}

fn expand_wildcard(word: &Word, pattern: &str) -> Option<Vec<String>> {
    if word.has_dquote {
        return None;
    }
    let ch: Vec<char> = pattern.chars().collect();
    if word.text.chars().count() != ch.len() {
        return None;
    }
    if !ch.iter().enumerate().any(|(idx, ch)| {
        matches!(ch, '*' | '?' | '[') && !word.literal_flags.get(idx).copied().unwrap_or(false)
    }) {
        return None;
    }
    let (dir_part, name_pattern) = match pattern.rfind('/') {
        Some(pos) => (&pattern[..=pos], &pattern[pos + 1..]),
        None => ("", pattern),
    };
    let read_dir_path = if dir_part.is_empty() { "." } else { dir_part };
    
    let matcher = glob::Pattern::new(&name_pattern.replace("[^", "[!")).ok()?;
    let show_hidden = name_pattern.starts_with('.');
    let read_dir = std::fs::read_dir(read_dir_path).ok()?;
    let mut matches: Vec<String> = Vec::new();
    for entry in read_dir.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.starts_with('.') && !show_hidden {
            continue;
        }
        if matcher.matches(&file_name) {
            matches.push(format!("{dir_part}{file_name}"));
        }
    }
    matches.sort_unstable();
    if matches.is_empty() {
        None
    } else {
        Some(matches)
    }
}

#[cfg(test)]
fn pattern_match(pattern: &str, text: &str) -> bool {
    
    let normalized = pattern.replace("[^", "[!");
    match glob::Pattern::new(&normalized) {
        Ok(pattern) => pattern.matches(text),
        Err(_) => pattern == text,
    }
}

#[cfg(test)]
mod lexer_tests {
    use super::*;

    fn test_var(key: &str) -> Option<String> {
        match key {
            "HOME" => Some("/home/u".into()),
            "名字" => Some("世界".into()),
            _ => None,
        }
    }

    fn expand_all(commands: &SimpleCommand) -> Vec<String> {
        commands
            .words
            .iter()
            .flat_map(|word| expand_word(word, &test_var, &|_, _| String::new()))
            .collect()
    }

    #[test]
    fn basic_tokenize() {
        
        let jobs = parse_command(r#"echo 你好 "a b" 'c $d' "$名字""#).unwrap();
        assert_eq!(jobs.len(), 1);
        let expanded = expand_all(&jobs[0].pipeline.commands[0]);
        assert_eq!(expanded, ["echo", "你好", "a b", "c $d", "世界"]);
    }

    #[test]
    fn pipeline_and_redirect() {
        let jobs = parse_command("ls -l | grep rust > /tmp/x 2>&1").unwrap();
        let first_job = &jobs[0];
        assert_eq!(first_job.pipeline.commands.len(), 2);
        assert_eq!(first_job.out.as_ref().unwrap().0.text, "/tmp/x");
        assert!(first_job.merge_err);
        assert!(first_job.stderr_redirect.is_none());
    }

    #[test]
    fn err_redirect() {
        let jobs = parse_command("cmd 2>>err.log <in.txt").unwrap();
        let first_job = &jobs[0];
        assert_eq!(first_job.stderr_redirect.as_ref().unwrap().0.text, "err.log");
        assert!(first_job.stderr_redirect.as_ref().unwrap().1);
        assert_eq!(first_job.input.as_ref().unwrap().text, "in.txt");

        
        let jobs = parse_command("cmd &>both.log 2>err.log").unwrap();
        let first_job = &jobs[0];
        assert_eq!(first_job.out.as_ref().unwrap().0.text, "both.log");
        assert_eq!(first_job.stderr_redirect.as_ref().unwrap().0.text, "err.log");

        
        let jobs = parse_command("echo 2-3").unwrap();
        assert_eq!(expand_all(&jobs[0].pipeline.commands[0]), ["echo", "2-3"]);
    }

    #[test]
    fn background_and_semicolon() {
        let jobs = parse_command("sleep 1 & echo hi; pwd").unwrap();
        assert_eq!(jobs.len(), 3);
        assert!(jobs[0].background);
        assert!(!jobs[1].background);
    }

    #[test]
    fn async_keyword() {
        
        let jobs = parse_command("异步 sleep 3; echo hi").unwrap();
        assert_eq!(jobs.len(), 2);
        assert!(jobs[0].background);
        assert_eq!(expand_all(&jobs[0].pipeline.commands[0]), ["sleep", "3"]);
        assert!(!jobs[1].background);
        assert_eq!(expand_all(&jobs[1].pipeline.commands[0]), ["echo", "hi"]);

        
        let jobs = parse_command("异步 A=1 echo hi | cat").unwrap();
        assert!(jobs[0].background);
        assert_eq!(jobs[0].pipeline.commands.len(), 2);
        assert_eq!(jobs[0].pipeline.commands[0].assigns.len(), 1);

        
        assert_eq!(parse_command("异步").unwrap_err(), SyntaxError::MissingCommand);
        assert_eq!(parse_command("异步 > 文件").unwrap_err(), SyntaxError::MissingCommand);

        
        for input in ["'异步' arg", "\"异步\" arg"] {
            let jobs = parse_command(input).unwrap();
            assert!(!jobs[0].background, "输入：{input}");
            assert_eq!(expand_all(&jobs[0].pipeline.commands[0]), ["异步", "arg"]);
        }

        
        let jobs = parse_command("echo 异步").unwrap();
        assert!(!jobs[0].background);
        assert_eq!(expand_all(&jobs[0].pipeline.commands[0]), ["echo", "异步"]);
    }

    #[test]
    fn cmdsub_not_swallowed() {
        
        let jobs = parse_command(r#"x="$(读取文本 配置.json --类型 JSON)""#).unwrap();
        let value_word = &jobs[0].pipeline.commands[0].assigns[0].1;
        let expanded = expand_word(value_word, &|_| None, &|commands, _| format!("CMD[{commands}]"));
        assert_eq!(expanded, vec!["CMD[读取文本 配置.json --类型 JSON]"]);

        let jobs = parse_command(r#"x="$(回显 甲)""#).unwrap();
        let value_word = &jobs[0].pipeline.commands[0].assigns[0].1;
        assert_eq!(
            expand_word(value_word, &|_| None, &|commands, _| format!("<{commands}>")),
            vec!["<回显 甲>"]
        );
    }

    #[test]
    fn cmdsub_in_double_quote() {
        
        let jobs = parse_command(r#"x="$(echo "a b")""#).unwrap();
        let commands = &jobs[0].pipeline.commands[0];
        assert_eq!(commands.assigns.len(), 1);
        let value_word = &commands.assigns[0].1;
        let expanded = expand_word(value_word, &|_| None, &|commands, _| format!("CMD[{commands}]"));
        assert_eq!(expanded, vec!["CMD[echo \"a b\"]"]);
    }

    #[test]
    fn vars_and_tilde() {
        let jobs = parse_command("echo $名字 ~/x ${HOME} $未知").unwrap();
        let expanded = expand_all(&jobs[0].pipeline.commands[0]);
        assert_eq!(expanded, ["echo", "世界", "/home/u/x", "/home/u", ""]);
    }

    #[test]
    fn leading_assign() {
        let jobs = parse_command("A=1 B=\"x y\" echo hi").unwrap();
        let first_cmd = &jobs[0].pipeline.commands[0];
        assert_eq!(first_cmd.assigns.len(), 2);
        assert_eq!(first_cmd.assigns[1].1.text, "x y");
        assert_eq!(expand_all(first_cmd), ["echo", "hi"]);
    }

    #[test]
    fn single_quote_literal() {
        
        let jobs = parse_command(r#"echo 'A'"$名字""#).unwrap();
        assert_eq!(expand_all(&jobs[0].pipeline.commands[0]), ["echo", "A世界"]);

        let jobs = parse_command(r"echo 'a\'b'").unwrap();
        assert_eq!(expand_all(&jobs[0].pipeline.commands[0]), ["echo", "a'b"]);

        
        let jobs = parse_command(r#"echo "x\"y" "a\tb""#).unwrap();
        assert_eq!(
            expand_all(&jobs[0].pipeline.commands[0]),
            ["echo", "x\"y", r"a\tb"]
        );

        
        let jobs = parse_command(r#"echo ~ '~'"#).unwrap();
        assert_eq!(
            expand_all(&jobs[0].pipeline.commands[0]),
            ["echo", "/home/u", "~"]
        );
    }

    #[test]
    fn hash_after_quote() {
        
        let jobs = parse_command(r"echo '#'").unwrap();
        assert_eq!(expand_all(&jobs[0].pipeline.commands[0]), ["echo", "#"]);

        let jobs = parse_command(r"echo prev'#'后").unwrap();
        assert_eq!(expand_all(&jobs[0].pipeline.commands[0]), ["echo", "prev#后"]);
    }

    #[test]
    fn chinese_var_assign() {
        
        let jobs = parse_command("名字=壳语言 echo $名字").unwrap();
        let first_cmd = &jobs[0].pipeline.commands[0];
        assert_eq!(first_cmd.assigns.len(), 1);
        assert_eq!(first_cmd.assigns[0].0, "名字");
        assert_eq!(first_cmd.assigns[0].1.text, "壳语言");
        
        assert_eq!(expand_all(first_cmd), ["echo", "世界"]);

        let jobs = parse_command("名=值").unwrap();
        assert_eq!(jobs[0].pipeline.commands[0].assigns[0].0, "名");
        assert_eq!(jobs[0].pipeline.commands[0].assigns[0].1.text, "值");
    }

    #[test]
    fn unclosed_quote_error() {
        assert_eq!(
            parse_command("echo 'abc").unwrap_err(),
            SyntaxError::UnclosedQuote('\'')
        );
        assert_eq!(
            parse_command("echo \"abc").unwrap_err(),
            SyntaxError::UnclosedQuote('"')
        );
    }

    #[test]
    fn empty_pipeline_error() {
        assert_eq!(parse_command("echo a |").unwrap_err(), SyntaxError::MissingCommand);
        assert_eq!(parse_command("| echo").unwrap_err(), SyntaxError::MissingCommand);
    }

    #[test]
    fn wildcard_match() {
        assert!(pattern_match("*.rs", "main.rs"));
        assert!(!pattern_match("*.rs", "main.py"));
        assert!(pattern_match("a?c", "abc"));
        assert!(pattern_match("[a-c]*", "banana"));
        assert!(pattern_match("[!a-c]*", "dog"));
        assert!(!pattern_match("[!a-c]*", "cat"));
        assert!(pattern_match("[^a-c]*", "dog")); 
        assert!(!pattern_match("[^a-c]*", "cat"));
        assert!(pattern_match("中*", "中文"));
        assert!(pattern_match("*", ""));
    }

    #[test]
    fn redirect_no_newline() {
        
        let jobs = parse_command("echo hi > 输出.txt\npwd").unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].out.as_ref().unwrap().0.text, "输出.txt");
        assert_eq!(expand_all(&jobs[1].pipeline.commands[0]), ["pwd"]);

        
        for (input, expected) in [
            ("echo x > \"带 空格.txt\"", "带 空格.txt"),
            ("echo x > '单 引号.txt'", "单 引号.txt"),
            ("echo x > 反\\ 斜杠.txt", "反 斜杠.txt"),
            ("echo x > a.txt\r\nls", "a.txt"),
        ] {
            let jobs = parse_command(input).unwrap();
            assert_eq!(
                jobs[0].out.as_ref().unwrap().0.text,
                expected,
                "输入：{input}"
            );
        }
        assert_eq!(parse_command("echo x > a.txt\r\nls").unwrap().len(), 2);
    }

    #[test]
    fn comments_and_continuations() {
        let jobs = parse_command("echo hi # 注释\npwd").unwrap();
        assert_eq!(jobs.len(), 2);
        let jobs = parse_command("echo \\\nhi").unwrap();
        assert_eq!(jobs.len(), 1);
    }

    #[test]
    fn pipeline_and_background() {
        let jobs = parse_command("sleep 1 & echo |& cat").unwrap();
        assert_eq!(jobs.len(), 2);
        assert!(jobs[0].background);
        assert_eq!(jobs[1].pipeline.connectors, [Connector::PipeWithErr]);

        let jobs = parse_command("echo a &> 全部.txt").unwrap();
        assert!(!jobs[0].background);
        assert_eq!(jobs[0].out.as_ref().unwrap().0.text, "全部.txt");
        assert_eq!(jobs[0].stderr_redirect.as_ref().unwrap().0.text, "全部.txt");
    }

    #[test]
    fn pipeline_and_error() {
        let jobs = parse_command("make |& tee log").unwrap();
        assert_eq!(jobs[0].pipeline.connectors, [Connector::PipeWithErr]);
        
        let jobs = parse_command("make 2>&1 | tee log").unwrap();
        assert!(jobs[0].merge_err);
        
        assert_eq!(jobs[0].pipeline.connectors, [Connector::Pipeline]);
        let jobs = parse_command("make 2>&1").unwrap();
        assert!(jobs[0].merge_err);
        assert!(jobs[0].pipeline.connectors.is_empty());
    }

    #[test]
    fn script_control_flow() {
        
        let stmt_list = parse_script("如果 正常\n    echo 是\n否则\n    echo 否").unwrap();
        assert_eq!(stmt_list.len(), 1);
        match &stmt_list[0] {
            Stmt::If { branches, else_body } => {
                assert_eq!(branches.len(), 1);
                assert_eq!(else_body.as_ref().unwrap().len(), 1);
            }
            _ => panic!("应为如果语句"),
        }

        
        let stmt_list =
            parse_script("如果 错误\n    echo 一\n或者 正常\n    echo 二\n否则\n    echo 三").unwrap();
        match &stmt_list[0] {
            Stmt::If { branches, else_body } => {
                assert_eq!(branches.len(), 2);
                assert!(else_body.is_some());
            }
            _ => panic!("应为如果语句"),
        }

        
        let stmt_list = parse_script("对于 项 甲 乙\n    echo $项").unwrap();
        match &stmt_list[0] {
            Stmt::For { variables, values, body } => {
                assert_eq!(variables, "项");
                assert_eq!(values.len(), 2);
                assert_eq!(body.len(), 1);
            }
            _ => panic!("应为对于语句"),
        }
        let stmt_list = parse_script("每当 正常\n    echo 圈").unwrap();
        assert!(matches!(&stmt_list[0], Stmt::While { .. }));

        
        let stmt_list =
            parse_script("对于 外 甲\n    如果 正常\n        对于 内 一\n            echo x").unwrap();
        assert_eq!(stmt_list.len(), 1);

        
        assert_eq!(parse_script("结束").unwrap_err().source, BlockError::NoMoreEnd);
        assert_eq!(
            parse_script("跳出").unwrap_err().source,
            BlockError::JumpOutsideLoop
        );
        assert_eq!(
            parse_script("如果 异步 true\n    echo x").unwrap_err().source,
            BlockError::CondBackground
        );

        
        let stmt_list = parse_script("\"如果\" arg").unwrap();
        assert!(matches!(&stmt_list[0], Stmt::Jobs(_)));
    }

    #[test]
    fn script_function() {
        let stmt_list = parse_script("函数 问候 名字\n    echo 你好\n    返回\n问候 世界").unwrap();
        assert_eq!(stmt_list.len(), 2);
        match &stmt_list[0] {
            Stmt::Function { name, args, body } => {
                assert_eq!(name, "问候");
                assert_eq!(args.len(), 1);
                assert_eq!(args[0], "名字");
                assert_eq!(body.len(), 2);
                assert!(matches!(&body[1], Stmt::Return(_)));
            }
            _ => panic!("应为函数语句"),
        }
        assert!(matches!(&stmt_list[1], Stmt::Jobs(_)));

        
        assert_eq!(
            parse_script("返回 1").unwrap_err().source,
            BlockError::ReturnOutsideFunc
        );
        assert_eq!(
            parse_script("函数").unwrap_err().source,
            BlockError::MissingFuncName
        );
    }
}

enum Plan {
    
    Empty,
    
    Assign(Vec<(String, String)>),
    
    Pipeline(PipelineSpec),
}

struct Planner<'a> {
    
    env_table: &'a std::collections::HashMap<String, String>,
    
    last_status: c_int,
    
    #[allow(clippy::type_complexity)]
    cmdsub: &'a dyn Fn(&str, &dyn Fn(&str) -> Option<String>) -> String,
}

impl<'a> Planner<'a> {
    fn lookup(&self, name: &str) -> Option<String> {
        match name {
            "?" => return Some(self.last_status.to_string()),
            "$" => return Some(std::process::id().to_string()),
            "!" => return Some(last_background_pid().to_string()),
            "#" => return Some(self.arg_count().to_string()),
            "@" | "*" => return Some(self.arg_list().join(" ")),
            
            "参数" => return Some(self.arg_list().into_iter().next().unwrap_or_default()),
            "参数列表" => return Some(self.arg_list().join(" ")),
            "参数个数" => return Some(self.arg_count().to_string()),
            "参数对象" => return Some(self.arg_object().to_json()),
            
            "当前目录" => {
                return Some(match std::env::current_dir() {
                    Ok(dir) => dir.display().to_string(),
                    Err(_) => self.env_table.get("PWD").cloned().unwrap_or_default(),
                })
            }
            "脚本路径" => return Some(self.env_table.get("脚本路径").cloned().unwrap_or_default()),
            "脚本目录" => return Some(self.env_table.get("脚本目录").cloned().unwrap_or_default()),
            _ => {}
        }
        if let Some(value) = self.env_table.get(name) {
            return Some(value.clone());
        }
        std::env::var(name).ok()
    }

    
    fn arg_list(&self) -> Vec<String> {
        self.positional()
    }

    
    fn arg_count(&self) -> usize {
        self.positional().len()
    }

    
    
    
    
    
    
    
    fn arg_object(&self) -> Value {
        use Value as Value;
        let text = |item: &str| Value::Text(item.to_string());

        let args = self.positional();
        let mut command_line = Vec::new();
        let mut options: Vec<(String, Value)> = Vec::new();
        let mut separator = String::new();
        let mut rest = Vec::new();

        let mut index = 0;
        while index < args.len() {
            let arg = args[index].as_str();
            index += 1;
            if arg == "--" {
                separator = "--".to_string();
                rest.extend(args[index..].iter().map(|item| text(item)));
                break;
            }
            if arg == "-" {
                options.push(("-".to_string(), text("true")));
                continue;
            }
            let Some(body) = arg.strip_prefix("--").or_else(|| arg.strip_prefix('-')) else {
                command_line.push(text(arg));
                continue;
            };
            match body.split_once('=') {
                Some((var_name, data)) => options.push((var_name.to_string(), text(data))),
                None if body.is_empty() => command_line.push(text(arg)),
                None => {
                    let short_opt = !arg.starts_with("--");
                    let next_is_value = short_opt
                        && args.get(index).is_some_and(|next_arg| !next_arg.starts_with('-'));
                    let opt_value = if next_is_value {
                        index += 1;
                        args[index - 1].clone()
                    } else {
                        "true".to_string()
                    };
                    options.push((body.to_string(), text(&opt_value)));
                }
            }
        }

        Value::Object(vec![
            ("命令行".to_string(), Value::Array(command_line)),
            ("选项".to_string(), Value::Object(options)),
            ("分隔符".to_string(), text(&separator)),
            ("剩余".to_string(), Value::Array(rest)),
        ])
    }

    
    fn positional(&self) -> Vec<String> {
        let mut item: Vec<(u32, String)> = self
            .env_table
            .iter()
            .filter_map(|(key, value)| {
                if key != "0" && !key.is_empty() && key.bytes().all(|ch| ch.is_ascii_digit()) {
                    key.parse::<u32>().ok().map(|index| (index, value.clone()))
                } else {
                    None
                }
            })
            .collect();
        item.sort_by_key(|(index, _)| *index);
        item.into_iter().map(|(_, value)| value).collect()
    }

    
    fn expand(&self, ch: &Word) -> Vec<String> {
        let lookup = |name: &str| self.lookup(name);
        expand_word(ch, &lookup, self.cmdsub)
    }

    
    fn expand_join(&self, ch: &Word) -> String {
        self.expand(ch).join(" ")
    }

    
    fn actual_args(&self, words: &[Word]) -> Vec<String> {
        words.iter().flat_map(|ch| self.expand(ch)).collect()
    }

    fn plan(&self, job: &Job) -> Result<Plan, String> {
        let cmds = &job.pipeline.commands;
        if cmds.is_empty() {
            return Ok(Plan::Empty);
        }

        
        if cmds.len() == 1 && cmds[0].words.is_empty() {
            let assigns = cmds[0]
                .assigns
                .iter()
                .map(|(name, val)| (name.clone(), self.expand_join(val)))
                .collect();
            return Ok(Plan::Assign(assigns));
        }

        let mut cmd_list: Vec<SimpleCmd> = Vec::with_capacity(cmds.len());
        let mut err_in_pipe: Vec<bool> = Vec::with_capacity(cmds.len());

        for (idx, simple) in cmds.iter().enumerate() {
            let arg_list = self.actual_args(&simple.words);
            if arg_list.is_empty() {
                return Err("展开后没有可执行的命令".into());
            }
            
            let name = simple.name().unwrap_or_default().to_string();
            let goes_to_child = cmds.len() > 1 || job.background;
            let is_builtin = goes_to_child && find_by_name(&name).is_some();

            
            let env = simple
                .assigns
                .iter()
                .map(|(name, val)| (name.clone(), self.expand_join(val)))
                .collect();

            cmd_list.push(SimpleCmd {
                arg_list,
                env,
                builtin: is_builtin,
            });

            if idx + 1 < cmds.len() {
                let connector = job
                    .pipeline
                    .connectors
                    .get(idx)
                    .copied()
                    .unwrap_or(Connector::Pipeline);
                err_in_pipe.push(connector == Connector::PipeWithErr);
            }
        }

        Ok(Plan::Pipeline(PipelineSpec {
            commands: cmd_list,
            err_in_pipe,
            redirect: Redirect {
                input: job.input.as_ref().map(|ch| self.expand_join(ch)),
                out: job
                    .out
                    .as_ref()
                    .map(|(ch, push_char)| (self.expand_join(ch), *push_char)),
                stderr_redirect: job
                    .stderr_redirect
                    .as_ref()
                    .map(|(ch, push_char)| (self.expand_join(ch), *push_char)),
                merge_err: job.merge_err,
            },
            background: job.background,
            input_from_file: job.input.is_some(),
        }))
    }
}

use std::ffi::CString;
use std::process::{Command, ExitStatus};

const STDIN: c_int = 0;
const STDOUT: c_int = 1;
const STDERR: c_int = 2;

use std::sync::atomic::{AtomicI32, Ordering};
static LAST_BACKGROUND: AtomicI32 = AtomicI32::new(0);

fn last_background_pid() -> c_int {
    LAST_BACKGROUND.load(Ordering::Relaxed)
}

struct SimpleCmd {
    arg_list: Vec<String>,
    env: Vec<(String, String)>,
    
    builtin: bool,
}

impl SimpleCmd {
    fn program_name(&self) -> String {
        self.arg_list.first().cloned().unwrap_or_default()
    }
}

#[derive(Default)]
struct Redirect {
    input: Option<String>,
    out: Option<(String, bool)>,
    stderr_redirect: Option<(String, bool)>,
    
    merge_err: bool,
}

impl Redirect {
    fn is_set(&self) -> bool {
        self.input.is_some() || self.out.is_some() || self.stderr_redirect.is_some()
    }
}

struct PipelineSpec {
    commands: Vec<SimpleCmd>,
    
    err_in_pipe: Vec<bool>,
    redirect: Redirect,
    background: bool,
    
    input_from_file: bool,
}

fn stdin_readable() -> bool {
    let mut stat_buf = [0u8; 144];
    
    let result = unsafe { libc::fstat(0, stat_buf.as_mut_ptr().cast()) };
    if result != 0 {
        return false;
    }
    let pattern = u32::from_ne_bytes([stat_buf[24], stat_buf[25], stat_buf[26], stat_buf[27]]);
    matches!(pattern & 0o170000, 0o010000 | 0o100000 | 0o140000)
}

impl PipelineSpec {
    
    fn builtin(&self) -> Option<&Builtin> {
        if self.commands.len() != 1 || self.background {
            return None;
        }
        find_by_name(self.commands.first()?.arg_list.first()?)
    }
}

#[derive(Debug, thiserror::Error)]
enum ExecError {
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
    #[error("等待进程失败: {0}")]
    Wait(std::io::Error),
    #[error("创建子进程失败")]
    Fork,
}

fn run_single(commands: &SimpleCmd) -> Result<c_int, ExecError> {
    let program_name = commands.program_name();
    let mut child = Command::new(&program_name);
    child.args(&commands.arg_list[1..]);
    for (key, val) in &commands.env {
        child.env(key, val);
    }
    let status_out =
        child.status().map_err(|source| ExecError::Exec {
            program: program_name, source
        })?;
    Ok(exit_code(status_out))
}

fn exit_code(status: ExitStatus) -> c_int {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .or_else(|| status.signal().map(|sig| 128 + sig))
        .unwrap_or(1)
}

fn run_pipeline_ext(spec: &PipelineSpec) -> Result<c_int, ExecError> {
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
                    if let Some(fd) = input_file {
                        dup_to(STDIN, fd)?;
                        sys_close(fd);
                    } else if let Some(fd) = read_pipe {
                        dup_to(STDIN, fd)?;
                        sys_close(fd);
                    }
                    if let Some(fd) = write_pipe {
                        dup_to(STDOUT, fd)?;
                        sys_close(fd);
                    } else if let Some(fd) = output_file {
                        dup_to(STDOUT, fd)?;
                        sys_close(fd);
                    }
                    if err_to_pipe {
                        
                        dup_to(STDERR, STDOUT)?;
                    } else if let Some(fd) = err_file {
                        dup_to(STDERR, fd)?;
                        sys_close(fd);
                    } else if spec.redirect.merge_err {
                        dup_to(STDERR, STDOUT)?;
                    }
                    Ok(())
                })()
            };
            if let Err(error) = wired {
                write_stderr(&format!("壳: {error}\n"));
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

fn exec_child(commands: &SimpleCmd) -> ! {
    let program_name = commands.program_name();

    
    
    if commands.builtin {
        let builtin = find_by_name(&program_name).expect("释义层已确认是内置命令");
        
        
        if matches!(builtin, Builtin::Import) && !program_name.starts_with('.') {
            let message = "导入: 不支持在管道/后台里执行。\n";
            unsafe {
                libc::write(2, message.as_ptr().cast(), message.len());
                libc::_exit(1);
            }
        }
        
        for (key, val) in &commands.env {
            std::env::set_var(key, val);
        }
        let mut env_table: Vec<(String, String)> = std::env::vars().collect();
        
        let mut empty_dir_stack: Vec<std::path::PathBuf> = Vec::new();
        let mut empty_ret = String::new();
        let status = builtin.run(
            &commands.arg_list[1..],
            &mut Context {
                env: &mut env_table,
                last_status: 0,
                quit: false,
                dir_stack: &mut empty_dir_stack,
                run_script_file: &mut |_| 1,
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
        std::io::ErrorKind::NotFound => format!("壳: {program_name}: 未找到命令\n"),
        std::io::ErrorKind::PermissionDenied => format!("壳: {program_name}: 权限不足\n"),
        _ => format!("壳: {program_name}: {source}\n"),
    };
    write_stderr(&message);
    let exit_code = if source.kind() == std::io::ErrorKind::NotFound {
        127
    } else {
        126
    };
    sys_exit(exit_code);
}

fn wait_child(
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

fn decode_status(raw_status: c_int) -> (c_int, Option<c_int>) {
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

fn finish(
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

struct RedirectGuard {
    saved_fds: Vec<(c_int, c_int)>,
}

impl RedirectGuard {
    fn apply(redirect: &Redirect) -> Result<Self, ExecError> {
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

struct OutputCapture {
    saved_fd: c_int,
    reader_thread: Option<std::thread::JoinHandle<Vec<u8>>>,
}

impl OutputCapture {
    fn start() -> Result<Self, ExecError> {
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

    
    fn take(mut self) -> Vec<u8> {
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

fn dup_to(target: c_int, from_fd: c_int) -> Result<(), ExecError> {
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

fn open_write(path: &str, push_char: bool) -> Result<c_int, ExecError> {
    let extra_flags = if push_char {
        O_APPEND
    } else {
        O_TRUNC
    };
    open_fd(path, O_WRONLY | O_CREAT | extra_flags, true)
}

fn open_read(path: &str) -> Result<c_int, ExecError> {
    open_fd(path, O_RDONLY, false)
}

fn open_fd(path: &str, flags: c_int, needs_mode: bool) -> Result<c_int, ExecError> {
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

fn write_stderr(text: &str) {
    sys_write(STDERR, text.as_ptr().cast(), text.len());
}

use std::io::{self, Write};

struct Context<'a> {
    env: &'a mut Vec<(String, String)>,
    
    last_status: c_int,
    
    quit: bool,
    
    dir_stack: &'a mut Vec<std::path::PathBuf>,
    
    
    run_script_file: &'a mut dyn FnMut(&str) -> c_int,
    
    ret_object: &'a mut String,
    
    stdin_is_pipe: bool,
}

impl Context<'_> {
    
    fn set_var(&mut self, name: &str, data: &str) {
        match self.env.iter_mut().find(|(existing, _)| existing == name) {
            Some((_, old_value)) => *old_value = data.to_string(),
            None => self.env.push((name.to_string(), data.to_string())),
        }
        std::env::set_var(name, data);
    }

    fn delete_var(&mut self, name: &str) {
        self.env.retain(|(existing, _)| existing != name);
        std::env::remove_var(name);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Builtin {
    EnterDir,
    PushDir,
    PopDir,
    Exit,
    Import,
    DeleteVar,
    Echo,
    Print,
    Exec,
    HelpCmd,
    True,
    False,
    Export,
    ReadFile,
    WriteFile,
}

const ALL: &[(&str, Builtin)] = &[
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
    ("帮助", Builtin::HelpCmd),
    ("正常", Builtin::True),
    ("错误", Builtin::False),
    ("读取文本", Builtin::ReadFile),
    ("覆写文件", Builtin::WriteFile),
];

fn find_by_name(name_str: &str) -> Option<&'static Builtin> {
    ALL
        .iter()
        .find(|(entry_name, _)| *entry_name == name_str)
        .map(|(_, commands)| commands)
}

impl Builtin {
    
    fn run(&self, args: &[String], ctx: &mut Context) -> c_int {
        match self {
            
            Self::Export => Self::run_export(args, ctx),
            Self::EnterDir => Self::run_cd(args),
            Self::PushDir => Self::run_push(args, ctx),
            Self::PopDir => Self::run_pop(ctx),
            Self::Import => Self::run_import(args, ctx),
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
            Self::HelpCmd => {
                help();
                0
            }
            Self::True => 0,
            Self::False => 1,
        }
    }

    fn run_cd(args: &[String]) -> c_int {
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

    
    fn run_push(args: &[String], ctx: &mut Context) -> c_int {
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

    
    fn run_pop(ctx: &mut Context) -> c_int {
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

    
    fn run_import(args: &[String], ctx: &mut Context) -> c_int {
        if args.is_empty() {
            let _ = writeln!(io::stderr(), "导入: 缺少脚本文件名");
            return 1;
        }
        let mut status = 0;
        for file in args {
            status = (ctx.run_script_file)(file);
            if ctx.quit {
                break;
            }
        }
        status
    }

    
    
    
    
    fn run_read_file(args: &[String], ctx: &mut Context) -> c_int {
        
        let dot_escape = args.first().is_some_and(|item| item.starts_with('.') && item != "." && item != "..");
        let (file, kind) = if dot_escape {
            let file = args[0].clone();
            let kind = infer_type(&file);
            (Some(file), Some(kind))
        } else {
            match parse_read_args(args, "读取文件") {
                Ok(result) => result,
                Err(status) => return status,
            }
        };
        
        let read_stdin = ctx.stdin_is_pipe && stdin_readable();
        let orig_text = match file.as_deref() {
            Some(path) => match std::fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) => {
                    let _ = writeln!(io::stderr(), "读取文件: {path}: {error}");
                    return 1;
                }
            },
            None if read_stdin => {
                let mut text = String::new();
                if let Err(error) = io::Read::read_to_string(&mut io::stdin().lock(), &mut text) {
                    let _ = writeln!(io::stderr(), "读取文件: 标准输入: {error}");
                    return 1;
                }
                text
            }
            None => {
                let _ = writeln!(io::stderr(), "读取文件: 缺少文件路径（标准输入不是管道）");
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
                    let _ = writeln!(io::stderr(), "读取文件: （{type_name}）: {message_s}");
                    1
                }
            },
        }
    }

    
    
    
    
    
    fn run_write_file(args: &[String], _ctx: &mut Context) -> c_int {
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

    
    
    
    
    fn run_export(args: &[String], ctx: &mut Context) -> c_int {
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
fn parse_read_args(args: &[String], cmd_name: &str) -> Result<(Option<String>, Option<String>), c_int> {
    
    
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

fn is_known_type(item: &str) -> bool {
    let kind = item.trim_start_matches('-').to_ascii_uppercase();
    matches!(kind.as_str(), "JSON" | "INI" | "TEXT" | "文本")
}

fn split_type_arg(args: &[String], cmd_name: &str) -> (Option<String>, Vec<String>) {
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

fn infer_type(file: &str) -> String {
    match std::path::Path::new(file).extension().and_then(|ext| ext.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("ini") || ext.eq_ignore_ascii_case("conf") => {
            "INI".to_string()
        }
        _ => "JSON".to_string(),
    }
}

fn infer_write_type(file: &str) -> String {
    match std::path::Path::new(file).extension().and_then(|ext| ext.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("json") => "JSON".to_string(),
        Some(ext) if ext.eq_ignore_ascii_case("ini") || ext.eq_ignore_ascii_case("conf") => {
            "INI".to_string()
        }
        _ => "文本".to_string(),
    }
}

fn is_valid_var(name: &str) -> bool {
    let mut chars_iter = name.chars();
    chars_iter
        .next()
        .is_some_and(|first_char| first_char.is_alphabetic() || first_char == '_')
        && chars_iter.all(|ch| ch.is_alphanumeric() || ch == '_')
}

fn print_stack(current: &[std::path::PathBuf], ctx: &Context) {
    let mut out = String::new();
    for path in current {
        out.push_str(&format!(" {}", short_path(path)));
    }
    for path in ctx.dir_stack.iter().rev() {
        out.push_str(&format!(" {}", short_path(path)));
    }
    println!("{out}");
}

fn short_path(path: &std::path::Path) -> String {
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

fn list_vars(ctx: &mut Context) {
    let mut entries: Vec<&(String, String)> = ctx.env.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = io::stdout().lock();
    for (key, val) in entries {
        let _ = writeln!(out, "{key}={val}");
    }
}

fn exec_replace(args: &[String]) -> c_int {
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

fn print_raw(args: &[String]) {
    let text = args.join(" ");
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

fn echo(args: &[String]) {
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

fn interpret_escapes(orig_text: &str) -> String {
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

fn help() {
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
  展开      $(命令)（命令替换，由本 shell 执行，取标准输出；(…) / {{…}} 按数组/对象处理）
            ~/路径、* ? [abc]、# 注释
  省略号    <文字> 去掉尖括号（echo <x=$x>）；文字以 . 开头时第一个词按实词
            「导入」要用「.导入 文件」，才能和管道等符号写在一起

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
                     给 --类型：按 JSON / INI / 文本 解析，打印值文本 ((…) / {{键=值}})，
                     结果也放进 $返回值对象
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
mod builtin_tests {
    use super::*;

    
    fn new_context<'a>(
        env: &'a mut Vec<(String, String)>,
        dir_stack: &'a mut Vec<std::path::PathBuf>,
        script: &'a mut dyn FnMut(&str) -> c_int,
    ) -> Context<'a> {
        let return_value = Box::leak(Box::new(String::new()));
        Context {
            env,
            last_status: 0,
            quit: false,
            dir_stack,
            run_script_file: script,
            ret_object: return_value,
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
        let mut env: Vec<(String, String)> = Vec::new();
        let mut dir_stack: Vec<std::path::PathBuf> = Vec::new();
        let mut script = |_: &str| 0;

        {
            let args = vec![src.display().to_string()];
            let mut ctx = new_context(&mut env, &mut dir_stack, &mut script);
            assert_eq!(Builtin::ReadFile.run(&args, &mut ctx), 0);
            assert_eq!(*ctx.ret_object, "{名称=壳 端口=8080}");
        }
        {
            let args = vec![out_path.display().to_string(), "{名称=壳 端口=8080}".into()];
            let mut ctx = new_context(&mut env, &mut dir_stack, &mut script);
            assert_eq!(Builtin::WriteFile.run(&args, &mut ctx), 0);
            let written = std::fs::read_to_string(&out_path).unwrap();
            assert!(written.contains("[全局]"));
            assert!(written.contains("名称=壳"));
        }
        {
            let orig_text = std::fs::read_to_string(&src).unwrap();
            let args = vec![src.display().to_string(), "--类型".into(), "文本".into()];
            let mut ctx = new_context(&mut env, &mut dir_stack, &mut script);
            assert_eq!(Builtin::ReadFile.run(&args, &mut ctx), 0);
            assert_eq!(*ctx.ret_object, orig_text);
        }
        {
            let args = vec![src.display().to_string(), "--类型".into(), "不认识".into()];
            let mut ctx = new_context(&mut env, &mut dir_stack, &mut script);
            assert_eq!(Builtin::ReadFile.run(&args, &mut ctx), 1);
            assert!(ctx.ret_object.is_empty());
        }
        {
            
            let bad_path = root.join("坏.ini");
            let args = vec![bad_path.display().to_string(), "--类型".into(), "XML".into()];
            let mut ctx = new_context(&mut env, &mut dir_stack, &mut script);
            assert_eq!(Builtin::WriteFile.run(&args, &mut ctx), 1);
            assert!(!bad_path.exists());
        }
        {
            
            let mut ctx = new_context(&mut env, &mut dir_stack, &mut script);
            assert_eq!(Builtin::ReadFile.run(&[], &mut ctx), 1);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn env_list_and_set() {
        let mut env: Vec<(String, String)> =
            vec![("乙".into(), "2".into()), ("甲".into(), "1".into())];
        let commands = Builtin::Export;
        
        assert_eq!(
            commands.run(&[], &mut new_context(&mut env, &mut Vec::new(), &mut |_| 0)),
            0
        );
        
        let args = vec!["甲=覆盖".to_string(), "丙=新".to_string()];
        assert_eq!(
            commands.run(&args, &mut new_context(&mut env, &mut Vec::new(), &mut |_| 0)),
            0
        );
        assert_eq!(env.iter().find(|(key, _)| key == "甲").unwrap().1, "覆盖");
        assert_eq!(env.iter().find(|(key, _)| key == "丙").unwrap().1, "新");
        
        let args = vec!["带空格的=你好 世界".to_string()];
        assert_eq!(
            commands.run(&args, &mut new_context(&mut env, &mut Vec::new(), &mut |_| 0)),
            0
        );
        assert_eq!(
            env.iter().find(|(key, _)| key == "带空格的").unwrap().1,
            "你好 世界"
        );
        
        assert_eq!(
            commands.run(
                &["甲=".to_string()],
                &mut new_context(&mut env, &mut Vec::new(), &mut |_| 0)
            ),
            0
        );
        assert!(env.iter().all(|(key, _)| key != "甲"));
        
        assert_eq!(
            commands.run(
                &["没有等号".to_string()],
                &mut new_context(&mut env, &mut Vec::new(), &mut |_| 0)
            ),
            1
        );
        assert_eq!(
            commands.run(
                &["1坏名=x".to_string()],
                &mut new_context(&mut env, &mut Vec::new(), &mut |_| 0)
            ),
            1
        );
    }

    #[test]
    fn chinese_var_to_child() {
        
        let commands = Builtin::Export;
        let args = vec!["跨进程名字=壳语言".to_string()];
        let mut env: Vec<(String, String)> = Vec::new();
        assert_eq!(
            commands.run(&args, &mut new_context(&mut env, &mut Vec::new(), &mut |_| 0)),
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
        let mut env: Vec<(String, String)> = Vec::new();
        let mut dir_stack: Vec<std::path::PathBuf> = Vec::new();
        let mut script = |_: &str| 0;
        let args = vec![tmp_path.display().to_string()];

        assert_eq!(
            Builtin::PushDir.run(&args, &mut new_context(&mut env, &mut dir_stack, &mut script)),
            0
        );
        assert_eq!(std::env::current_dir().unwrap(), tmp_path);
        assert_eq!(dir_stack, vec![orig.clone()]);

        
        assert_eq!(
            Builtin::PushDir.run(&[], &mut new_context(&mut env, &mut dir_stack, &mut script)),
            0
        );
        assert_eq!(std::env::current_dir().unwrap(), orig);
        assert_eq!(dir_stack, vec![tmp_path.clone()]);

        assert_eq!(
            Builtin::PopDir.run(&[], &mut new_context(&mut env, &mut dir_stack, &mut script)),
            0
        );
        assert_eq!(std::env::current_dir().unwrap(), tmp_path);
        assert!(dir_stack.is_empty());

        
        assert_eq!(
            Builtin::PopDir.run(&[], &mut new_context(&mut env, &mut dir_stack, &mut script)),
            1
        );
        assert_eq!(
            Builtin::PushDir.run(
                &["/不存在的目录".into()],
                &mut new_context(&mut env, &mut dir_stack, &mut script)
            ),
            1
        );
        assert!(dir_stack.is_empty());
        assert_eq!(std::env::current_dir().unwrap(), tmp_path);

        std::env::set_current_dir(orig).unwrap();
    }

    #[test]
    fn import_each_line() {
        let mut env: Vec<(String, String)> = Vec::new();
        let mut dir_stack: Vec<std::path::PathBuf> = Vec::new();
        let mut executed: Vec<String> = Vec::new();
        {
            let mut script = |file: &str| {
                executed.push(file.to_string());
                0
            };
            let args = vec!["一.壳".to_string(), "二.壳".to_string()];
            assert_eq!(
                Builtin::Import.run(&args, &mut new_context(&mut env, &mut dir_stack, &mut script)),
                0
            );
        }
        assert_eq!(executed, vec!["一.壳", "二.壳"]);
        assert_eq!(
            Builtin::Import.run(&[], &mut new_context(&mut env, &mut dir_stack, &mut |_| 0)),
            1
        );
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

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Flow {
    Normal(c_int),
    Break(c_int),
    Continue(c_int),
    Return(c_int),
}

enum RunFeedback {
    Done(c_int),
    Continue,
}

struct Shell {
    variables: HashMap<String, String>,
    
    dir_stack: Vec<std::path::PathBuf>,
    last_status: c_int,
    quit: bool,
    
    exit_code: c_int,
    
    block_buffer: Vec<String>,
    
    functions: HashMap<String, (Vec<String>, Vec<Stmt>)>,
    
    return_value: Value,
    
    ret_object: String,
}

impl Shell {
    fn new() -> Self {
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
            block_buffer: Vec::new(),
            functions: HashMap::new(),
            return_value: Value::Int(0),
            ret_object: String::new(),
        }
    }

    
    
    
    
    fn load_rc(&mut self) {
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

    
    
    
    
    fn run_script_file(&mut self, file_name: &str) -> c_int {
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
        
        self.variables.extend(std::env::vars());

        restore_var(&mut self.variables, "脚本路径", saved_path);
        restore_var(&mut self.variables, "脚本目录", saved_dir);

        match result {
            Flow::Normal(code) => code,
            
            Flow::Break(code) | Flow::Continue(code) | Flow::Return(code) => code,
        }
    }

    
    
    fn set_positional(&mut self, script_path: &str, args: &[String]) {
        self.variables.insert("0".into(), script_path.into());
        for (idx, value) in args.iter().enumerate() {
            self.variables.insert((idx + 1).to_string(), value.clone());
        }
    }

    fn greet(&self) {
        println!(
            "壳 v{}（壳语言）—— 输入 help 查看用法，Ctrl-D 退出。",
            env!("CARGO_PKG_VERSION")
        );
    }

    
    fn prompt(&self) -> String {
        let user = self
            .variables
            .get("USER")
            .or_else(|| self.variables.get("LOGNAME"))
            .cloned()
            .unwrap_or_else(|| "?".into());
        let host = std::fs::read_to_string("/etc/hostname")
            .map(|text| text.trim().to_string())
            .ok()
            .filter(|text| !text.is_empty())
            .or_else(|| self.variables.get("HOSTNAME").cloned())
            .unwrap_or_else(|| "localhost".into());
        let symbol = if self.last_status == 0 { "$" } else { "#" };
        format!("{user}@{host}:{}{symbol} ", short_dir())
    }

    
    
    
    
    
    fn run_jobs(&mut self, jobs: &[Job]) -> c_int {
        for job in jobs {
            if self.quit {
                break;
            }
            
            
            let this_proc = std::sync::Arc::new(std::sync::Mutex::new(Kernel {
                shell: self.clone_shell(),
                changed: false,
            }));
            let plan_result = {
                let planner = Planner {
                    env_table: &self.variables,
                    last_status: self.last_status,
                    cmdsub: &|commands, lookup| Self::capture_cmd(&this_proc, commands, lookup),
                };
                planner.plan(job)
            };
            let kernel_lock = std::sync::Arc::try_unwrap(this_proc).ok().and_then(|lock| lock.into_inner().ok());
            if let Some(kernel) = kernel_lock {
                if kernel.changed {
                    self.adopt(kernel.shell);
                }
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

    
    fn clone_shell(&self) -> Shell {
        Shell {
            variables: self.variables.clone(),
            dir_stack: self.dir_stack.clone(),
            last_status: self.last_status,
            quit: false,
            exit_code: 0,
            block_buffer: Vec::new(),
            functions: self.functions.clone(),
            return_value: self.return_value.clone(),
            ret_object: self.ret_object.clone(),
        }
    }

    
    fn capture_cmd(
        owner: &std::sync::Arc<std::sync::Mutex<Kernel>>,
        commands: &str,
        lookup: &dyn Fn(&str) -> Option<String>,
    ) -> String {
        let expanded_cmd = expand_cmd_text(commands, lookup);
        let Ok(mut lock) = owner.lock() else {
            return String::new();
        };
        lock.capture_output(&expanded_cmd)
    }

    
    fn adopt(&mut self, kernel: Shell) {
        if kernel.variables == self.variables {
            return;
        }
        self.variables = kernel.variables;
    }

    
    fn exec_stmts(&mut self, stmts: &[Stmt]) -> Flow {
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

    fn exec_stmt(&mut self, stmt: &Stmt) -> Flow {
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

    
    fn feed_line(&mut self, line: &str) -> RunFeedback {
        if line.trim().is_empty() {
            if self.block_buffer.is_empty() {
                return RunFeedback::Done(0);
            }
            let text = std::mem::take(&mut self.block_buffer).join("\n");
            return self.run_buffer(&text);
        }
        let multi_line = {
            let first_line = self.block_buffer.first().map(String::as_str).unwrap_or(line);
            let first_word = first_line.split_whitespace().next().unwrap_or("");
            matches!(first_word, "如果" | "或者" | "否则" | "对于" | "每当" | "函数")
        };
        self.block_buffer.push(line.to_string());
        if multi_line {
            RunFeedback::Continue
        } else {
            let text = std::mem::take(&mut self.block_buffer).join("\n");
            self.run_buffer(&text)
        }
    }

    fn run_buffer(&mut self, text: &str) -> RunFeedback {
        match parse_script(text) {
            Ok(stmt_list) => {
                let status = match self.exec_stmts(&stmt_list) {
                    Flow::Normal(code) => code,
                    Flow::Break(code) | Flow::Continue(code) | Flow::Return(code) => code,
                };
                RunFeedback::Done(status)
            }
            Err(error) => {
                eprintln!("语法错误：{error}");
                RunFeedback::Done(2)
            }
        }
    }

    
    fn run_text(&mut self, text: &str) -> c_int {
        let stmt_list = match parse_script(text) {
            Ok(entries) => entries,
            Err(error) => {
                eprintln!("语法错误：{error}");
                return 2;
            }
        };
        match self.exec_stmts(&stmt_list) {
            Flow::Normal(code) => code,
            Flow::Break(code) | Flow::Continue(code) | Flow::Return(code) => code,
        }
    }

    fn run_pipeline(&mut self, spec: PipelineSpec) -> c_int {
        
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
            
            
            if matches!(
                builtin,
                Builtin::Echo
                    | Builtin::Print
                    | Builtin::Exec
                    | Builtin::HelpCmd
                    | Builtin::True
                    | Builtin::False
            ) {
                let mut empty_env: Vec<(String, String)> = Vec::new();
                let mut empty_stack: Vec<std::path::PathBuf> = Vec::new();
                
                let mut ret_object = std::mem::take(&mut self.ret_object);
                let status = {
                    let this = &mut *self;
                    let mut ctx = Context {
                        env: &mut empty_env,
                        last_status: this.last_status,
                        quit: false,
                        dir_stack: &mut empty_stack,
                        run_script_file: &mut |file| this.run_script_file(file),
                        stdin_is_pipe: spec.commands.len() > 1 || spec.input_from_file || stdin_readable(),
                        ret_object: &mut ret_object,
                    };
                    let args = &spec.commands[0].arg_list[1..];
                    let status = builtin.run(args, &mut ctx);
                    if ctx.quit {
                        this.quit = true;
                        this.exit_code = status;
                    }
                    status
                };
                self.ret_object = ret_object;
                self.sync_return_vars();
                return status;
            }

            let mut env_table = self.snapshot_vars();
            let mut dir_stack = std::mem::take(&mut self.dir_stack);
            let mut ret_object = std::mem::take(&mut self.ret_object);
            let status;
            {
                
                let this = &mut *self;
                let mut ctx = Context {
                    env: &mut env_table,
                    last_status: this.last_status,
                    quit: false,
                    dir_stack: &mut dir_stack,
                    run_script_file: &mut |file| this.run_script_file(file),
                    stdin_is_pipe: spec.commands.len() > 1 || spec.input_from_file || stdin_readable(),
                    ret_object: &mut ret_object,
                };
                let args = &spec.commands[0].arg_list[1..];
                status = builtin.run(args, &mut ctx);
                if ctx.quit {
                    this.quit = true;
                    this.exit_code = status;
                }
            }
            self.dir_stack = dir_stack;
            self.ret_object = ret_object;
            self.variables = self.merge_builtin(env_table);
            self.sync_return_vars();
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
                1
            }
        }
    }

    
    fn call_function(
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

        let result = self.exec_stmts(body);
        let code = match result {
            Flow::Normal(code) | Flow::Return(code) => code,
            
            Flow::Break(code) | Flow::Continue(code) => code,
        };

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

    fn snapshot_positional(&self) -> Vec<(String, Option<String>)> {
        self.variables
            .keys()
            .filter(|key| !key.is_empty() && key.chars().all(|ch| ch.is_ascii_digit()))
            .map(|key| (key.clone(), self.variables.get(key).cloned()))
            .collect()
    }

    fn restore_positional(&mut self, snapshot: Vec<(String, Option<String>)>) {
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

    
    
    
    
    
    fn merge_builtin(&self, old_table: Vec<(String, String)>) -> HashMap<String, String> {
        let proc_env: HashMap<String, String> = std::env::vars().collect();
        let mut result: HashMap<String, String> = old_table.into_iter().collect();
        for (key, val) in proc_env {
            result.insert(key, val);
        }
        result
    }

    
    fn sync_return_vars(&mut self) {
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

    fn snapshot_vars(&self) -> Vec<(String, String)> {
        self.variables
            .iter()
            .map(|(key, val)| (key.clone(), val.clone()))
            .collect()
    }
}

struct Kernel {
    shell: Shell,
    changed: bool,
}

impl Kernel {
    
    
    
    
    
    fn capture_output(&mut self, commands: &str) -> String {
        self.changed = true;
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
        let result = self.shell.exec_stmts(&stmt_list);
        let out = capture.take();
        self.shell.last_status = code(result);
        let orig_text = String::from_utf8_lossy(&out).trim_end_matches('\n').to_string();
        
        
        match Value::from_value_text(&orig_text) {
            Some(restored) => restored.to_text(),
            None => orig_text,
        }
    }
}

fn code(result: Flow) -> c_int {
    match result {
        Flow::Normal(code) | Flow::Break(code) | Flow::Continue(code) | Flow::Return(code) => code,
    }
}

fn restore_var(variables: &mut HashMap<String, String>, var_name: &str, raw: Option<String>) {
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

fn short_dir() -> String {
    let home_dir = dirs::home_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "?".into());
    if !home_dir.is_empty() {
        if cwd == home_dir {
            return "~".into();
        }
        if let Some(rest) = cwd.strip_prefix(&format!("{home_dir}/")) {
            return format!("~/{rest}");
        }
    }
    cwd
}

fn expand_cmd_text(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
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
                        match lookup(&var_name) {
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

fn empty_cmdsub(_cmd: &str, _lookup: &dyn Fn(&str) -> Option<String>) -> String {
    String::new()
}

use std::io::{BufRead, IsTerminal};

use clap::Parser;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

#[derive(Parser)]
#[command(
    name = "壳",
    version,
    about = "壳语言：中文命名的 shell",
    disable_help_flag = true
)]
struct Cli {
    #[arg(short = 'c', value_name = "命令")]
    command: Option<String>,

    #[arg(short = 'h', long = "help", action = clap::ArgAction::SetTrue)]
    show_help: bool,

    #[arg(value_name = "脚本")]
    script: Option<String>,

    #[arg(value_name = "参数", trailing_var_arg = true)]
    script_args: Vec<String>,
}

fn main() {
    let args = Cli::parse();
    let mut shell = Shell::new();

    if args.show_help {
        help();
        return;
    }

    if let Some(command_text) = args.command {
        shell.load_rc();
        let status = shell.run_text(&command_text);
        std::process::exit(if shell.quit {
            shell.exit_code
        } else {
            status
        });
    }

    if let Some(script_file) = args.script {
        shell.load_rc();
        shell.set_positional(&script_file, &args.script_args);
        let status = shell.run_script_file(&script_file);
        std::process::exit(if shell.quit {
            shell.exit_code
        } else {
            status
        });
    }

    shell.greet();
    shell.load_rc();

    sys_signal(SIGINT, SIG_IGN);
    sys_signal(SIGTERM, SIG_IGN);

    let is_tty = std::io::stdin().is_terminal();
    let mut editor = if is_tty {
        match DefaultEditor::new() {
            Ok(editor) => Some(editor),
            Err(error) => {
                eprintln!("editor init failed: {error}");
                None
            }
        }
    } else {
        None
    };

    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    let mut line_buf = String::new();
    let mut continued = false;

    loop {
        let line = if let Some(editor) = editor.as_mut() {
            let prompt = if continued {
                "> ".to_string()
            } else {
                shell.prompt()
            };
            match editor.readline(&prompt) {
                Ok(line) => {
                    let _ = editor.add_history_entry(&line);
                    line
                }
                Err(ReadlineError::Interrupted) => {
                    shell.block_buffer.clear();
                    continued = false;
                    shell.last_status = 130;
                    continue;
                }
                Err(ReadlineError::Eof) => break,
                Err(error) => {
                    eprintln!("read failed: {error}");
                    break;
                }
            }
        } else {
            line_buf.clear();
            match reader.read_line(&mut line_buf) {
                Ok(0) => break,
                Ok(_) => line_buf.clone(),
                Err(error) => {
                    eprintln!("read failed: {error}");
                    break;
                }
            }
        };

        match shell.feed_line(&line) {
            RunFeedback::Done(code) => {
                shell.last_status = code;
                continued = false;
            }
            RunFeedback::Continue => continued = true,
        }
        if shell.quit {
            break;
        }
    }

    if !shell.block_buffer.is_empty() {
        if let RunFeedback::Done(code) = shell.feed_line("") {
            shell.last_status = code;
        }
    }

    if is_tty {
        println!("bye.");
    }
    std::process::exit(if shell.quit {
        shell.exit_code
    } else {
        shell.last_status
    });
}
