//! Interactive Flake REPL.

use std::io::{self, Write};

use flake_ast::Source;
use flake_interpreter::{Engine, Value};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

use crate::report;

pub fn run() -> i32 {
    println!("Flake {} — Clarity, crystallized.", env!("CARGO_PKG_VERSION"));
    println!("Type :help for help, :quit to exit.\n");

    let mut rl = match DefaultEditor::new() {
        Ok(ed) => ed,
        Err(err) => {
            eprintln!("error: failed to start REPL: {err}");
            return 1;
        }
    };
    let mut engine = Engine::new();
    let mut pending = String::new();

    loop {
        let prompt = if pending.is_empty() { "flake> " } else { "   ... " };
        let line = match rl.readline(prompt) {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                pending.clear();
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!();
                break;
            }
            Err(err) => {
                eprintln!("error: {err}");
                return 1;
            }
        };

        let trimmed = line.trim();
        if pending.is_empty() && trimmed.starts_with(':') {
            match handle_meta(trimmed) {
                Meta::Continue => continue,
                Meta::Quit => break,
            }
        }

        if pending.is_empty() {
            pending = line;
        } else {
            pending.push('\n');
            pending.push_str(&line);
        }

        if !complete_input(&pending) {
            continue;
        }

        let _ = rl.add_history_entry(&pending);
        let source = Source::new("<repl>", pending.clone());
        pending.clear();

        let mut stdout = io::stdout();
        match engine.eval_repl(&source, &mut stdout) {
            Ok(Value::Nil) => {}
            Ok(value) => println!("{}", value.repr()),
            Err(err) => {
                if let Some(span) = err.span() {
                    report::emit(&source, span, &err.to_string());
                } else {
                    report::emit_message(&err.to_string());
                }
            }
        }
        let _ = stdout.flush();
    }
    0
}

enum Meta {
    Continue,
    Quit,
}

fn handle_meta(cmd: &str) -> Meta {
    match cmd {
        ":q" | ":quit" | ":exit" => Meta::Quit,
        ":help" | ":h" => {
            println!(
                "\
Commands:
  :help       Show this help
  :quit       Exit the REPL

Enter expressions, statements, or declarations.
Unclosed '{{' continues on the next line.
Results of expressions are printed automatically."
            );
            Meta::Continue
        }
        other => {
            println!("unknown command `{other}`; try :help");
            Meta::Continue
        }
    }
}

fn complete_input(src: &str) -> bool {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for c in src.chars() {
        if in_string {
            if escape {
                escape = false;
                continue;
            }
            match c {
                '\\' => escape = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth -= 1,
            _ => {}
        }
    }
    !in_string && depth <= 0
}
