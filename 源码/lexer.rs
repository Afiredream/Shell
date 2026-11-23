use crate::value::*;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Word {
    pub(crate) text: String,
    pub(crate) literal_flags: Vec<bool>,
    
    pub(crate) has_dquote: bool,
}

impl Word {
    
    pub(crate) fn push_char(&mut self, ch: char, is_literal: bool) {
        self.text.push(ch);
        self.literal_flags.push(is_literal);
    }

    
    pub(crate) fn drop_prefix(&mut self, count: usize) {
        self.text = self.text.chars().skip(count).collect();
        self.literal_flags.drain(..count.min(self.literal_flags.len()));
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SimpleCommand {
    
    pub(crate) assigns: Vec<(String, Word)>,
    pub(crate) words: Vec<Word>,
}

impl SimpleCommand {
    
    pub(crate) fn name(&self) -> Option<&str> {
        self.words.first().map(|word| word.text.as_str())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.assigns.is_empty() && self.words.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Connector {
    Pipeline,
    
    PipeWithErr,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Pipeline {
    pub(crate) commands: Vec<SimpleCommand>,
    
    pub(crate) connectors: Vec<Connector>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Job {
    pub(crate) pipeline: Pipeline,
    
    pub(crate) out: Option<(Word, bool)>,
    
    pub(crate) stderr_redirect: Option<(Word, bool)>,
    
    pub(crate) input: Option<Word>,
    
    pub(crate) merge_err: bool,
    pub(crate) background: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Stmt {
    
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
pub(crate) enum BlockError {
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
pub(crate) struct ScriptError {
    pub(crate) line: usize,
    pub(crate) source: BlockError,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub(crate) enum SyntaxError {
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

pub(crate) fn is_name_char(ch: char) -> bool {
    if ch == '_' {
        return true;
    }
    ch.is_alphanumeric() && !is_cjk_punct(ch)
}

pub(crate) fn is_cjk_punct(ch: char) -> bool {
    matches!(
        ch,
        '\u{3000}'..='\u{303F}'   
        | '\u{FF00}'..='\u{FF0F}' 
        | '\u{FF1A}'..='\u{FF20}'
        | '\u{FF3B}'..='\u{FF40}'
        | '\u{FF5B}'..='\u{FF65}'
    )
}

pub(crate) fn is_space(ch: char) -> bool {
    ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n'
}

pub(crate) fn is_operator(ch: char) -> bool {
    matches!(ch, '|' | '&' | ';' | '<' | '>' | '(' | ')')
}

pub(crate) struct Lexer<'a> {
    pub(crate) chars: &'a [char],
    pub(crate) cursor: usize,
    pub(crate) current_word: Option<Word>,
    
    pub(crate) eq_index: Option<usize>,
    pub(crate) current_cmd: SimpleCommand,
    pub(crate) current_pipeline: Pipeline,
    pub(crate) jobs: Vec<Job>,
    pub(crate) current_job: Job,
}

pub(crate) fn parse_command(input: &str) -> Result<Vec<Job>, SyntaxError> {
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

pub(crate) fn parse_script(input: &str) -> Result<Vec<Stmt>, ScriptError> {
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

pub(crate) fn merge_continuations(input: &str) -> String {
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

pub(crate) struct BlockFrame {
    pub(crate) indent: usize,
    pub(crate) kind: BlockKind,
}

pub(crate) enum BlockKind {
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
    pub(crate) fn into_stmt(self) -> Stmt {
        match self {
            BlockKind::If { branches, else_body, .. } => Stmt::If { branches, else_body },
            BlockKind::For { variables, values, body, .. } => Stmt::For { variables, values, body },
            BlockKind::While { cond, body, .. } => Stmt::While { cond, body },
            BlockKind::Function { name, args, body, .. } => Stmt::Function { name, args, body },
        }
    }
}

pub(crate) fn indent_width(line: &str) -> usize {
    line.chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .map(|ch| if ch == '\t' { 4 } else { 1 })
        .sum()
}

pub(crate) struct ScriptParser {
    pub(crate) root: Vec<Stmt>,
    pub(crate) stack: Vec<BlockFrame>,
    pub(crate) plain_lines: Vec<String>,
}

impl ScriptParser {
    
    pub(crate) fn body_mut(&mut self) -> &mut Vec<Stmt> {
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

    pub(crate) fn push_stmt(&mut self, stmt: Stmt) {
        self.body_mut().push(stmt);
    }

    pub(crate) fn in_loop(&self) -> bool {
        self.stack
            .iter()
            .any(|frame| matches!(frame.kind, BlockKind::For { .. } | BlockKind::While { .. }))
    }

    pub(crate) fn in_function(&self) -> bool {
        self.stack.iter().any(|frame| matches!(frame.kind, BlockKind::Function { .. }))
    }

    
    pub(crate) fn reduce_top(&mut self, line_no: usize) -> Result<(), ScriptError> {
        self.flush(line_no)?;
        let frame = self.stack.pop().unwrap();
        self.body_mut().push(frame.kind.into_stmt());
        Ok(())
    }

    
    pub(crate) fn flush(&mut self, line_no: usize) -> Result<(), ScriptError> {
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

    pub(crate) fn parse_cond(&self, text: &str, line_no: usize) -> Result<Job, ScriptError> {
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

    pub(crate) fn parse_for_header(&self, text: &str, line_no: usize) -> Result<(String, Vec<Word>), ScriptError> {
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

    pub(crate) fn parse_func_header(&self, text: &str, line_no: usize) -> Result<(String, Vec<String>), ScriptError> {
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

    pub(crate) fn handle_line(&mut self, orig_text: &str, line_no: usize) -> Result<(), ScriptError> {
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

    pub(crate) fn finish_parse(mut self, line_no: usize) -> Result<Vec<Stmt>, ScriptError> {
        self.flush(line_no)?;
        while !self.stack.is_empty() {
            self.reduce_top(line_no)?;
        }
        Ok(self.root)
    }
}

impl<'a> Lexer<'a> {
    pub(crate) fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.cursor + offset).copied()
    }

    pub(crate) fn current_word_mut(&mut self) -> &mut Word {
        self.current_word.get_or_insert_with(Word::default)
    }

    
    pub(crate) fn collect_word(&mut self) {
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

    pub(crate) fn end_cmd(&mut self) {
        self.collect_word();
        if self.current_cmd.is_empty() {
            return;
        }
        self.current_pipeline.commands.push(std::mem::take(&mut self.current_cmd));
    }

    
    pub(crate) fn end_job(&mut self) -> Result<(), SyntaxError> {
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

    
    pub(crate) fn read_redirect_target(&mut self) -> Result<Word, SyntaxError> {
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

    
    pub(crate) fn read_quote_into(&mut self, target: &mut Word, quote: char) -> Result<(), SyntaxError> {
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

    pub(crate) fn run_lexer(&mut self) -> Result<(), SyntaxError> {
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

    
    pub(crate) fn read_err_redirect(&mut self) -> Result<(), SyntaxError> {
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

    
    pub(crate) fn read_cmdsub(&mut self) -> Result<(), SyntaxError> {
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

    
    pub(crate) fn read_pipe(&mut self) -> Result<(), SyntaxError> {
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

    
    pub(crate) fn merge_stderr(&mut self) {
        self.current_job.merge_err = true;
    }

    pub(crate) fn read_quote(&mut self, quote: char) -> Result<(), SyntaxError> {
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
pub(crate) fn expand_word(
    word: &Word,
    lookup: &dyn Fn(&str) -> Option<String>,
    #[allow(clippy::type_complexity)]
    cmdsub: &dyn Fn(&str, &dyn Fn(&str) -> Option<String>) -> String,
) -> Vec<String> {
    let text = expand_vars(word, lookup, cmdsub);
    if let Some(matches) = expand_wildcard(word, &text) {
        return matches;
    }
    vec![text]
}

#[allow(clippy::type_complexity)]
pub(crate) fn resolve_var(name: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let (base, path) = match name.split_once('.') {
        Some((base, path)) => (base, Some(path)),
        None => (name, None),
    };
    let value = lookup(base)?;
    let Some(path) = path else {
        return Some(value);
    };
    let mut current = Value::from_value_text(&value)?;
    for seg in path.split('.') {
        let mut found = None;
        loop {
            match &current {
                Value::Object(items) => {
                    if let Some((_, v)) = items.iter().find(|(key, _)| key == seg) {
                        found = Some(v.clone());
                    } else if items.len() == 1 {
                        current = items.iter().next()?.1.clone();
                        continue;
                    }
                }
                Value::Array(items) => {
                    found = items.get(seg.parse::<usize>().ok()?).cloned();
                }
                _ => {}
            }
            break;
        }
        current = found?;
    }
    Some(current.to_text())
}

#[allow(clippy::type_complexity)]
pub(crate) fn expand_vars(
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
                        result.push_str(&resolve_var(&name, lookup).unwrap_or_default());
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

pub(crate) fn expand_wildcard(word: &Word, pattern: &str) -> Option<Vec<String>> {
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
pub(crate) fn pattern_match(pattern: &str, text: &str) -> bool {
    
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
