mod sys;
mod value;
mod lexer;
mod plan;
mod exec;
mod builtin;
mod shell;

use crate::shell::Shell;

pub(crate) fn main() {
    let mut argv = std::env::args_os().skip(1);
    let Some(first) = argv.next() else {
        std::process::exit(2);
    };
    let args: Vec<String> = argv.map(|arg| arg.to_string_lossy().into_owned()).collect();
    let first = first.to_string_lossy().into_owned();
    let mut shell = Shell::new();
    shell.load_rc();
    let status = if std::path::Path::new(&first).is_file() {
        shell.set_positional(&first, &args);
        shell.run_script_file(&first)
    } else {
        shell.set_positional("壳", &args);
        shell.run_text(&first)
    };
    std::process::exit(if shell.quit { shell.exit_code } else { status });
}
