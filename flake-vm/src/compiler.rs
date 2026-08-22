//! AST → bytecode compiler.

use std::collections::{HashMap, HashSet};

use flake_ast::{
    AssignOp, BinOp, Block, Expr, FnDecl, InterpPart, Item, Literal, MatchArm, Pattern, Program,
    Stmt, UnOp,
};
use flake_parser::{ModuleGraph, import_alias, is_exported, qualify};

use crate::error::VmError;
use crate::opcode::{Chunk, Op};
use crate::value::{Function, Value};

struct Names {
    /// Module prefix for this file (`math`), `None` for the entry file.
    prefix: Option<String>,
    /// alias → module name
    imports: HashMap<String, String>,
    local_fns: HashSet<String>,
    /// bare imported name → qualified name
    imported_fns: HashMap<String, String>,
    /// alias → exported function names (empty set means nothing is exported)
    exported: HashMap<String, HashSet<String>>,
    enums: HashMap<String, Vec<String>>,
}

impl Names {
    fn global(&self, name: &str) -> String {
        if self.local_fns.contains(name) {
            if let Some(prefix) = &self.prefix {
                return qualify(prefix, name);
            }
        }
        if let Some(q) = self.imported_fns.get(name) {
            return q.clone();
        }
        name.to_string()
    }

    fn field_global(&self, alias: &str, field: &str) -> Option<String> {
        let module = self.imports.get(alias)?;
        if let Some(exports) = self.exported.get(alias) {
            if !exports.contains(field) {
                return None;
            }
        }
        Some(qualify(module, field))
    }

    fn enum_variants(&self, target: &Expr) -> Option<&[String]> {
        match target {
            Expr::Ident(id) => self.enums.get(&id.name).map(Vec::as_slice),
            Expr::Field { target, field, .. } => {
                if let Expr::Ident(module) = target.as_ref() {
                    if self.imports.contains_key(&module.name) {
                        return self.enums.get(&field.name).map(Vec::as_slice);
                    }
                }
                None
            }
            _ => None,
        }
    }
}

pub struct Compiled {
    pub functions: Vec<Function>,
}

struct Local {
    name: String,
    depth: u32,
}

struct LoopCtx {
    continue_target: u16,
    break_jumps: Vec<usize>,
}

struct FnCompiler<'a> {
    chunk: Chunk,
    locals: Vec<Local>,
    scope: u32,
    max_locals: u16,
    loops: Vec<LoopCtx>,
    names: &'a Names,
}

impl<'a> FnCompiler<'a> {
    fn new(params: Vec<String>, names: &'a Names) -> Self {
        let locals = params
            .into_iter()
            .map(|name| Local { name, depth: 0 })
            .collect::<Vec<_>>();
        let max_locals = locals.len() as u16;
        Self {
            chunk: Chunk::new(),
            locals,
            scope: 0,
            max_locals,
            loops: Vec::new(),
            names,
        }
    }

    fn finish(self, name: String, arity: u8) -> Function {
        Function {
            name,
            arity,
            chunk: self.chunk,
            locals: self.max_locals,
        }
    }

    fn push_scope(&mut self) {
        self.scope += 1;
    }

    fn pop_scope(&mut self) {
        self.scope = self.scope.saturating_sub(1);
        while self
            .locals
            .last()
            .is_some_and(|local| local.depth > self.scope)
        {
            self.locals.pop();
        }
    }

    fn add_local(&mut self, name: String) -> u16 {
        let i = self.locals.len() as u16;
        self.locals.push(Local {
            name,
            depth: self.scope,
        });
        self.max_locals = self.max_locals.max(self.locals.len() as u16);
        i
    }

    fn resolve_local(&self, name: &str) -> Option<u16> {
        self.locals
            .iter()
            .enumerate()
            .rev()
            .find(|(_, local)| local.name == name)
            .map(|(i, _)| i as u16)
    }

    fn compile_block_value(&mut self, block: &Block, scoped: bool) -> Result<(), VmError> {
        if scoped {
            self.push_scope();
        }
        for stmt in &block.stmts {
            self.compile_stmt(stmt)?;
        }
        if let Some(tail) = &block.tail {
            self.compile_expr(tail)?;
        } else {
            self.chunk.emit(Op::Nil);
        }
        if scoped {
            self.pop_scope();
        }
        Ok(())
    }

    fn compile_block_as_stmt(&mut self, block: &Block) -> Result<(), VmError> {
        self.compile_block_value(block, true)?;
        self.chunk.emit(Op::Pop);
        Ok(())
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), VmError> {
        match stmt {
            Stmt::Let(s) | Stmt::Var(s) => {
                self.compile_expr(&s.value)?;
                let slot = self.add_local(s.name.name.clone());
                self.chunk.emit(Op::SetLocal(slot));
                self.chunk.emit(Op::Pop);
                Ok(())
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.compile_expr(v)?;
                } else {
                    self.chunk.emit(Op::Nil);
                }
                self.chunk.emit(Op::Return);
                Ok(())
            }
            Stmt::Expr(e) => {
                self.compile_expr(e)?;
                self.chunk.emit(Op::Pop);
                Ok(())
            }
            Stmt::While { cond, body, .. } => self.compile_while(cond, body),
            Stmt::Loop { body, .. } => self.compile_loop(body),
            Stmt::For {
                name, iter, body, ..
            } => self.compile_for(&name.name, iter, body),
            Stmt::Break { span } => {
                let jump = self.chunk.emit(Op::Jump(0));
                match self.loops.last_mut() {
                    Some(ctx) => {
                        ctx.break_jumps.push(jump);
                        Ok(())
                    }
                    None => Err(VmError::new(*span, "`break` outside of a loop")),
                }
            }
            Stmt::Continue { span } => {
                let target = self
                    .loops
                    .last()
                    .map(|ctx| ctx.continue_target)
                    .ok_or_else(|| VmError::new(*span, "`continue` outside of a loop"))?;
                self.chunk.emit(Op::Jump(target));
                Ok(())
            }
        }
    }

    fn compile_while(&mut self, cond: &Expr, body: &Block) -> Result<(), VmError> {
        let start = self.chunk.ops.len() as u16;
        self.loops.push(LoopCtx {
            continue_target: start,
            break_jumps: Vec::new(),
        });
        self.compile_expr(cond)?;
        let false_jump = self.chunk.emit(Op::JumpIfFalse(0));
        self.chunk.emit(Op::Pop);
        self.compile_block_as_stmt(body)?;
        self.chunk.emit(Op::Jump(start));
        let false_exit = self.chunk.ops.len();
        self.chunk.patch_jump(false_jump, false_exit);
        self.chunk.emit(Op::Pop);
        let end = self.chunk.ops.len();
        self.patch_breaks(end);
        Ok(())
    }

    fn compile_loop(&mut self, body: &Block) -> Result<(), VmError> {
        let start = self.chunk.ops.len() as u16;
        self.loops.push(LoopCtx {
            continue_target: start,
            break_jumps: Vec::new(),
        });
        self.compile_block_as_stmt(body)?;
        self.chunk.emit(Op::Jump(start));
        let end = self.chunk.ops.len();
        self.patch_breaks(end);
        Ok(())
    }

    fn compile_for(&mut self, name: &str, iter: &Expr, body: &Block) -> Result<(), VmError> {
        self.compile_expr(iter)?;
        self.chunk.emit(Op::MakeIter);
        self.push_scope();
        let iter_slot = self.add_local(format!("__iter_{name}"));
        self.chunk.emit(Op::SetLocal(iter_slot));
        self.chunk.emit(Op::Pop);
        let var_slot = self.add_local(name.to_string());

        let start = self.chunk.ops.len() as u16;
        self.loops.push(LoopCtx {
            continue_target: start,
            break_jumps: Vec::new(),
        });
        self.chunk.emit(Op::GetLocal(iter_slot));
        let done_jump = self.chunk.emit(Op::IterNext(0));
        self.chunk.emit(Op::SetLocal(var_slot));
        self.chunk.emit(Op::Pop); // item
        self.chunk.emit(Op::Pop); // iterator copy
        self.compile_block_as_stmt(body)?;
        self.chunk.emit(Op::Jump(start));
        let exhausted = self.chunk.ops.len();
        self.chunk.patch_jump(done_jump, exhausted);
        self.chunk.emit(Op::Pop); // leftover iterator copy
        let end = self.chunk.ops.len();
        self.patch_breaks(end);
        self.pop_scope();
        Ok(())
    }

    fn patch_breaks(&mut self, end: usize) {
        if let Some(ctx) = self.loops.pop() {
            for jump in ctx.break_jumps {
                self.chunk.patch_jump(jump, end);
            }
        }
    }

    fn compile_expr(&mut self, expr: &Expr) -> Result<(), VmError> {
        match expr {
            Expr::Literal { value, .. } => {
                match value {
                    Literal::Nil => {
                        self.chunk.emit(Op::Nil);
                    }
                    Literal::Bool(true) => {
                        self.chunk.emit(Op::True);
                    }
                    Literal::Bool(false) => {
                        self.chunk.emit(Op::False);
                    }
                    Literal::Int(n) => {
                        let c = self.chunk.add_constant(Value::Int(*n));
                        self.chunk.emit(Op::Constant(c));
                    }
                    Literal::Float(n) => {
                        let c = self.chunk.add_constant(Value::Float(*n));
                        self.chunk.emit(Op::Constant(c));
                    }
                    Literal::String(s) => {
                        let c = self.chunk.add_constant(Value::from_string(s.clone()));
                        self.chunk.emit(Op::Constant(c));
                    }
                }
                Ok(())
            }
            Expr::Ident(id) => {
                if let Some(slot) = self.resolve_local(&id.name) {
                    self.chunk.emit(Op::GetLocal(slot));
                } else {
                    let name = self.names.global(&id.name);
                    let c = self.chunk.add_constant(Value::from_string(name));
                    self.chunk.emit(Op::GetGlobal(c));
                }
                Ok(())
            }
            Expr::Interpolated { parts, .. } => {
                let mut n = 0u8;
                for part in parts {
                    match part {
                        InterpPart::Text(t) => {
                            let c = self.chunk.add_constant(Value::from_string(t.clone()));
                            self.chunk.emit(Op::Constant(c));
                            n += 1;
                        }
                        InterpPart::Expr(e) => {
                            self.compile_expr(e)?;
                            n += 1;
                        }
                    }
                }
                self.chunk.emit(Op::Concat(n));
                Ok(())
            }
            Expr::List { elements, .. } => {
                for e in elements {
                    self.compile_expr(e)?;
                }
                self.chunk.emit(Op::BuildList(elements.len() as u16));
                Ok(())
            }
            Expr::Map { entries, .. } => {
                for (k, v) in entries {
                    self.compile_expr(k)?;
                    self.compile_expr(v)?;
                }
                self.chunk.emit(Op::BuildMap(entries.len() as u16));
                Ok(())
            }
            Expr::StructInit { name, fields, .. } => {
                let mut field_consts = Vec::new();
                for (field, value) in fields {
                    self.compile_expr(value)?;
                    field_consts.push(
                        self.chunk
                            .add_constant(Value::from_string(field.name.clone())),
                    );
                }
                let name_c = self
                    .chunk
                    .add_constant(Value::from_string(name.name.clone()));
                self.chunk.emit(Op::BuildStruct {
                    name: name_c,
                    fields: field_consts,
                });
                Ok(())
            }
            Expr::Unary { op, expr, .. } => {
                self.compile_expr(expr)?;
                match op {
                    UnOp::Neg => {
                        self.chunk.emit(Op::Neg);
                    }
                    UnOp::Not => {
                        self.chunk.emit(Op::Not);
                    }
                    UnOp::Ref | UnOp::RefMut => {}
                }
                Ok(())
            }
            Expr::Binary {
                op, left, right, ..
            } => {
                if *op == BinOp::And {
                    self.compile_expr(left)?;
                    let jump = self.chunk.emit(Op::JumpIfFalse(0));
                    self.chunk.emit(Op::Pop);
                    self.compile_expr(right)?;
                    let end = self.chunk.ops.len();
                    self.chunk.patch_jump(jump, end);
                    return Ok(());
                }
                if *op == BinOp::Or {
                    self.compile_expr(left)?;
                    let jump = self.chunk.emit(Op::JumpIfFalse(0));
                    let skip = self.chunk.emit(Op::Jump(0));
                    let false_target = self.chunk.ops.len();
                    self.chunk.patch_jump(jump, false_target);
                    self.chunk.emit(Op::Pop);
                    self.compile_expr(right)?;
                    let end = self.chunk.ops.len();
                    self.chunk.patch_jump(skip, end);
                    return Ok(());
                }
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                self.chunk.emit(bin_op(*op));
                Ok(())
            }
            Expr::Range { start, end, .. } => {
                self.compile_expr(start)?;
                self.compile_expr(end)?;
                self.chunk.emit(Op::MakeRange);
                Ok(())
            }
            Expr::Assign {
                op,
                target,
                value,
                span,
            } => self.compile_assign(*op, target, value, *span),
            Expr::Call { callee, args, .. } => {
                if let Expr::Field { target, field, .. } = callee.as_ref() {
                    if let Some(vars) = self.names.enum_variants(target) {
                        if let Some(tag) = vars.iter().position(|n| n == &field.name) {
                            let c = self.chunk.add_constant(Value::Int(tag as i64));
                            self.chunk.emit(Op::Constant(c));
                            for arg in args {
                                self.compile_expr(arg)?;
                            }
                            self.chunk.emit(Op::BuildList((args.len() + 1) as u16));
                            return Ok(());
                        }
                    }
                }
                self.compile_expr(callee)?;
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.chunk.emit(Op::Call(args.len() as u8));
                Ok(())
            }
            Expr::Spawn { call, span } => {
                let Expr::Call { callee, args, .. } = call.as_ref() else {
                    return Err(VmError::new(*span, "`spawn` expects a function call"));
                };
                // Enum construction has no callable value in the VM. Preserve
                // the Task[T] surface while completing that zero-work task now.
                if let Expr::Field { target, field, .. } = callee.as_ref() {
                    if self
                        .names
                        .enum_variants(target)
                        .is_some_and(|vars| vars.iter().any(|name| name == &field.name))
                    {
                        self.compile_expr(call)?;
                        self.chunk.emit(Op::ReadyTask);
                        return Ok(());
                    }
                }
                self.compile_expr(callee)?;
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.chunk.emit(Op::Spawn(args.len() as u8));
                Ok(())
            }
            Expr::Await { task, .. } => {
                self.compile_expr(task)?;
                self.chunk.emit(Op::Await);
                Ok(())
            }
            Expr::Index { target, index, .. } => {
                self.compile_expr(target)?;
                self.compile_expr(index)?;
                self.chunk.emit(Op::GetIndex);
                Ok(())
            }
            Expr::Field { target, field, .. } => {
                if let Some(vars) = self.names.enum_variants(target) {
                    if let Some(tag) = vars.iter().position(|n| n == &field.name) {
                        let c = self.chunk.add_constant(Value::Int(tag as i64));
                        self.chunk.emit(Op::Constant(c));
                        self.chunk.emit(Op::BuildList(1));
                        return Ok(());
                    }
                }
                if let Expr::Ident(id) = target.as_ref() {
                    if let Some(global) = self.names.field_global(&id.name, &field.name) {
                        let c = self.chunk.add_constant(Value::from_string(global));
                        self.chunk.emit(Op::GetGlobal(c));
                        return Ok(());
                    }
                }
                self.compile_expr(target)?;
                let c = self
                    .chunk
                    .add_constant(Value::from_string(field.name.clone()));
                self.chunk.emit(Op::GetField(c));
                Ok(())
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.compile_expr(cond)?;
                let else_jump = self.chunk.emit(Op::JumpIfFalse(0));
                self.chunk.emit(Op::Pop);
                self.compile_block_value(then_block, true)?;
                let end_jump = self.chunk.emit(Op::Jump(0));
                let else_target = self.chunk.ops.len();
                self.chunk.patch_jump(else_jump, else_target);
                self.chunk.emit(Op::Pop);
                if let Some(els) = else_block {
                    self.compile_expr(els)?;
                } else {
                    self.chunk.emit(Op::Nil);
                }
                let end = self.chunk.ops.len();
                self.chunk.patch_jump(end_jump, end);
                Ok(())
            }
            Expr::Match {
                scrutinee, arms, ..
            } => self.compile_match(scrutinee, arms),
            Expr::Block(b) => self.compile_block_value(b, true),
        }
    }

    fn compile_match(&mut self, scrutinee: &Expr, arms: &[MatchArm]) -> Result<(), VmError> {
        if arms.is_empty() {
            self.chunk.emit(Op::Nil);
            return Ok(());
        }
        self.compile_expr(scrutinee)?;
        self.push_scope();
        let src_slot = self.add_local("__match_src".into());
        self.chunk.emit(Op::SetLocal(src_slot));
        self.chunk.emit(Op::Pop);

        self.chunk.emit(Op::GetLocal(src_slot));
        let zero = self.chunk.add_constant(Value::Int(0));
        self.chunk.emit(Op::Constant(zero));
        self.chunk.emit(Op::GetIndex);
        let tag_slot = self.add_local("__match_tag".into());
        self.chunk.emit(Op::SetLocal(tag_slot));
        self.chunk.emit(Op::Pop);

        let mut end_jumps = Vec::new();
        let mut last_was_variant = false;
        for (i, arm) in arms.iter().enumerate() {
            let last = i + 1 == arms.len();
            last_was_variant = false;
            match &arm.pattern {
                Pattern::Wildcard { .. } | Pattern::Ident(_) => {
                    self.push_scope();
                    if let Pattern::Ident(id) = &arm.pattern {
                        self.chunk.emit(Op::GetLocal(src_slot));
                        let slot = self.add_local(id.name.clone());
                        self.chunk.emit(Op::SetLocal(slot));
                        self.chunk.emit(Op::Pop);
                    }
                    self.compile_expr(&arm.body)?;
                    self.pop_scope();
                    if !last {
                        end_jumps.push(self.chunk.emit(Op::Jump(0)));
                    }
                }
                Pattern::Variant {
                    variant, binds, ty, ..
                } => {
                    last_was_variant = true;
                    let ename = ty.as_ref().map(|t| t.name.as_str()).unwrap_or("");
                    let tag_val = self
                        .names
                        .enums
                        .get(ename)
                        .and_then(|vs| vs.iter().position(|n| n == &variant.name))
                        .unwrap_or(0) as i64;
                    self.chunk.emit(Op::GetLocal(tag_slot));
                    let want = self.chunk.add_constant(Value::Int(tag_val));
                    self.chunk.emit(Op::Constant(want));
                    self.chunk.emit(Op::Eq);
                    let miss = self.chunk.emit(Op::JumpIfFalse(0));
                    self.chunk.emit(Op::Pop);
                    self.push_scope();
                    for (fi, bind) in binds.iter().enumerate() {
                        self.chunk.emit(Op::GetLocal(src_slot));
                        let idx = self.chunk.add_constant(Value::Int((fi + 1) as i64));
                        self.chunk.emit(Op::Constant(idx));
                        self.chunk.emit(Op::GetIndex);
                        let slot = self.add_local(bind.name.clone());
                        self.chunk.emit(Op::SetLocal(slot));
                        self.chunk.emit(Op::Pop);
                    }
                    self.compile_expr(&arm.body)?;
                    self.pop_scope();
                    end_jumps.push(self.chunk.emit(Op::Jump(0)));
                    let miss_target = self.chunk.ops.len();
                    self.chunk.patch_jump(miss, miss_target);
                    self.chunk.emit(Op::Pop);
                }
            }
        }
        if last_was_variant {
            self.chunk.emit(Op::Nil);
        }
        let end = self.chunk.ops.len();
        for jump in end_jumps {
            self.chunk.patch_jump(jump, end);
        }
        self.pop_scope();
        Ok(())
    }

    fn compile_assign(
        &mut self,
        op: AssignOp,
        target: &Expr,
        value: &Expr,
        span: flake_ast::Span,
    ) -> Result<(), VmError> {
        if op == AssignOp::Assign {
            return self.compile_store(target, value, None, span);
        }
        let bin = match op {
            AssignOp::AddAssign => BinOp::Add,
            AssignOp::SubAssign => BinOp::Sub,
            AssignOp::MulAssign => BinOp::Mul,
            AssignOp::DivAssign => BinOp::Div,
            AssignOp::RemAssign => BinOp::Rem,
            AssignOp::Assign => unreachable!(),
        };
        self.compile_store(target, value, Some(bin), span)
    }

    fn compile_store(
        &mut self,
        target: &Expr,
        value: &Expr,
        compound: Option<BinOp>,
        span: flake_ast::Span,
    ) -> Result<(), VmError> {
        match target {
            Expr::Ident(id) => {
                if let Some(bin) = compound {
                    if let Some(slot) = self.resolve_local(&id.name) {
                        self.chunk.emit(Op::GetLocal(slot));
                    } else {
                        let c = self.chunk.add_constant(Value::from_string(id.name.clone()));
                        self.chunk.emit(Op::GetGlobal(c));
                    }
                    self.compile_expr(value)?;
                    self.chunk.emit(bin_op(bin));
                } else {
                    self.compile_expr(value)?;
                }
                if let Some(slot) = self.resolve_local(&id.name) {
                    self.chunk.emit(Op::SetLocal(slot));
                } else {
                    let c = self.chunk.add_constant(Value::from_string(id.name.clone()));
                    self.chunk.emit(Op::DefineGlobal(c));
                }
                Ok(())
            }
            Expr::Index {
                target: container,
                index,
                ..
            } => {
                if let Some(bin) = compound {
                    self.compile_expr(container)?;
                    self.compile_expr(index)?;
                    self.chunk.emit(Op::DupTwo);
                    self.chunk.emit(Op::GetIndex);
                    self.compile_expr(value)?;
                    self.chunk.emit(bin_op(bin));
                    self.chunk.emit(Op::Rot3);
                    self.chunk.emit(Op::SetIndex);
                } else {
                    self.compile_expr(value)?;
                    self.compile_expr(container)?;
                    self.compile_expr(index)?;
                    self.chunk.emit(Op::SetIndex);
                }
                Ok(())
            }
            Expr::Field {
                target: container,
                field,
                ..
            } => {
                let c = self
                    .chunk
                    .add_constant(Value::from_string(field.name.clone()));
                if let Some(bin) = compound {
                    self.compile_expr(container)?;
                    self.chunk.emit(Op::Dup);
                    self.chunk.emit(Op::GetField(c));
                    self.compile_expr(value)?;
                    self.chunk.emit(bin_op(bin));
                    self.chunk.emit(Op::Swap);
                    self.chunk.emit(Op::SetField(c));
                } else {
                    self.compile_expr(value)?;
                    self.compile_expr(container)?;
                    self.chunk.emit(Op::SetField(c));
                }
                Ok(())
            }
            _ => Err(VmError::new(span, "invalid assignment target")),
        }
    }
}

fn bin_op(op: BinOp) -> Op {
    match op {
        BinOp::Add => Op::Add,
        BinOp::Sub => Op::Sub,
        BinOp::Div => Op::Div,
        BinOp::Mul => Op::Mul,
        BinOp::Rem => Op::Rem,
        BinOp::Eq => Op::Eq,
        BinOp::Ne => Op::Ne,
        BinOp::Lt => Op::Lt,
        BinOp::Le => Op::Le,
        BinOp::Gt => Op::Gt,
        BinOp::Ge => Op::Ge,
        BinOp::And | BinOp::Or => unreachable!("short-circuit ops are compiled separately"),
    }
}

#[allow(dead_code)]
pub fn compile(program: &Program) -> Result<Compiled, VmError> {
    compile_with_names(
        program,
        &Names {
            prefix: None,
            imports: HashMap::new(),
            local_fns: program
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Fn(f) => Some(f.name.name.clone()),
                    _ => None,
                })
                .collect(),
            imported_fns: HashMap::new(),
            exported: HashMap::new(),
            enums: enums_from(program),
        },
    )
}

pub fn compile_graph(graph: &ModuleGraph) -> Result<Compiled, VmError> {
    let mut functions = Vec::new();
    let entry = graph.entry().name.as_str();
    for module in &graph.modules {
        let names = names_for(graph, module, module.name == entry);
        let compiled = compile_with_names(&module.program, &names)?;
        functions.extend(compiled.functions);
    }
    Ok(Compiled { functions })
}

fn names_for(graph: &ModuleGraph, module: &flake_parser::LoadedModule, is_entry: bool) -> Names {
    let local_fns: HashSet<String> = module
        .program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Fn(f) => Some(f.name.name.clone()),
            _ => None,
        })
        .collect();
    let mut imports = HashMap::new();
    let mut imported_fns = HashMap::new();
    let mut exported = HashMap::new();
    let mut enums = enums_from(&module.program);
    for item in &module.program.items {
        if let Item::Import(import) = item {
            let module_name = import.path.name.clone();
            let alias = import_alias(import).to_string();
            imports.insert(alias.clone(), module_name.clone());
            exported.entry(alias.clone()).or_insert_with(HashSet::new);
            if let Some(imported) = graph.get(&module_name) {
                for item in &imported.program.items {
                    if !is_exported(item, &imported.program) {
                        continue;
                    }
                    match item {
                        Item::Fn(func) => {
                            imported_fns
                                .entry(func.name.name.clone())
                                .or_insert_with(|| qualify(&module_name, &func.name.name));
                            exported
                                .get_mut(&alias)
                                .unwrap()
                                .insert(func.name.name.clone());
                        }
                        Item::Enum(en) => {
                            enums.entry(en.name.name.clone()).or_insert_with(|| {
                                en.variants.iter().map(|v| v.name.name.clone()).collect()
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Names {
        prefix: if is_entry {
            None
        } else {
            Some(module.name.clone())
        },
        imports,
        local_fns,
        imported_fns,
        exported,
        enums,
    }
}

fn enums_from(program: &Program) -> HashMap<String, Vec<String>> {
    let mut enums = HashMap::new();
    for item in &program.items {
        if let Item::Enum(en) = item {
            enums.insert(
                en.name.name.clone(),
                en.variants.iter().map(|v| v.name.name.clone()).collect(),
            );
        }
    }
    enums
}

fn compile_with_names(program: &Program, names: &Names) -> Result<Compiled, VmError> {
    let mut functions = Vec::new();
    for item in &program.items {
        match item {
            Item::Fn(func) => functions.push(compile_fn(func, names)?),
            Item::Import(_) | Item::Struct(_) | Item::Type(_) | Item::Enum(_) => {}
        }
    }
    Ok(Compiled { functions })
}

fn compile_fn(func: &FnDecl, names: &Names) -> Result<Function, VmError> {
    let params: Vec<String> = func.params.iter().map(|p| p.name.name.clone()).collect();
    let arity = params.len() as u8;
    let mut compiler = FnCompiler::new(params, names);
    compiler.compile_block_value(&func.body, false)?;
    compiler.chunk.emit(Op::Return);
    Ok(compiler.finish(names.global(&func.name.name), arity))
}
