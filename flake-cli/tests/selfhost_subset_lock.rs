use flake_ast::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn flake_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_flake"));
    cmd.env("NO_COLOR", "1");
    cmd
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

fn selfhost_main() -> PathBuf {
    repo_root()
        .join("selfhost")
        .join("frontend")
        .join("main.flk")
}

const ALLOWED_EFFECTS: &[&str] = &["io", "alloc", "conc", "panic", "pure"];

const ALLOWED_MODULES: &[&str] = &[
    // Selfhost frontend modules
    "span",
    "tokens",
    "lexer",
    "ast",
    "parser",
    "check",
    "scope",
    "types",
    "effects",
    "ownership",
    "main",
    // Approved stdlib modules
    "fs",
    "path",
    "list",
    "string",
    "option",
    "result",
    "math",
    "map",
    "bytes",
    "channel",
    "process",
];

const APPROVED_BUILTINS: &[&str] = &[
    "print", "println", "len", "str", "push", "pop", "int", "float", "bool",
    "exit", "panic", "assert", "join", "split", "trim", "starts_with",
    "ends_with", "contains", "replace", "to_uppercase", "to_lowercase",
    "is_empty", "keys", "values", "clone", "args", "read_to_string",
    "write_to_string", "read_lines", "write_lines", "append_string",
    "exists", "remove", "file_size", "is_directory", "is_regular_file",
    "is_file", "is_dir", "read_dir", "walk", "Option", "Result",
    "Some", "None", "Ok", "Err", "Task", "sleep", "now", "process", "fs",
    "path", "math", "string", "list", "map", "channel", "bytes", "option", "result",
    "cancel", "is_cancelled", "is_completed", "task_status",
    "upper", "lower", "sum", "abs", "sqrt", "min", "max", "range",
    "is_err", "is_ok", "first", "last", "type_of", "read_file", "write_file",
    "file_exists", "remove_file", "append_file", "create_dir", "env", "cwd",
    "has_key", "entries", "run_cmd", "repeat",
];

fn audit_program(source: &Source, prog: &Program) -> Result<(), String> {
    let approved_modules: HashSet<&str> = ALLOWED_MODULES.iter().copied().collect();
    let approved_builtins: HashSet<&str> = APPROVED_BUILTINS.iter().copied().collect();
    let allowed_effects: HashSet<&str> = ALLOWED_EFFECTS.iter().copied().collect();

    for item in &prog.items {
        match item {
            Item::Import(imp) => {
                let mod_name = imp.path.name.as_str();
                if !approved_modules.contains(mod_name) {
                    let loc = source.locate(imp.span.start);
                    return Err(format!(
                        "{}:{}: forbidden import `{mod_name}` outside stable subset",
                        source.name(), loc
                    ));
                }
            }
            Item::Fn(func) => {
                audit_fn(source, func, &allowed_effects, &approved_builtins)?;
            }
            Item::Impl(imp) => {
                for m in &imp.methods {
                    audit_fn(source, m, &allowed_effects, &approved_builtins)?;
                }
            }
            Item::Trait(t) => {
                for m in &t.methods {
                    for eff in m.effects.names() {
                        if !allowed_effects.contains(eff) {
                            let loc = source.locate(m.span.start);
                            return Err(format!(
                                "{}:{}: unapproved effect `{eff}` in trait method",
                                source.name(), loc
                            ));
                        }
                    }
                }
            }
            Item::Struct(_) | Item::Enum(_) | Item::Type(_) | Item::Const(_) => {}
        }
    }
    Ok(())
}

fn audit_fn(
    source: &Source,
    func: &FnDecl,
    allowed_effects: &HashSet<&str>,
    approved_builtins: &HashSet<&str>,
) -> Result<(), String> {
    if func.is_const {
        for eff in func.effects.names() {
            if eff != "pure" {
                let loc = source.locate(func.span.start);
                return Err(format!(
                    "{}:{}: const fn `{}` cannot declare impure effect `{}`",
                    source.name(), loc, func.name.name, eff
                ));
            }
        }
    }
    for eff in func.effects.names() {
        if !allowed_effects.contains(eff) {
            let loc = source.locate(func.span.start);
            return Err(format!(
                "{}:{}: unapproved effect `{}` on fn `{}`",
                source.name(), loc, eff, func.name.name
            ));
        }
    }
    audit_block(source, &func.body, approved_builtins)
}

fn audit_block(
    source: &Source,
    block: &Block,
    approved_builtins: &HashSet<&str>,
) -> Result<(), String> {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let(s) => {
                audit_expr(source, &s.value, approved_builtins)?;
            }
            Stmt::Var(s) => {
                audit_expr(source, &s.value, approved_builtins)?;
            }
            Stmt::Expr(e) => {
                audit_expr(source, e, approved_builtins)?;
            }
            Stmt::While { cond, body, .. } => {
                audit_expr(source, cond, approved_builtins)?;
                audit_block(source, body, approved_builtins)?;
            }
            Stmt::For { iter, body, .. } => {
                audit_expr(source, iter, approved_builtins)?;
                audit_block(source, body, approved_builtins)?;
            }
            Stmt::Loop { body, .. } => {
                audit_block(source, body, approved_builtins)?;
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    audit_expr(source, v, approved_builtins)?;
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }
    if let Some(tail) = &block.tail {
        audit_expr(source, tail, approved_builtins)?;
    }
    Ok(())
}

fn audit_expr(
    source: &Source,
    expr: &Expr,
    approved_builtins: &HashSet<&str>,
) -> Result<(), String> {
    match expr {
        Expr::Call { callee, args, span } => {
            if let Expr::Ident(id) = callee.as_ref() {
                // If it looks like a macro invocation or foreign calling convention
                if id.name.starts_with("__") || id.name.ends_with('!') {
                    let loc = source.locate(span.start);
                    return Err(format!(
                        "{}:{}: forbidden macro/internal call `{}`",
                        source.name(), loc, id.name
                    ));
                }
            }
            audit_expr(source, callee, approved_builtins)?;
            for a in args {
                audit_expr(source, a, approved_builtins)?;
            }
        }
        Expr::Unary { expr, .. } => audit_expr(source, expr, approved_builtins)?,
        Expr::Binary { left, right, .. } => {
            audit_expr(source, left, approved_builtins)?;
            audit_expr(source, right, approved_builtins)?;
        }
        Expr::If { cond, then_block, else_block, .. } => {
            audit_expr(source, cond, approved_builtins)?;
            audit_block(source, then_block, approved_builtins)?;
            if let Some(eb) = else_block {
                audit_expr(source, eb, approved_builtins)?;
            }
        }
        Expr::Block(b) => audit_block(source, b, approved_builtins)?,
        Expr::Match { scrutinee, arms, .. } => {
            audit_expr(source, scrutinee, approved_builtins)?;
            for arm in arms {
                audit_expr(source, &arm.body, approved_builtins)?;
            }
        }
        Expr::Spawn { call, .. } => audit_expr(source, call, approved_builtins)?,
        Expr::Await { task, .. } => audit_expr(source, task, approved_builtins)?,
        Expr::Try { expr, .. } => audit_expr(source, expr, approved_builtins)?,
        Expr::Field { target, .. } => audit_expr(source, target, approved_builtins)?,
        Expr::Index { target, index, .. } => {
            audit_expr(source, target, approved_builtins)?;
            audit_expr(source, index, approved_builtins)?;
        }
        Expr::Assign { value, .. } => audit_expr(source, value, approved_builtins)?,
        Expr::Nursery { body, .. } => audit_block(source, body, approved_builtins)?,
        Expr::StructInit { fields, .. } => {
            for (_, val) in fields {
                audit_expr(source, val, approved_builtins)?;
            }
        }
        Expr::List { elements, .. } => {
            for el in elements {
                audit_expr(source, el, approved_builtins)?;
            }
        }
        Expr::Map { entries, .. } => {
            for (k, v) in entries {
                audit_expr(source, k, approved_builtins)?;
                audit_expr(source, v, approved_builtins)?;
            }
        }
        Expr::Interpolated { parts, .. } => {
            for part in parts {
                if let InterpPart::Expr(e) = part {
                    audit_expr(source, e, approved_builtins)?;
                }
            }
        }
        Expr::Range { start, end, .. } => {
            audit_expr(source, start, approved_builtins)?;
            audit_expr(source, end, approved_builtins)?;
        }
        Expr::Literal { .. } | Expr::Ident(_) => {}
    }
    Ok(())
}

#[test]
fn selfhost_sources_satisfy_stable_subset_lock() {
    let selfhost_dir = repo_root().join("selfhost");
    let mut files = Vec::new();
    for entry in walkdir(selfhost_dir) {
        if entry.extension().is_some_and(|ext| ext == "flk") {
            files.push(entry);
        }
    }
    assert_eq!(files.len(), 11, "expected exactly 11 selfhost .flk files");

    for file in &files {
        let content = std::fs::read_to_string(file).expect("read file");
        let filename = file.file_name().unwrap().to_str().unwrap();
        let src = Source::new(filename, &content);
        let prog = flake_parser::parse_str(&content).expect("parse selfhost file");

        // 1. AST audit visitor
        audit_program(&src, &prog).unwrap_or_else(|err| {
            panic!("subset audit failed for {filename}: {err}");
        });

        // 2. Host compiler flake check passes
        let mut host = flake_bin();
        host.current_dir(repo_root());
        host.arg("check").arg(file);
        let host_out = host.output().expect("run flake check");
        assert!(
            host_out.status.success(),
            "host check failed on {filename}:\n{}",
            String::from_utf8_lossy(&host_out.stderr)
        );
    }

    // 3. Selfhost checker also passes on selfhost main
    let mut cmd = flake_bin();
    cmd.current_dir(repo_root());
    cmd.arg("run")
        .arg(selfhost_main())
        .arg("--")
        .arg("--walk")
        .arg("selfhost");
    let out = cmd.output().expect("run selfhost walk selfhost");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("Scanned 11 files: all parsed successfully"),
        "selfhost walk selfhost failed:\n{stdout}"
    );
}

#[test]
fn subset_lock_fails_on_forbidden_constructs() {
    // 1. Forbidden import
    let bad_import = "import host_unsafe_module\nfn main() {}";
    let src = Source::new("bad_import.flk", bad_import);
    let prog = flake_parser::parse_str(bad_import).unwrap();
    let err = audit_program(&src, &prog).unwrap_err();
    assert!(err.contains("forbidden import `host_unsafe_module`"), "{err}");

    // 2. Impure const fn
    let bad_const_fn = "const fn bad() / io { 42 }\nfn main() {}";
    let src = Source::new("bad_const_fn.flk", bad_const_fn);
    let prog = flake_parser::parse_str(bad_const_fn).unwrap();
    let err = audit_program(&src, &prog).unwrap_err();
    assert!(err.contains("cannot declare impure effect `io`"), "{err}");

    // 3. Unapproved effect
    let bad_effect = "fn work() / network {}\nfn main() {}";
    let src = Source::new("bad_effect.flk", bad_effect);
    let prog = flake_parser::parse_str(bad_effect).unwrap();
    let err = audit_program(&src, &prog).unwrap_err();
    assert!(err.contains("unapproved effect `network`"), "{err}");

    // 4. Macro-style call
    let bad_macro = "fn main() { __internal_call(42) }";
    let src = Source::new("bad_macro.flk", bad_macro);
    let prog = flake_parser::parse_str(bad_macro).unwrap();
    let err = audit_program(&src, &prog).unwrap_err();
    assert!(err.contains("forbidden macro/internal call `__internal_call`"), "{err}");
}

fn audit_example_program(source: &Source, prog: &Program) -> Result<(), String> {
    let approved_builtins: HashSet<&str> = APPROVED_BUILTINS.iter().copied().collect();
    let allowed_effects: HashSet<&str> = ALLOWED_EFFECTS.iter().copied().collect();

    for item in &prog.items {
        match item {
            Item::Import(_) => {
                // Examples use standard library and relative project imports
            }
            Item::Fn(func) => {
                audit_fn(source, func, &allowed_effects, &approved_builtins)?;
            }
            Item::Impl(imp) => {
                for m in &imp.methods {
                    audit_fn(source, m, &allowed_effects, &approved_builtins)?;
                }
            }
            Item::Trait(t) => {
                for m in &t.methods {
                    for eff in m.effects.names() {
                        if !allowed_effects.contains(eff) {
                            let loc = source.locate(m.span.start);
                            return Err(format!(
                                "{}:{}: unapproved effect `{eff}` in trait method",
                                source.name(), loc
                            ));
                        }
                    }
                }
            }
            Item::Struct(_) | Item::Enum(_) | Item::Type(_) | Item::Const(_) => {}
        }
    }
    Ok(())
}

#[test]
fn examples_sources_satisfy_stable_subset_lock() {
    let examples_dir = repo_root().join("examples");
    let mut files = Vec::new();
    for entry in walkdir(examples_dir) {
        if entry.extension().is_some_and(|ext| ext == "flk") {
            files.push(entry);
        }
    }
    assert_eq!(files.len(), 62, "expected exactly 62 example .flk files");

    for file in &files {
        let content = std::fs::read_to_string(file).expect("read file");
        let filename = file.file_name().unwrap().to_str().unwrap();
        let src = Source::new(filename, &content);
        let prog = flake_parser::parse_str(&content).expect("parse example file");

        audit_example_program(&src, &prog).unwrap_or_else(|err| {
            panic!("subset audit failed for example {filename}: {err}");
        });
    }
}


fn walkdir(dir: PathBuf) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(walkdir(path));
            } else {
                result.push(path);
            }
        }
    }
    result
}
