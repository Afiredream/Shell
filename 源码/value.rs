use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Int(i64),
    Text(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

pub(crate) fn value_word(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|ch| !ch.is_whitespace() && !matches!(ch, '=' | '(' | ')' | '{' | '}' | '"' | ','));
    if plain {
        text.to_string()
    } else {
        let mut out = String::from("\"");
        for ch in text.chars() {
            if ch == '"' || ch == '\\' {
                out.push('\\');
            }
            out.push(ch);
        }
        out.push('"');
        out
    }
}

impl Value {
    pub(crate) fn to_text_part(&self) -> String {
        match self {
            Value::Text(text) if value_word(text) != *text => {
                let mut out = String::from("\"");
                for ch in text.chars() {
                    if ch == '"' || ch == '\\' {
                        out.push('\\');
                    }
                    out.push(ch);
                }
                out.push('"');
                out
            }
            _ => self.to_text(),
        }
    }

    pub(crate) fn to_text(&self) -> String {
        match self {
            Value::Int(n) => n.to_string(),
            Value::Text(text) => text.clone(),
            Value::Array(items) => {
                let content: Vec<String> = items.iter().map(|item| item.to_text_part()).collect();
                format!("({})", content.join(" "))
            }
            Value::Object(items) => {
                let content: Vec<String> = items
                    .iter()
                    .map(|(key, item)| format!("{}={}", value_word(key), item.to_text_part()))
                    .collect();
                format!("{{{}}}", content.join(" "))
            }
        }
    }

    
    #[allow(non_snake_case)]
    pub(crate) fn json(&self) -> String {
        self.to_json()
    }

    
    
    pub(crate) fn from_value_text(text: &str) -> Option<Value> {
        let first = text.chars().next()?;
        if !matches!(first, '(' | '{' | '-' | '+' | '0'..='9') {
            return None;
        }
        let expr = parse_expr(text).ok()?;
        Some(eval(&expr, &|_| None))
    }

    
    pub(crate) fn to_exit_code(&self) -> i64 {
        match self {
            Value::Int(n) => *n,
            Value::Text(text) => text.parse().unwrap_or(0),
            _ => 0,
        }
    }
}

impl Value {
    
    
    pub(crate) fn read_json(text: &str) -> Option<Value> {
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

    
    pub(crate) fn to_json(&self) -> String {
        let mut out = String::new();
        write_json_value(self, &mut out);
        out
    }
}

pub(crate) fn skip_space(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && matches!(bytes[*pos], b' ' | b'\t' | b'\r' | b'\n') {
        *pos += 1;
    }
}

pub(crate) fn read_json_value(bytes: &[u8], pos: &mut usize) -> Option<Value> {
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

pub(crate) fn read_json_string(bytes: &[u8], pos: &mut usize) -> Option<String> {
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
    
    pub(crate) fn parse_typed(text: &str, kind: &str) -> Result<Value, String> {
        match kind.to_ascii_uppercase().as_str() {
            "JSON" => Value::read_json(text).ok_or_else(|| "不是合法的 JSON".to_string()),
            "INI" => Ok(read_ini(text)),
            "文本" | "TEXT" => Ok(Value::Text(text.to_string())),
            other => Err(format!("不支持的类型 `{other}`（可用 JSON、INI、文本）")),
        }
    }
}

impl Value {
    
    
    pub(crate) fn write_typed(text: &str, kind: &str) -> Result<String, String> {
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

pub(crate) fn indent_json(item: &Value, indent: usize) -> String {
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

pub(crate) fn write_ini(item: &Value) -> String {
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

pub(crate) fn read_ini(text: &str) -> Value {
    
    let mut section_name = String::from("全局");
    let mut sections: Vec<(String, Vec<(String, Value)>)> = vec![(section_name.clone(), Vec::new())];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(seg) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            section_name = strip_ws(seg);
            if !sections.iter().any(|(entry_name, _)| *entry_name == section_name) {
                sections.push((section_name.clone(), Vec::new()));
            }
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let key = strip_ws(key);
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

pub(crate) fn strip_ws(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

pub(crate) fn strip_trailing_comment(raw: &str) -> &str {
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

pub(crate) fn ini_value(raw: &str) -> Value {
    
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

pub(crate) fn write_json_value(item: &Value, out: &mut String) {
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

pub(crate) fn write_json_string(text: &str, out: &mut String) {
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
pub(crate) enum Expr {
    Value(Value),
    Var(String),
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub(crate) enum ExprError {
    #[error("{0} 没有闭合")]
    UnclosedParen(char),
    #[error("多余的符号 `{0}`")]
    ExtraSymbol(char),
    #[error("对象项必须写成 `键=值`")]
    BadObjectItem,
    #[error("表达式为空")]
    EmptyExpr,
}

pub(crate) fn eval(expr: &Expr, lookup: &dyn Fn(&str) -> Option<String>) -> Value {
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

pub(crate) fn parse_expr(text: &str) -> Result<Expr, ExprError> {
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

pub(crate) struct ExprParser<'a> {
    pub(crate) chars: &'a [char],
    pub(crate) pos: usize,
}

impl ExprParser<'_> {
    pub(crate) fn current(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    pub(crate) fn skip_space(&mut self) {
        while self.current().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    pub(crate) fn parse_value(&mut self) -> Result<Expr, ExprError> {
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

    pub(crate) fn parse_array(&mut self) -> Result<Expr, ExprError> {
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

    pub(crate) fn parse_object(&mut self) -> Result<Expr, ExprError> {
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
                    let key = if self.current() == Some('"') {
                        self.read_quoted()?
                    } else {
                        self.read_word()?
                    };
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

    pub(crate) fn parse_var(&mut self) -> Result<Expr, ExprError> {
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

    pub(crate) fn read_quoted(&mut self) -> Result<String, ExprError> {
        let quote = self.current().unwrap();
        self.pos += 1;
        let mut text = String::new();
        while let Some(ch) = self.current() {
            if ch == quote {
                self.pos += 1;
                return Ok(text);
            }
            if ch == '\\' {
                self.pos += 1;
                if let Some(next) = self.current() {
                    if next == quote || next == '\\' {
                        text.push(next);
                        self.pos += 1;
                        continue;
                    }
                    text.push('\\');
                    text.push(next);
                    self.pos += 1;
                    continue;
                }
            }
            text.push(ch);
            self.pos += 1;
        }
        Err(ExprError::UnclosedParen(quote))
    }

    pub(crate) fn parse_quoted(&mut self) -> Result<Expr, ExprError> {
        Ok(Expr::Value(Value::Text(self.read_quoted()?)))
    }

    pub(crate) fn parse_word(&mut self) -> Result<Expr, ExprError> {
        let word = self.read_word()?;
        if let Ok(n) = word.parse::<i64>() {
            Ok(Expr::Value(Value::Int(n)))
        } else {
            Ok(Expr::Value(Value::Text(word)))
        }
    }

    pub(crate) fn read_word(&mut self) -> Result<String, ExprError> {
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
        let spaced = Value::Object(vec![(
            "桌面 代".into(),
            Value::Object(vec![("甲".into(), Value::Text("一 二".into()))]),
        )]);
        assert_eq!(Value::from_value_text(&spaced.to_text()), Some(spaced.clone()));
        assert_eq!(spaced.to_text(), r#"{"桌面 代"={甲="一 二"}}"#);
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
