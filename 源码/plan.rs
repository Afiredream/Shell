use libc::c_int;
use crate::value::*;
use crate::lexer::*;
use crate::exec::*;
use crate::builtin::*;

pub(crate) enum Plan {
    
    Empty,
    
    Assign(Vec<(String, String)>),
    
    Pipeline(PipelineSpec),
}

pub(crate) struct Planner<'a> {
    
    pub(crate) env_table: &'a std::collections::HashMap<String, String>,
    
    pub(crate) last_status: c_int,
    
    #[allow(clippy::type_complexity)]
    pub(crate) cmdsub: &'a dyn Fn(&str, &dyn Fn(&str) -> Option<String>) -> String,
}

impl<'a> Planner<'a> {
    pub(crate) fn lookup(&self, name: &str) -> Option<String> {
        match name {
            "?" => return Some(self.last_status.to_string()),
            "$" => return Some(std::process::id().to_string()),
            "!" => return Some(last_background_pid().to_string()),
            "#" => return Some(self.arg_count().to_string()),
            
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

    
    pub(crate) fn arg_list(&self) -> Vec<String> {
        self.positional()
    }

    
    pub(crate) fn arg_count(&self) -> usize {
        self.positional().len()
    }

    
    
    
    
    
    
    
    pub(crate) fn arg_object(&self) -> Value {
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

    
    pub(crate) fn positional(&self) -> Vec<String> {
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

    
    pub(crate) fn expand(&self, ch: &Word) -> Vec<String> {
        let lookup = |name: &str| self.lookup(name);
        expand_word(ch, &lookup, self.cmdsub)
    }

    
    pub(crate) fn expand_join(&self, ch: &Word) -> String {
        self.expand(ch).join(" ")
    }

    
    pub(crate) fn actual_args(&self, words: &[Word]) -> Vec<String> {
        words.iter().flat_map(|ch| self.expand(ch)).collect()
    }

    pub(crate) fn plan(&self, job: &Job) -> Result<Plan, String> {
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
