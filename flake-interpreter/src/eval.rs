//! Tree-walking evaluator.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

use flake_ast::{
    AssignOp, BinOp, Block, Expr, InterpPart, Item, LetStmt, Literal, Program, Source, Span, Stmt,
    UnOp,
};
use flake_parser::{ModuleGraph, ReplInput, import_alias, load_graph, parse_repl};

use crate::env::Env;
use crate::error::{RunError, RuntimeError};
use crate::value::{Function, MapKey, NativeFn, TaskRef, TaskState, Value};

const MAX_CALL_DEPTH: usize = 10_000;

thread_local! {
    static PROGRAM_ARGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Arguments forwarded to the running Flake program (`args()` builtin).
pub fn set_program_args(args: Vec<String>) {
    PROGRAM_ARGS.with(|slot| {
        *slot.borrow_mut() = args;
    });
}

fn program_args() -> Vec<String> {
    PROGRAM_ARGS.with(|slot| slot.borrow().clone())
}

enum Control {
    Return(Value),
    Break,
    Continue,
}

enum Fail {
    Runtime(RuntimeError),
    Control(Control),
}

type EvalResult<T> = Result<T, Fail>;

impl From<RuntimeError> for Fail {
    fn from(err: RuntimeError) -> Self {
        Self::Runtime(err)
    }
}

struct Interpreter<'io> {
    source: &'io Source,
    env: Env,
    stdout: &'io mut dyn Write,
    depth: usize,
    task_scopes: Vec<Vec<TaskRef>>,
}

#[derive(Clone)]
struct InstalledModule {
    value: Value,
    env: Env,
}

/// Persistent evaluation engine used by `flake run` and the REPL.
pub struct Engine {
    env: Env,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    #[must_use]
    pub fn new() -> Self {
        let env = Env::root();
        install_builtins(&env);
        Self { env }
    }

    /// Run a complete program (requires `main`).
    pub fn run(&mut self, source: &Source, stdout: &mut dyn Write) -> Result<Value, RunError> {
        let graph = load_graph(source)?;
        let mut interp = Interpreter {
            source,
            env: self.env.clone(),
            stdout,
            depth: 0,
            task_scopes: vec![Vec::new()],
        };
        interp.install_graph(&graph).map_err(RunError::from)?;
        interp
            .call_main(&graph.entry().program)
            .map_err(RunError::from)
    }

    /// Evaluate REPL input: a program fragment or a script of statements.
    pub fn eval_repl(
        &mut self,
        source: &Source,
        stdout: &mut dyn Write,
    ) -> Result<Value, RunError> {
        let input = parse_repl(source)?;
        let mut interp = Interpreter {
            source,
            env: self.env.clone(),
            stdout,
            depth: 0,
            task_scopes: vec![Vec::new()],
        };
        let result = match input {
            ReplInput::Program(program) => {
                interp.collect_items(&program).map_err(RunError::from)?;
                let has_main = program
                    .items
                    .iter()
                    .any(|item| matches!(item, Item::Fn(f) if f.name.name == "main"));
                if has_main {
                    interp.call_main(&program).map_err(RunError::from)
                } else {
                    Ok(Value::Nil)
                }
            }
            ReplInput::Script(block) => match interp.eval_open_block(&block) {
                Ok(v) => Ok(v),
                Err(Fail::Runtime(e)) => Err(e.into()),
                Err(Fail::Control(Control::Return(v))) => Ok(v),
                Err(Fail::Control(Control::Break)) => {
                    Err(RuntimeError::new(block.span, "`break` outside of a loop").into())
                }
                Err(Fail::Control(Control::Continue)) => {
                    Err(RuntimeError::new(block.span, "`continue` outside of a loop").into())
                }
            },
        };
        match result {
            Ok(value) => {
                interp.finish_root_task_scope().map_err(RunError::from)?;
                Ok(value)
            }
            Err(error) => {
                interp.cancel_task_scope();
                Err(error)
            }
        }
    }
}

/// Parse and execute a Flake program, writing `print` output to `stdout`.
pub fn execute(source: &Source, stdout: &mut dyn Write) -> Result<Value, RunError> {
    Engine::new().run(source, stdout)
}

/// Execute an already-parsed program.
pub fn execute_program(
    source: &Source,
    program: &Program,
    stdout: &mut dyn Write,
) -> Result<Value, RunError> {
    let mut interp = Interpreter {
        source,
        env: Env::root(),
        stdout,
        depth: 0,
        task_scopes: vec![Vec::new()],
    };
    install_builtins(&interp.env);
    interp.collect_items(program).map_err(RunError::from)?;
    interp.call_main(program).map_err(RunError::from)
}

fn install_builtins(env: &Env) {
    for native in [
        NativeFn::Print,
        NativeFn::Len,
        NativeFn::Push,
        NativeFn::Pop,
        NativeFn::Str,
        NativeFn::Int,
        NativeFn::Float,
        NativeFn::TypeOf,
        NativeFn::Assert,
        NativeFn::ReadFile,
        NativeFn::Abs,
        NativeFn::Min,
        NativeFn::Max,
        NativeFn::Range,
        NativeFn::Join,
        NativeFn::Split,
        NativeFn::WriteFile,
        NativeFn::Contains,
        NativeFn::StartsWith,
        NativeFn::EndsWith,
        NativeFn::First,
        NativeFn::Last,
        NativeFn::Trim,
        NativeFn::Upper,
        NativeFn::Lower,
        NativeFn::FileExists,
        NativeFn::Env,
        NativeFn::Cwd,
        NativeFn::RemoveFile,
        NativeFn::Keys,
        NativeFn::Values,
        NativeFn::Entries,
        NativeFn::IsEmpty,
        NativeFn::HasKey,
        NativeFn::Cancel,
        NativeFn::IsCancelled,
        NativeFn::IsCompleted,
        NativeFn::TaskStatus,
        NativeFn::Args,
        NativeFn::ListDir,
        NativeFn::IsDir,
        NativeFn::IsFile,
        NativeFn::AppendFile,
        NativeFn::CreateDir,
        NativeFn::RunCmd,
    ] {
        env.define(native.name(), Value::Native(native), false);
    }
}

/// Execute `source` and return both the result and captured stdout.
pub fn execute_captured(source: &Source) -> Result<(Value, String), RunError> {
    let mut buf = Vec::new();
    let value = execute(source, &mut buf)?;
    Ok((value, String::from_utf8_lossy(&buf).into_owned()))
}

impl<'io> Interpreter<'io> {
    fn install_graph(&mut self, graph: &ModuleGraph) -> Result<(), RuntimeError> {
        let prelude = self.env.clone();
        let mut done = HashMap::new();
        self.install_module(graph, graph.entry(), &prelude, &mut done)?;
        self.env = done
            .get(&graph.entry().name)
            .expect("entry module was installed")
            .env
            .clone();
        Ok(())
    }

    fn install_module(
        &mut self,
        graph: &ModuleGraph,
        module: &flake_parser::LoadedModule,
        prelude: &Env,
        done: &mut HashMap<String, InstalledModule>,
    ) -> Result<(), RuntimeError> {
        if done.contains_key(&module.name) {
            return Ok(());
        }
        let module_env = prelude.child();
        for item in &module.program.items {
            if let Item::Import(import) = item {
                let Some(imported) = graph.imported(module, import) else {
                    return Err(RuntimeError::new(
                        import.span,
                        format!("unresolved import `{}`", import.path.name),
                    ));
                };
                self.install_module(graph, imported, prelude, done)?;
                let alias = import_alias(import);
                if let Some(installed) = done.get(&imported.name) {
                    module_env.define(alias, installed.value.clone(), false);
                    if let Value::Module { members, .. } = &installed.value {
                        for (name, value) in members.iter() {
                            if graph.unqualified_import_is_unambiguous(module, name) {
                                module_env.define(name, value.clone(), false);
                            }
                        }
                    }
                    for (item, origin) in graph.exported_items(imported) {
                        if let Item::Struct(st) = item {
                            if graph.unqualified_import_is_unambiguous(module, &st.name.name) {
                                module_env.define_type(
                                    &st.name.name,
                                    flake_parser::qualify(&origin.name, &st.name.name),
                                );
                            }
                        }
                    }
                }
            }
        }
        let type_prefix = (module.name != graph.entry().name).then_some(module.name.as_str());
        Self::collect_items_in(&module_env, &module.program, type_prefix)?;
        let mut members = HashMap::new();
        for (item, _origin) in graph.exported_items(module) {
            match item {
                Item::Fn(func) => {
                    if let Some(value) = module_env.get(&func.name.name) {
                        members.insert(func.name.name.clone(), value);
                    }
                }
                Item::Enum(en) => {
                    if let Some(value) = module_env.get(&en.name.name) {
                        members.insert(en.name.name.clone(), value);
                    }
                }
                Item::Import(imp) if imp.is_pub => {
                    let alias = import_alias(imp);
                    if let Some(value) = module_env.get(alias) {
                        members.insert(alias.to_string(), value);
                    }
                }
                _ => {}
            }
        }
        let module_val = Value::Module {
            name: Rc::from(module.name.as_str()),
            members: Rc::new(members),
        };
        done.insert(
            module.name.clone(),
            InstalledModule {
                value: module_val,
                env: module_env,
            },
        );
        Ok(())
    }

    fn collect_items(&mut self, program: &Program) -> Result<(), RuntimeError> {
        Self::collect_items_in(&self.env, program, None)
    }

    fn collect_items_in(
        env: &Env,
        program: &Program,
        module_name: Option<&str>,
    ) -> Result<(), RuntimeError> {
        for item in &program.items {
            match item {
                Item::Fn(func) => {
                    let value = Value::Function(Rc::new(Function {
                        name: func.name.name.clone(),
                        params: func.params.iter().map(|p| p.name.clone()).collect(),
                        body: func.body.clone(),
                        closure: env.clone(),
                        span: func.span,
                    }));
                    env.define(&func.name.name, value, false);
                }
                Item::Struct(st) => {
                    let type_name = module_name
                        .map(|module| flake_parser::qualify(module, &st.name.name))
                        .unwrap_or_else(|| st.name.name.clone());
                    env.define_type(&st.name.name, type_name);
                }
                Item::Type(_) | Item::Import(_) | Item::Trait(_) | Item::Impl(_) => {}
                Item::Enum(en) => {
                    let type_name = module_name
                        .map(|module| flake_parser::qualify(module, &en.name.name))
                        .unwrap_or_else(|| en.name.name.clone());
                    let mut members = HashMap::new();
                    for (tag, v) in en.variants.iter().enumerate() {
                        let ctor = if v.fields.is_empty() {
                            Value::Enum {
                                type_name: Rc::from(type_name.as_str()),
                                variant: Rc::from(v.name.name.as_str()),
                                tag: tag as i64,
                                fields: Vec::new(),
                            }
                        } else {
                            Value::VariantCtor {
                                type_name: Rc::from(type_name.as_str()),
                                variant: Rc::from(v.name.name.as_str()),
                                tag: tag as i64,
                                arity: v.fields.len(),
                            }
                        };
                        members.insert(v.name.name.clone(), ctor);
                    }
                    env.define(
                        &en.name.name,
                        Value::Module {
                            name: Rc::from(en.name.name.as_str()),
                            members: Rc::new(members),
                        },
                        false,
                    );
                }
            }
        }
        Ok(())
    }

    fn call_main(&mut self, program: &Program) -> Result<Value, RuntimeError> {
        let Some(main) = program.items.iter().find_map(|item| match item {
            Item::Fn(f) if f.name.name == "main" => Some(f),
            _ => None,
        }) else {
            return Err(RuntimeError::new(
                program.span,
                "program has no `main` function",
            ));
        };
        if !main.params.is_empty() {
            return Err(RuntimeError::new(
                main.params[0].span,
                "`main` cannot take parameters",
            ));
        }
        let Some(Value::Function(func)) = self.env.get("main") else {
            return Err(RuntimeError::new(
                main.span,
                "internal error: `main` not bound",
            ));
        };
        match self.call_function(&func, &[], main.span) {
            Ok(v) => Ok(v),
            Err(Fail::Runtime(e)) => Err(e),
            Err(Fail::Control(Control::Return(v))) => Ok(v),
            Err(Fail::Control(Control::Break)) => {
                Err(RuntimeError::new(main.span, "`break` outside of a loop"))
            }
            Err(Fail::Control(Control::Continue)) => {
                Err(RuntimeError::new(main.span, "`continue` outside of a loop"))
            }
        }
    }

    fn call_function(&mut self, func: &Function, args: &[Value], span: Span) -> EvalResult<Value> {
        if args.len() != func.params.len() {
            return Err(RuntimeError::new(
                span,
                format!(
                    "function `{}` expected {} argument(s), got {}",
                    func.name,
                    func.params.len(),
                    args.len()
                ),
            )
            .into());
        }
        if self.depth >= MAX_CALL_DEPTH {
            return Err(RuntimeError::new(span, "maximum call depth exceeded").into());
        }
        self.depth += 1;
        self.task_scopes.push(Vec::new());
        let call_env = func.closure.child();
        for (param, arg) in func.params.iter().zip(args) {
            call_env.define(&param.name, arg.clone(), false);
        }
        let saved = self.env.clone();
        self.env = call_env;
        let mut result = match self.eval_block(&func.body) {
            Ok(v) => Ok(v),
            Err(Fail::Control(Control::Return(v))) => Ok(v),
            Err(other) => Err(other),
        };
        if result.is_ok() {
            if let Err(err) = self.finish_task_scope() {
                result = Err(err);
            }
        } else {
            self.cancel_task_scope();
        }
        self.env = saved;
        self.depth -= 1;
        result
    }

    fn eval_block(&mut self, block: &Block) -> EvalResult<Value> {
        let saved = self.env.clone();
        self.env = saved.child();
        let result = self.eval_open_block(block);
        self.env = saved;
        result
    }

    fn eval_open_block(&mut self, block: &Block) -> EvalResult<Value> {
        for stmt in &block.stmts {
            self.eval_stmt(stmt)?;
        }
        if let Some(tail) = &block.tail {
            self.eval_expr(tail)
        } else {
            Ok(Value::Nil)
        }
    }

    fn eval_stmt(&mut self, stmt: &Stmt) -> EvalResult<()> {
        match stmt {
            Stmt::Let(s) => self.eval_binding(s, false),
            Stmt::Var(s) => self.eval_binding(s, true),
            Stmt::Return { value, .. } => {
                let v = match value {
                    Some(e) => self.eval_expr(e)?,
                    None => Value::Nil,
                };
                Err(Fail::Control(Control::Return(v)))
            }
            Stmt::Break { .. } => Err(Fail::Control(Control::Break)),
            Stmt::Continue { .. } => Err(Fail::Control(Control::Continue)),
            Stmt::While { cond, body, span } => self.eval_while(cond, body, *span),
            Stmt::For {
                name, iter, body, ..
            } => self.eval_for(&name.name, iter, body),
            Stmt::Loop { body, .. } => self.eval_loop(body),
            Stmt::Expr(e) => {
                self.eval_expr(e)?;
                Ok(())
            }
        }
    }

    fn eval_binding(&mut self, stmt: &LetStmt, mutable: bool) -> EvalResult<()> {
        let value = self.eval_expr(&stmt.value)?;
        self.env.define(&stmt.name.name, value, mutable);
        Ok(())
    }

    fn eval_while(&mut self, cond: &Expr, body: &Block, span: Span) -> EvalResult<()> {
        loop {
            let c = self.eval_expr(cond)?;
            if !c.as_bool(span)? {
                break;
            }
            match self.eval_block(body) {
                Ok(_) => {}
                Err(Fail::Control(Control::Break)) => break,
                Err(Fail::Control(Control::Continue)) => continue,
                Err(other) => return Err(other),
            }
        }
        Ok(())
    }

    fn eval_loop(&mut self, body: &Block) -> EvalResult<()> {
        loop {
            match self.eval_block(body) {
                Ok(_) => {}
                Err(Fail::Control(Control::Break)) => break,
                Err(Fail::Control(Control::Continue)) => continue,
                Err(other) => return Err(other),
            }
        }
        Ok(())
    }

    fn eval_for(&mut self, name: &str, iter: &Expr, body: &Block) -> EvalResult<()> {
        let iterable = self.eval_expr(iter)?;
        let items = self.iterate(&iterable, iter.span())?;
        for item in items {
            let saved = self.env.clone();
            self.env = saved.child();
            self.env.define(name, item, false);
            let result = self.eval_block(body);
            self.env = saved;
            match result {
                Ok(_) => {}
                Err(Fail::Control(Control::Break)) => break,
                Err(Fail::Control(Control::Continue)) => continue,
                Err(other) => return Err(other),
            }
        }
        Ok(())
    }

    fn iterate(&self, value: &Value, span: Span) -> EvalResult<Vec<Value>> {
        match value {
            Value::List(items) => Ok(items.borrow().clone()),
            Value::Range { start, end } => {
                let mut items = Vec::new();
                if *end >= *start {
                    items.extend((*start..*end).map(Value::Int));
                } else {
                    let mut i = *start;
                    while i > *end {
                        items.push(Value::Int(i));
                        i -= 1;
                    }
                }
                Ok(items)
            }
            Value::String(s) => Ok(s
                .chars()
                .map(|c| Value::from_string(c.to_string()))
                .collect()),
            other => Err(RuntimeError::new(
                span,
                format!("cannot iterate over {}", other.type_name()),
            )
            .into()),
        }
    }

    fn eval_expr(&mut self, expr: &Expr) -> EvalResult<Value> {
        match expr {
            Expr::Literal { value, .. } => Ok(literal_value(value)),
            Expr::Ident(id) => self.env.get(&id.name).ok_or_else(|| {
                RuntimeError::new(id.span, format!("undefined variable `{}`", id.name)).into()
            }),
            Expr::Interpolated { parts, .. } => self.eval_interpolated(parts),
            Expr::List { elements, .. } => {
                let mut items = Vec::with_capacity(elements.len());
                for e in elements {
                    items.push(self.eval_expr(e)?);
                }
                Ok(Value::List(Rc::new(RefCell::new(items))))
            }
            Expr::Map { entries, .. } => {
                let mut map = HashMap::new();
                for (k, v) in entries {
                    let key = self.eval_expr(k)?;
                    let key = map_key(&key, k.span())?;
                    let val = self.eval_expr(v)?;
                    map.insert(key, val);
                }
                Ok(Value::Map(Rc::new(RefCell::new(map))))
            }
            Expr::Unary { op, expr, span } => {
                let v = self.eval_expr(expr)?;
                unary(*op, v, *span)
            }
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => {
                let l = self.eval_expr(left)?;
                if *op == BinOp::And {
                    if !l.as_bool(*span)? {
                        return Ok(Value::Bool(false));
                    }
                    let r = self.eval_expr(right)?;
                    return Ok(Value::Bool(r.as_bool(*span)?));
                }
                if *op == BinOp::Or {
                    if l.as_bool(*span)? {
                        return Ok(Value::Bool(true));
                    }
                    let r = self.eval_expr(right)?;
                    return Ok(Value::Bool(r.as_bool(*span)?));
                }
                let r = self.eval_expr(right)?;
                binary(*op, l, r, *span)
            }
            Expr::Assign {
                op,
                target,
                value,
                span,
            } => self.eval_assign(*op, target, value, *span),
            Expr::Call { callee, args, span } => {
                let func = self.eval_expr(callee)?;
                let mut arg_values = Vec::with_capacity(args.len());
                for a in args {
                    arg_values.push(self.eval_expr(a)?);
                }
                self.call_value(&func, &arg_values, *span)
            }
            Expr::Spawn { call, span } => {
                let Expr::Call {
                    callee,
                    args,
                    span: call_span,
                } = call.as_ref()
                else {
                    return Err(RuntimeError::new(*span, "`spawn` expects a function call").into());
                };
                let func = self.eval_expr(callee)?;
                let mut arg_values = Vec::with_capacity(args.len());
                for arg in args {
                    arg_values.push(self.eval_expr(arg)?);
                }
                let task = Rc::new(RefCell::new(TaskState::Pending {
                    callee: func,
                    args: arg_values,
                    span: *call_span,
                }));
                let Some(scope) = self.task_scopes.last_mut() else {
                    return Err(RuntimeError::new(*span, "`spawn` outside of a task scope").into());
                };
                scope.push(task.clone());
                Ok(Value::Task(task))
            }
            Expr::Await { task, span } => {
                let value = self.eval_expr(task)?;
                let Value::Task(task) = value else {
                    return Err(RuntimeError::new(
                        *span,
                        format!("cannot await value of type {}", value.type_name()),
                    )
                    .into());
                };
                self.join_task(&task, *span)
            }
            Expr::Try { expr, span } => {
                let value = self.eval_expr(expr)?;
                match &value {
                    Value::Enum {
                        variant, fields, ..
                    } if variant.as_ref() == "Ok" && fields.len() == 1 => Ok(fields[0].clone()),
                    Value::Enum {
                        variant, fields, ..
                    } if variant.as_ref() == "Err" && fields.len() == 1 => {
                        Err(Fail::Control(Control::Return(value)))
                    }
                    other => Err(RuntimeError::new(
                        *span,
                        format!(
                            "`?` expected Result.Ok(value) or Result.Err(error), found {}",
                            other.display_value()
                        ),
                    )
                    .into()),
                }
            }
            Expr::Index {
                target,
                index,
                span,
            } => {
                let t = self.eval_expr(target)?;
                let i = self.eval_expr(index)?;
                index_get(&t, &i, *span)
            }
            Expr::Field { target, field, .. } => {
                let t = self.eval_expr(target)?;
                field_get(&t, &field.name, field.span)
            }
            Expr::Range { start, end, .. } => {
                let s = self.eval_expr(start)?;
                let e = self.eval_expr(end)?;
                let start_i = expect_int(&s, start.span())?;
                let end_i = expect_int(&e, end.span())?;
                Ok(Value::Range {
                    start: start_i,
                    end: end_i,
                })
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                span,
            } => {
                let c = self.eval_expr(cond)?;
                if c.as_bool(*span)? {
                    self.eval_block(then_block)
                } else if let Some(els) = else_block {
                    self.eval_expr(els)
                } else {
                    Ok(Value::Nil)
                }
            }
            Expr::Block(block) => self.eval_block(block),
            Expr::Nursery { body, .. } => {
                self.task_scopes.push(Vec::new());
                let result = self.eval_block(body);
                match result {
                    Ok(val) => {
                        self.finish_task_scope()?;
                        Ok(val)
                    }
                    Err(error) => {
                        self.cancel_task_scope();
                        Err(error)
                    }
                }
            }
            Expr::Match {
                scrutinee,
                arms,
                span,
            } => {
                let value = self.eval_expr(scrutinee)?;
                for arm in arms {
                    if let Some(binds) = match_pattern(&arm.pattern, &value, &self.env) {
                        let saved = self.env.clone();
                        self.env = saved.child();
                        for (name, val) in binds {
                            self.env.define(name, val, false);
                        }
                        let result = self.eval_expr(&arm.body);
                        self.env = saved;
                        return result;
                    }
                }
                Err(RuntimeError::new(*span, "non-exhaustive match").into())
            }
            Expr::StructInit { name, fields, .. } => {
                let mut map = HashMap::new();
                for (field, value) in fields {
                    map.insert(field.name.clone(), self.eval_expr(value)?);
                }
                let type_name = if let Some((alias, item)) = name.name.split_once('.') {
                    match self.env.get(alias) {
                        Some(Value::Module { name: module, .. }) => {
                            flake_parser::qualify(&module, item)
                        }
                        _ => name.name.clone(),
                    }
                } else {
                    self.env
                        .resolve_type(&name.name)
                        .unwrap_or_else(|| name.name.clone())
                };
                Ok(Value::Struct {
                    name: Rc::from(type_name),
                    fields: Rc::new(RefCell::new(map)),
                })
            }
        }
    }

    fn eval_interpolated(&mut self, parts: &[InterpPart]) -> EvalResult<Value> {
        let mut out = String::new();
        for part in parts {
            match part {
                InterpPart::Text(t) => out.push_str(t),
                InterpPart::Expr(e) => {
                    let v = self.eval_expr(e)?;
                    out.push_str(&v.display_value());
                }
            }
        }
        Ok(Value::from_string(out))
    }

    fn eval_assign(
        &mut self,
        op: AssignOp,
        target: &Expr,
        value: &Expr,
        span: Span,
    ) -> EvalResult<Value> {
        let rhs = self.eval_expr(value)?;
        match target {
            Expr::Ident(id) => {
                let next = if op == AssignOp::Assign {
                    rhs
                } else {
                    let current = self.env.get(&id.name).ok_or_else(|| {
                        RuntimeError::new(id.span, format!("undefined variable `{}`", id.name))
                    })?;
                    compound(op, current, rhs, span)?
                };
                self.env.assign(&id.name, next.clone(), span)?;
                Ok(next)
            }
            Expr::Index {
                target: container,
                index,
                span: ispan,
            } => {
                let t = self.eval_expr(container)?;
                let i = self.eval_expr(index)?;
                let next = if op == AssignOp::Assign {
                    rhs
                } else {
                    let current = index_get(&t, &i, *ispan)?;
                    compound(op, current, rhs, span)?
                };
                index_set(&t, &i, next.clone(), *ispan)?;
                Ok(next)
            }
            Expr::Field {
                target: container,
                field,
                ..
            } => {
                let t = self.eval_expr(container)?;
                let next = if op == AssignOp::Assign {
                    rhs
                } else {
                    let current = field_get(&t, &field.name, field.span)?;
                    compound(op, current, rhs, span)?
                };
                field_set(&t, &field.name, next.clone(), field.span)?;
                Ok(next)
            }
            _ => Err(RuntimeError::new(span, "invalid assignment target").into()),
        }
    }

    fn call_value(&mut self, func: &Value, args: &[Value], span: Span) -> EvalResult<Value> {
        match func {
            Value::Function(f) => self.call_function(f, args, span),
            Value::Native(n) => self.call_native(*n, args, span),
            Value::VariantCtor {
                type_name,
                variant,
                tag,
                arity,
            } => {
                if args.len() != *arity {
                    return Err(RuntimeError::new(
                        span,
                        format!(
                            "variant `{type_name}.{variant}` expected {arity} argument(s), got {}",
                            args.len()
                        ),
                    )
                    .into());
                }
                Ok(Value::Enum {
                    type_name: type_name.clone(),
                    variant: variant.clone(),
                    tag: *tag,
                    fields: args.to_vec(),
                })
            }
            other => Err(RuntimeError::new(
                span,
                format!("cannot call value of type {}", other.type_name()),
            )
            .into()),
        }
    }

    fn join_task(&mut self, task: &TaskRef, await_span: Span) -> EvalResult<Value> {
        let (callee, args, call_span) = {
            let mut state = task.borrow_mut();
            match &*state {
                TaskState::Pending { .. } => {}
                TaskState::Completed(_) => {
                    let TaskState::Completed(val) =
                        std::mem::replace(&mut *state, TaskState::Joined)
                    else {
                        unreachable!()
                    };
                    return Ok(val);
                }
                TaskState::Running => {
                    return Err(RuntimeError::new(await_span, "task is already running").into());
                }
                TaskState::Joined => {
                    return Err(RuntimeError::new(await_span, "task was already awaited").into());
                }
                TaskState::Cancelled => {
                    return Err(RuntimeError::new(await_span, "task was cancelled").into());
                }
            }
            let TaskState::Pending { callee, args, span } =
                std::mem::replace(&mut *state, TaskState::Running)
            else {
                unreachable!("pending task changed while borrowed")
            };
            (callee, args, span)
        };
        let result = self.call_value(&callee, &args, call_span);
        *task.borrow_mut() = TaskState::Joined;
        result
    }

    fn finish_task_scope(&mut self) -> EvalResult<()> {
        let tasks = self.task_scopes.pop().unwrap_or_default();
        for (index, task) in tasks.iter().enumerate() {
            let pending = matches!(&*task.borrow(), TaskState::Pending { .. });
            if pending {
                if let Err(error) = self.join_task(task, Span::DUMMY) {
                    for remaining in &tasks[index + 1..] {
                        Self::cancel_task(remaining);
                    }
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    fn cancel_task_scope(&mut self) {
        for task in self.task_scopes.pop().unwrap_or_default() {
            Self::cancel_task(&task);
        }
    }

    fn cancel_task(task: &TaskRef) {
        let mut state = task.borrow_mut();
        if matches!(
            &*state,
            TaskState::Pending { .. } | TaskState::Running | TaskState::Completed(_)
        ) {
            *state = TaskState::Cancelled;
        }
    }

    fn finish_root_task_scope(&mut self) -> Result<(), RuntimeError> {
        match self.finish_task_scope() {
            Ok(()) => Ok(()),
            Err(Fail::Runtime(error)) => Err(error),
            Err(Fail::Control(_)) => Err(RuntimeError::new(
                Span::DUMMY,
                "invalid control flow while joining a task",
            )),
        }
    }

    fn call_native(&mut self, native: NativeFn, args: &[Value], span: Span) -> EvalResult<Value> {
        match native {
            NativeFn::Print => {
                let text: Vec<_> = args.iter().map(Value::display_value).collect();
                writeln!(self.stdout, "{}", text.join(" "))
                    .map_err(|e| RuntimeError::new(span, format!("failed to write output: {e}")))?;
                Ok(Value::Nil)
            }
            NativeFn::Len => {
                expect_arity("len", args, 1, span)?;
                match &args[0] {
                    Value::List(v) => Ok(Value::Int(v.borrow().len() as i64)),
                    Value::String(s) => Ok(Value::Int(s.chars().count() as i64)),
                    Value::Map(m) => Ok(Value::Int(m.borrow().len() as i64)),
                    other => Err(RuntimeError::new(
                        span,
                        format!(
                            "len() expected List, String, or Map, found {}",
                            other.type_name()
                        ),
                    )
                    .into()),
                }
            }
            NativeFn::Push => {
                expect_arity("push", args, 2, span)?;
                match &args[0] {
                    Value::List(v) => {
                        v.borrow_mut().push(args[1].clone());
                        Ok(Value::Nil)
                    }
                    other => Err(RuntimeError::new(
                        span,
                        format!("push() expected List, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::Pop => {
                expect_arity("pop", args, 1, span)?;
                match &args[0] {
                    Value::List(v) => Ok(v.borrow_mut().pop().unwrap_or(Value::Nil)),
                    other => Err(RuntimeError::new(
                        span,
                        format!("pop() expected List, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::Str => {
                expect_arity("str", args, 1, span)?;
                Ok(Value::from_string(args[0].display_value()))
            }
            NativeFn::Int => {
                expect_arity("int", args, 1, span)?;
                to_int(&args[0], span)
            }
            NativeFn::Float => {
                expect_arity("float", args, 1, span)?;
                to_float(&args[0], span)
            }
            NativeFn::TypeOf => {
                expect_arity("type_of", args, 1, span)?;
                Ok(Value::from_string(args[0].type_name()))
            }
            NativeFn::Assert => {
                if args.is_empty() || args.len() > 2 {
                    return Err(
                        RuntimeError::new(span, "assert() expected 1 or 2 arguments").into(),
                    );
                }
                if !args[0].as_bool(span)? {
                    let msg = if args.len() == 2 {
                        args[1].display_value()
                    } else {
                        "assertion failed".into()
                    };
                    return Err(RuntimeError::new(span, msg).into());
                }
                Ok(Value::Nil)
            }
            NativeFn::ReadFile => {
                expect_arity("read_file", args, 1, span)?;
                let path = match &args[0] {
                    Value::String(s) => s.to_string(),
                    other => {
                        return Err(RuntimeError::new(
                            span,
                            format!("read_file() expected String, found {}", other.type_name()),
                        )
                        .into());
                    }
                };
                match std::fs::read_to_string(&path) {
                    Ok(text) => Ok(Value::from_string(text)),
                    Err(e) => {
                        Err(RuntimeError::new(span, format!("failed to read `{path}`: {e}")).into())
                    }
                }
            }
            NativeFn::Abs => {
                expect_arity("abs", args, 1, span)?;
                match &args[0] {
                    Value::Int(n) => n
                        .checked_abs()
                        .map(Value::Int)
                        .ok_or_else(|| RuntimeError::new(span, "integer overflow").into()),
                    Value::Float(n) => Ok(Value::Float(n.abs())),
                    other => Err(RuntimeError::new(
                        span,
                        format!("abs() expected Int or Float, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::Min | NativeFn::Max => {
                if args.len() < 2 {
                    return Err(RuntimeError::new(
                        span,
                        format!("{}() expected at least 2 arguments", native.name()),
                    )
                    .into());
                }
                let mut acc = args[0].clone();
                for arg in &args[1..] {
                    let lt = match cmp(acc.clone(), arg.clone(), span, |o| o.is_lt())? {
                        Value::Bool(b) => b,
                        _ => false,
                    };
                    match native {
                        NativeFn::Min if !lt => acc = arg.clone(),
                        NativeFn::Max if lt => acc = arg.clone(),
                        _ => {}
                    }
                }
                Ok(acc)
            }
            NativeFn::Range => {
                let (start, end) = match args.len() {
                    1 => (0, expect_int(&args[0], span)?),
                    2 => (expect_int(&args[0], span)?, expect_int(&args[1], span)?),
                    _ => {
                        return Err(
                            RuntimeError::new(span, "range() expected 1 or 2 arguments").into()
                        );
                    }
                };
                Ok(Value::Range { start, end })
            }
            NativeFn::Join => {
                expect_arity("join", args, 2, span)?;
                let sep = match &args[1] {
                    Value::String(s) => s.to_string(),
                    other => {
                        return Err(RuntimeError::new(
                            span,
                            format!(
                                "join() separator expected String, found {}",
                                other.type_name()
                            ),
                        )
                        .into());
                    }
                };
                match &args[0] {
                    Value::List(items) => {
                        let parts: Vec<_> =
                            items.borrow().iter().map(Value::display_value).collect();
                        Ok(Value::from_string(parts.join(&sep)))
                    }
                    other => Err(RuntimeError::new(
                        span,
                        format!("join() expected List, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::Split => {
                expect_arity("split", args, 2, span)?;
                let s = match &args[0] {
                    Value::String(s) => s.to_string(),
                    other => {
                        return Err(RuntimeError::new(
                            span,
                            format!("split() expected String, found {}", other.type_name()),
                        )
                        .into());
                    }
                };
                let sep = match &args[1] {
                    Value::String(s) => s.to_string(),
                    other => {
                        return Err(RuntimeError::new(
                            span,
                            format!(
                                "split() separator expected String, found {}",
                                other.type_name()
                            ),
                        )
                        .into());
                    }
                };
                let parts: Vec<_> = if sep.is_empty() {
                    s.chars()
                        .map(|c| Value::from_string(c.to_string()))
                        .collect()
                } else {
                    s.split(&sep).map(Value::from_string).collect()
                };
                Ok(Value::List(Rc::new(RefCell::new(parts))))
            }
            NativeFn::WriteFile => {
                expect_arity("write_file", args, 2, span)?;
                let path = match &args[0] {
                    Value::String(s) => s.to_string(),
                    other => {
                        return Err(RuntimeError::new(
                            span,
                            format!(
                                "write_file() expected String path, found {}",
                                other.type_name()
                            ),
                        )
                        .into());
                    }
                };
                let text = args[1].display_value();
                std::fs::write(&path, text).map_err(|e| {
                    RuntimeError::new(span, format!("failed to write `{path}`: {e}"))
                })?;
                Ok(Value::Nil)
            }
            NativeFn::Contains => {
                expect_arity("contains", args, 2, span)?;
                match &args[0] {
                    Value::String(s) => {
                        let needle = args[1].display_value();
                        Ok(Value::Bool(s.contains(&needle)))
                    }
                    Value::List(items) => Ok(Value::Bool(
                        items.borrow().iter().any(|v| v.equals(&args[1])),
                    )),
                    Value::Map(map) => {
                        let key = map_key(&args[1], span)?;
                        Ok(Value::Bool(map.borrow().contains_key(&key)))
                    }
                    Value::Range { start, end } => {
                        let n = expect_int(&args[1], span)?;
                        if *start <= *end {
                            Ok(Value::Bool(n >= *start && n < *end))
                        } else {
                            Ok(Value::Bool(n <= *start && n > *end))
                        }
                    }
                    other => Err(RuntimeError::new(
                        span,
                        format!(
                            "contains() expected String, List, Map, or Range, found {}",
                            other.type_name()
                        ),
                    )
                    .into()),
                }
            }
            NativeFn::StartsWith => {
                expect_arity("starts_with", args, 2, span)?;
                match (&args[0], &args[1]) {
                    (Value::String(s), Value::String(p)) => {
                        Ok(Value::Bool(s.starts_with(p.as_ref())))
                    }
                    _ => Err(RuntimeError::new(span, "starts_with() expected two Strings").into()),
                }
            }
            NativeFn::EndsWith => {
                expect_arity("ends_with", args, 2, span)?;
                match (&args[0], &args[1]) {
                    (Value::String(s), Value::String(p)) => {
                        Ok(Value::Bool(s.ends_with(p.as_ref())))
                    }
                    _ => Err(RuntimeError::new(span, "ends_with() expected two Strings").into()),
                }
            }
            NativeFn::First => {
                expect_arity("first", args, 1, span)?;
                match &args[0] {
                    Value::List(v) => Ok(v.borrow().first().cloned().unwrap_or(Value::Nil)),
                    Value::String(s) => Ok(s
                        .chars()
                        .next()
                        .map(|c| Value::from_string(c.to_string()))
                        .unwrap_or(Value::Nil)),
                    other => Err(RuntimeError::new(
                        span,
                        format!(
                            "first() expected List or String, found {}",
                            other.type_name()
                        ),
                    )
                    .into()),
                }
            }
            NativeFn::Last => {
                expect_arity("last", args, 1, span)?;
                match &args[0] {
                    Value::List(v) => Ok(v.borrow().last().cloned().unwrap_or(Value::Nil)),
                    Value::String(s) => Ok(s
                        .chars()
                        .last()
                        .map(|c| Value::from_string(c.to_string()))
                        .unwrap_or(Value::Nil)),
                    other => Err(RuntimeError::new(
                        span,
                        format!(
                            "last() expected List or String, found {}",
                            other.type_name()
                        ),
                    )
                    .into()),
                }
            }
            NativeFn::Trim => {
                expect_arity("trim", args, 1, span)?;
                match &args[0] {
                    Value::String(s) => Ok(Value::from_string(s.trim().to_string())),
                    other => Err(RuntimeError::new(
                        span,
                        format!("trim() expected String, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::Upper => {
                expect_arity("upper", args, 1, span)?;
                match &args[0] {
                    Value::String(s) => Ok(Value::from_string(s.to_uppercase())),
                    other => Err(RuntimeError::new(
                        span,
                        format!("upper() expected String, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::Lower => {
                expect_arity("lower", args, 1, span)?;
                match &args[0] {
                    Value::String(s) => Ok(Value::from_string(s.to_lowercase())),
                    other => Err(RuntimeError::new(
                        span,
                        format!("lower() expected String, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::FileExists => {
                expect_arity("file_exists", args, 1, span)?;
                let path = match &args[0] {
                    Value::String(s) => s.to_string(),
                    other => {
                        return Err(RuntimeError::new(
                            span,
                            format!("file_exists() expected String, found {}", other.type_name()),
                        )
                        .into());
                    }
                };
                Ok(Value::Bool(std::path::Path::new(&path).exists()))
            }
            NativeFn::Env => {
                expect_arity("env", args, 1, span)?;
                let name = match &args[0] {
                    Value::String(s) => s.to_string(),
                    other => {
                        return Err(RuntimeError::new(
                            span,
                            format!("env() expected String, found {}", other.type_name()),
                        )
                        .into());
                    }
                };
                Ok(Value::from_string(std::env::var(name).unwrap_or_default()))
            }
            NativeFn::Cwd => {
                expect_arity("cwd", args, 0, span)?;
                let dir = std::env::current_dir()
                    .map_err(|e| RuntimeError::new(span, format!("cwd() failed: {e}")))?;
                Ok(Value::from_string(dir.to_string_lossy().replace('\\', "/")))
            }
            NativeFn::RemoveFile => {
                expect_arity("remove_file", args, 1, span)?;
                let path = match &args[0] {
                    Value::String(s) => s.to_string(),
                    other => {
                        return Err(RuntimeError::new(
                            span,
                            format!("remove_file() expected String, found {}", other.type_name()),
                        )
                        .into());
                    }
                };
                let _ = std::fs::remove_file(&path);
                Ok(Value::Nil)
            }
            NativeFn::Keys => {
                expect_arity("keys", args, 1, span)?;
                match &args[0] {
                    Value::Map(map) => {
                        let mut sorted_keys: Vec<_> = map.borrow().keys().cloned().collect();
                        sorted_keys.sort();
                        let keys: Vec<_> = sorted_keys.into_iter().map(|k| k.to_value()).collect();
                        Ok(Value::List(Rc::new(RefCell::new(keys))))
                    }
                    other => Err(RuntimeError::new(
                        span,
                        format!("keys() expected Map, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::Values => {
                expect_arity("values", args, 1, span)?;
                match &args[0] {
                    Value::Map(map) => {
                        let mut entries: Vec<_> = map
                            .borrow()
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        entries.sort_by(|a, b| a.0.cmp(&b.0));
                        let values: Vec<_> = entries.into_iter().map(|(_, v)| v).collect();
                        Ok(Value::List(Rc::new(RefCell::new(values))))
                    }
                    other => Err(RuntimeError::new(
                        span,
                        format!("values() expected Map, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::Entries => {
                expect_arity("entries", args, 1, span)?;
                match &args[0] {
                    Value::Map(map) => {
                        let mut entries: Vec<_> = map
                            .borrow()
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        entries.sort_by(|a, b| a.0.cmp(&b.0));
                        let pairs: Vec<_> = entries
                            .into_iter()
                            .map(|(k, v)| Value::List(Rc::new(RefCell::new(vec![k.to_value(), v]))))
                            .collect();
                        Ok(Value::List(Rc::new(RefCell::new(pairs))))
                    }
                    other => Err(RuntimeError::new(
                        span,
                        format!("entries() expected Map, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::IsEmpty => {
                expect_arity("is_empty", args, 1, span)?;
                match &args[0] {
                    Value::List(l) => Ok(Value::Bool(l.borrow().is_empty())),
                    Value::String(s) => Ok(Value::Bool(s.is_empty())),
                    Value::Map(m) => Ok(Value::Bool(m.borrow().is_empty())),
                    other => Err(RuntimeError::new(
                        span,
                        format!(
                            "is_empty() expected List, String, or Map, found {}",
                            other.type_name()
                        ),
                    )
                    .into()),
                }
            }
            NativeFn::HasKey => {
                expect_arity("has_key", args, 2, span)?;
                match &args[0] {
                    Value::Map(map) => {
                        let key = map_key(&args[1], span)?;
                        Ok(Value::Bool(map.borrow().contains_key(&key)))
                    }
                    other => Err(RuntimeError::new(
                        span,
                        format!("has_key() expected Map, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::Cancel => {
                expect_arity("cancel", args, 1, span)?;
                match &args[0] {
                    Value::Task(task) => {
                        Self::cancel_task(task);
                        Ok(Value::Nil)
                    }
                    other => Err(RuntimeError::new(
                        span,
                        format!("cancel() expected Task, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::IsCancelled => {
                expect_arity("is_cancelled", args, 1, span)?;
                match &args[0] {
                    Value::Task(task) => {
                        let cancelled = matches!(&*task.borrow(), TaskState::Cancelled);
                        Ok(Value::Bool(cancelled))
                    }
                    other => Err(RuntimeError::new(
                        span,
                        format!("is_cancelled() expected Task, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::IsCompleted => {
                expect_arity("is_completed", args, 1, span)?;
                match &args[0] {
                    Value::Task(task) => {
                        let completed =
                            matches!(&*task.borrow(), TaskState::Completed(_) | TaskState::Joined);
                        Ok(Value::Bool(completed))
                    }
                    other => Err(RuntimeError::new(
                        span,
                        format!("is_completed() expected Task, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::TaskStatus => {
                expect_arity("task_status", args, 1, span)?;
                match &args[0] {
                    Value::Task(task) => {
                        let status = match &*task.borrow() {
                            TaskState::Pending { .. } => "pending",
                            TaskState::Running => "running",
                            TaskState::Completed(_) => "completed",
                            TaskState::Joined => "joined",
                            TaskState::Cancelled => "cancelled",
                        };
                        Ok(Value::from_string(status))
                    }
                    other => Err(RuntimeError::new(
                        span,
                        format!("task_status() expected Task, found {}", other.type_name()),
                    )
                    .into()),
                }
            }
            NativeFn::Args => {
                expect_arity("args", args, 0, span)?;
                let values: Vec<_> = program_args()
                    .into_iter()
                    .map(Value::from_string)
                    .collect();
                Ok(Value::List(Rc::new(RefCell::new(values))))
            }
            NativeFn::ListDir => {
                expect_arity("list_dir", args, 1, span)?;
                let path = expect_string("list_dir", &args[0], span)?;
                match sys_list_dir(&path) {
                    Ok(names) => {
                        let values: Vec<_> = names.into_iter().map(Value::from_string).collect();
                        Ok(Value::List(Rc::new(RefCell::new(values))))
                    }
                    Err(e) => Err(RuntimeError::new(span, e).into()),
                }
            }
            NativeFn::IsDir => {
                expect_arity("is_dir", args, 1, span)?;
                let path = expect_string("is_dir", &args[0], span)?;
                Ok(Value::Bool(std::path::Path::new(&path).is_dir()))
            }
            NativeFn::IsFile => {
                expect_arity("is_file", args, 1, span)?;
                let path = expect_string("is_file", &args[0], span)?;
                Ok(Value::Bool(std::path::Path::new(&path).is_file()))
            }
            NativeFn::AppendFile => {
                expect_arity("append_file", args, 2, span)?;
                let path = expect_string("append_file", &args[0], span)?;
                let contents = args[1].display_value();
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .and_then(|mut f| f.write_all(contents.as_bytes()))
                    .map_err(|e| RuntimeError::new(span, format!("append_file() failed: {e}")))?;
                Ok(Value::Nil)
            }
            NativeFn::CreateDir => {
                expect_arity("create_dir", args, 1, span)?;
                let path = expect_string("create_dir", &args[0], span)?;
                Ok(Value::Bool(std::fs::create_dir(&path).is_ok()))
            }
            NativeFn::RunCmd => {
                expect_arity("run_cmd", args, 1, span)?;
                let cmd = expect_string("run_cmd", &args[0], span)?;
                Ok(sys_run_cmd(&cmd))
            }
        }
    }
}

fn expect_string(name: &str, value: &Value, span: Span) -> EvalResult<String> {
    match value {
        Value::String(s) => Ok(s.to_string()),
        other => Err(RuntimeError::new(
            span,
            format!("{name}() expected String, found {}", other.type_name()),
        )
        .into()),
    }
}

fn sys_list_dir(path: &str) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    let entries = std::fs::read_dir(path).map_err(|e| format!("list_dir() failed: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("list_dir() failed: {e}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name != "." && name != ".." {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

fn sys_run_cmd(cmd: &str) -> Value {
    let output = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", cmd])
            .output()
    } else {
        std::process::Command::new("sh")
            .args(["-c", cmd])
            .output()
    };
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).replace('\r', "");
            let stderr = String::from_utf8_lossy(&out.stderr).replace('\r', "");
            let code = i64::from(out.status.code().unwrap_or(-1));
            Value::List(Rc::new(RefCell::new(vec![
                Value::from_string(stdout),
                Value::from_string(stderr),
                Value::Int(code),
            ])))
        }
        Err(_) => Value::List(Rc::new(RefCell::new(Vec::new()))),
    }
}

fn literal_value(lit: &Literal) -> Value {
    match lit {
        Literal::Nil => Value::Nil,
        Literal::Bool(b) => Value::Bool(*b),
        Literal::Int(n) => Value::Int(*n),
        Literal::Float(n) => Value::Float(*n),
        Literal::String(s) => Value::from_string(s.clone()),
    }
}

fn unary(op: UnOp, value: Value, span: Span) -> EvalResult<Value> {
    match op {
        UnOp::Neg => match value {
            Value::Int(n) => n
                .checked_neg()
                .map(Value::Int)
                .ok_or_else(|| RuntimeError::new(span, "integer overflow").into()),
            Value::Float(n) => Ok(Value::Float(-n)),
            other => {
                Err(RuntimeError::new(span, format!("cannot negate {}", other.type_name())).into())
            }
        },
        UnOp::Not => Ok(Value::Bool(!value.as_bool(span)?)),
        UnOp::Ref | UnOp::RefMut => Ok(value),
    }
}

fn binary(op: BinOp, left: Value, right: Value, span: Span) -> EvalResult<Value> {
    match op {
        BinOp::Add => add(left, right, span),
        BinOp::Sub => arith(left, right, span, |a, b| a.checked_sub(b), |a, b| a - b),
        BinOp::Mul => arith(left, right, span, |a, b| a.checked_mul(b), |a, b| a * b),
        BinOp::Div => div(left, right, span),
        BinOp::Rem => rem(left, right, span),
        BinOp::Eq => Ok(Value::Bool(left.equals(&right))),
        BinOp::Ne => Ok(Value::Bool(!left.equals(&right))),
        BinOp::Lt => cmp(left, right, span, |o| o.is_lt()),
        BinOp::Le => cmp(left, right, span, |o| o.is_le()),
        BinOp::Gt => cmp(left, right, span, |o| o.is_gt()),
        BinOp::Ge => cmp(left, right, span, |o| o.is_ge()),
        BinOp::And | BinOp::Or => unreachable!("short-circuit ops handled in eval_expr"),
    }
}

fn add(left: Value, right: Value, span: Span) -> EvalResult<Value> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => a
            .checked_add(b)
            .map(Value::Int)
            .ok_or_else(|| RuntimeError::new(span, "integer overflow").into()),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 + b)),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + b as f64)),
        (Value::String(a), Value::String(b)) => Ok(Value::from_string(format!("{a}{b}"))),
        (Value::String(a), b) => Ok(Value::from_string(format!("{a}{}", b.display_value()))),
        (a, Value::String(b)) => Ok(Value::from_string(format!("{}{b}", a.display_value()))),
        (Value::List(a), Value::List(b)) => {
            let mut out = a.borrow().clone();
            out.extend(b.borrow().iter().cloned());
            Ok(Value::List(Rc::new(RefCell::new(out))))
        }
        (l, r) => Err(RuntimeError::new(
            span,
            format!("cannot add {} and {}", l.type_name(), r.type_name()),
        )
        .into()),
    }
}

fn arith(
    left: Value,
    right: Value,
    span: Span,
    ints: fn(i64, i64) -> Option<i64>,
    floats: fn(f64, f64) -> f64,
) -> EvalResult<Value> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => ints(a, b)
            .map(Value::Int)
            .ok_or_else(|| RuntimeError::new(span, "integer overflow").into()),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(floats(a, b))),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(floats(a as f64, b))),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(floats(a, b as f64))),
        (l, r) => Err(RuntimeError::new(
            span,
            format!(
                "cannot apply arithmetic to {} and {}",
                l.type_name(),
                r.type_name()
            ),
        )
        .into()),
    }
}

fn div(left: Value, right: Value, span: Span) -> EvalResult<Value> {
    match (left, right) {
        (Value::Int(_), Value::Int(0)) => Err(RuntimeError::new(span, "division by zero").into()),
        (Value::Int(a), Value::Int(b)) => a
            .checked_div(b)
            .map(Value::Int)
            .ok_or_else(|| RuntimeError::new(span, "integer overflow").into()),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 / b)),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a / b as f64)),
        (l, r) => Err(RuntimeError::new(
            span,
            format!("cannot divide {} by {}", l.type_name(), r.type_name()),
        )
        .into()),
    }
}

fn rem(left: Value, right: Value, span: Span) -> EvalResult<Value> {
    match (left, right) {
        (Value::Int(_), Value::Int(0)) => Err(RuntimeError::new(span, "division by zero").into()),
        (Value::Int(a), Value::Int(b)) => a
            .checked_rem(b)
            .map(Value::Int)
            .ok_or_else(|| RuntimeError::new(span, "integer overflow").into()),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a % b)),
        (l, r) => Err(RuntimeError::new(
            span,
            format!(
                "cannot compute remainder of {} and {}",
                l.type_name(),
                r.type_name()
            ),
        )
        .into()),
    }
}

fn cmp(
    left: Value,
    right: Value,
    span: Span,
    pred: fn(std::cmp::Ordering) -> bool,
) -> EvalResult<Value> {
    let ord = match (&left, &right) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => a
            .partial_cmp(b)
            .ok_or_else(|| RuntimeError::new(span, "cannot compare NaN"))?,
        (Value::Int(a), Value::Float(b)) => (*a as f64)
            .partial_cmp(b)
            .ok_or_else(|| RuntimeError::new(span, "cannot compare NaN"))?,
        (Value::Float(a), Value::Int(b)) => a
            .partial_cmp(&(*b as f64))
            .ok_or_else(|| RuntimeError::new(span, "cannot compare NaN"))?,
        (Value::String(a), Value::String(b)) => a.cmp(b),
        (l, r) => {
            return Err(RuntimeError::new(
                span,
                format!("cannot compare {} and {}", l.type_name(), r.type_name()),
            )
            .into());
        }
    };
    Ok(Value::Bool(pred(ord)))
}

fn compound(op: AssignOp, left: Value, right: Value, span: Span) -> EvalResult<Value> {
    match op {
        AssignOp::Assign => Ok(right),
        AssignOp::AddAssign => add(left, right, span),
        AssignOp::SubAssign => arith(left, right, span, |a, b| a.checked_sub(b), |a, b| a - b),
        AssignOp::MulAssign => arith(left, right, span, |a, b| a.checked_mul(b), |a, b| a * b),
        AssignOp::DivAssign => div(left, right, span),
        AssignOp::RemAssign => rem(left, right, span),
    }
}

fn index_get(target: &Value, index: &Value, span: Span) -> EvalResult<Value> {
    match target {
        Value::List(items) => {
            let i = expect_int(index, span)?;
            let items = items.borrow();
            let idx = normalize_index(i, items.len(), span)?;
            Ok(items[idx].clone())
        }
        Value::String(s) => {
            let i = expect_int(index, span)?;
            let chars: Vec<char> = s.chars().collect();
            let idx = normalize_index(i, chars.len(), span)?;
            Ok(Value::from_string(chars[idx].to_string()))
        }
        Value::Map(map) => {
            let key = map_key(index, span)?;
            map.borrow().get(&key).cloned().ok_or_else(|| {
                RuntimeError::new(span, format!("map has no key {}", key.repr())).into()
            })
        }
        other => Err(RuntimeError::new(span, format!("cannot index {}", other.type_name())).into()),
    }
}

fn index_set(target: &Value, index: &Value, value: Value, span: Span) -> EvalResult<()> {
    match target {
        Value::List(items) => {
            let i = expect_int(index, span)?;
            let mut items = items.borrow_mut();
            let idx = normalize_index(i, items.len(), span)?;
            items[idx] = value;
            Ok(())
        }
        Value::Map(map) => {
            let key = map_key(index, span)?;
            map.borrow_mut().insert(key, value);
            Ok(())
        }
        other => Err(
            RuntimeError::new(span, format!("cannot index-assign {}", other.type_name())).into(),
        ),
    }
}

fn match_pattern(
    pat: &flake_ast::Pattern,
    value: &Value,
    env: &Env,
) -> Option<Vec<(String, Value)>> {
    match pat {
        flake_ast::Pattern::Wildcard { .. } => Some(Vec::new()),
        flake_ast::Pattern::Literal { value: literal, .. } => {
            literal_value(literal).equals(value).then(Vec::new)
        }
        flake_ast::Pattern::Ident(id) => {
            if let Value::Enum {
                variant, fields, ..
            } = value
            {
                if id.name.chars().next().is_some_and(|c| c.is_uppercase())
                    && variant.as_ref() == id.name
                    && fields.is_empty()
                {
                    return Some(Vec::new());
                }
            }
            Some(vec![(id.name.clone(), value.clone())])
        }
        flake_ast::Pattern::List { patterns, .. } => match value {
            Value::List(items) => {
                let items = items.borrow();
                if items.len() != patterns.len() {
                    return None;
                }
                let mut all_binds = Vec::new();
                for (sub_pat, item) in patterns.iter().zip(items.iter()) {
                    let binds = match_pattern(sub_pat, item, env)?;
                    all_binds.extend(binds);
                }
                Some(all_binds)
            }
            _ => None,
        },
        flake_ast::Pattern::Variant {
            ty,
            variant,
            fields: sub_pats,
            ..
        } => match value {
            Value::Enum {
                type_name,
                variant: vname,
                fields,
                ..
            } => {
                if let Some(t) = ty {
                    let matches_type = if let Some((alias, name)) = t.name.split_once('.') {
                        match env.get(alias) {
                            Some(Value::Module { name: module, .. }) => {
                                flake_parser::qualify(&module, name) == type_name.as_ref()
                            }
                            _ => false,
                        }
                    } else {
                        type_name
                            .rsplit('.')
                            .next()
                            .is_some_and(|name| name == t.name)
                    };
                    if !matches_type {
                        return None;
                    }
                }
                if vname.as_ref() != variant.name {
                    return None;
                }
                if sub_pats.len() != fields.len() {
                    return None;
                }
                let mut all_binds = Vec::new();
                for (sub_pat, field_val) in sub_pats.iter().zip(fields.iter()) {
                    let binds = match_pattern(sub_pat, field_val, env)?;
                    all_binds.extend(binds);
                }
                Some(all_binds)
            }
            _ => None,
        },
    }
}

fn field_get(target: &Value, name: &str, span: Span) -> EvalResult<Value> {
    match target {
        Value::Struct { fields, .. } => fields
            .borrow()
            .get(name)
            .cloned()
            .ok_or_else(|| RuntimeError::new(span, format!("no field `{name}`")).into()),
        Value::Module {
            name: module,
            members,
        } => members.get(name).cloned().ok_or_else(|| {
            RuntimeError::new(span, format!("module `{module}` has no export `{name}`")).into()
        }),
        other => Err(RuntimeError::new(
            span,
            format!("cannot access field `{name}` on {}", other.type_name()),
        )
        .into()),
    }
}

fn field_set(target: &Value, name: &str, value: Value, span: Span) -> EvalResult<()> {
    match target {
        Value::Struct { fields, .. } => {
            fields.borrow_mut().insert(name.to_string(), value);
            Ok(())
        }
        other => Err(RuntimeError::new(
            span,
            format!("cannot assign field `{name}` on {}", other.type_name()),
        )
        .into()),
    }
}

fn map_key(value: &Value, span: Span) -> EvalResult<MapKey> {
    match value {
        Value::String(s) => Ok(MapKey::String(s.clone())),
        Value::Int(n) => Ok(MapKey::Int(*n)),
        Value::Bool(b) => Ok(MapKey::Bool(*b)),
        other => Err(RuntimeError::new(
            span,
            format!("cannot use {} as a map key", other.type_name()),
        )
        .into()),
    }
}

fn expect_int(value: &Value, span: Span) -> EvalResult<i64> {
    match value {
        Value::Int(n) => Ok(*n),
        other => Err(
            RuntimeError::new(span, format!("expected Int, found {}", other.type_name())).into(),
        ),
    }
}

fn normalize_index(index: i64, len: usize, span: Span) -> EvalResult<usize> {
    let idx = if index < 0 { len as i64 + index } else { index };
    if idx < 0 || idx as usize >= len {
        Err(RuntimeError::new(
            span,
            format!("index {index} out of bounds for length {len}"),
        )
        .into())
    } else {
        Ok(idx as usize)
    }
}

fn expect_arity(name: &str, args: &[Value], n: usize, span: Span) -> EvalResult<()> {
    if args.len() == n {
        Ok(())
    } else {
        Err(RuntimeError::new(
            span,
            format!("{name}() expected {n} argument(s), got {}", args.len()),
        )
        .into())
    }
}

fn to_int(value: &Value, span: Span) -> EvalResult<Value> {
    match value {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(n) => Ok(Value::Int(*n as i64)),
        Value::Bool(true) => Ok(Value::Int(1)),
        Value::Bool(false) => Ok(Value::Int(0)),
        Value::String(s) => s
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| RuntimeError::new(span, format!("cannot parse `{s}` as Int")).into()),
        other => Err(RuntimeError::new(
            span,
            format!("cannot convert {} to Int", other.type_name()),
        )
        .into()),
    }
}

fn to_float(value: &Value, span: Span) -> EvalResult<Value> {
    match value {
        Value::Float(n) => Ok(Value::Float(*n)),
        Value::Int(n) => Ok(Value::Float(*n as f64)),
        Value::String(s) => s
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| RuntimeError::new(span, format!("cannot parse `{s}` as Float")).into()),
        other => Err(RuntimeError::new(
            span,
            format!("cannot convert {} to Float", other.type_name()),
        )
        .into()),
    }
}

// Silence unused source field warning by using it in a helper.
impl Interpreter<'_> {
    #[allow(dead_code)]
    fn file_name(&self) -> &str {
        self.source.name()
    }
}
