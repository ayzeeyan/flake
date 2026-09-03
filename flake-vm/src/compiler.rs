//! AST → bytecode compiler.

use std::collections::{HashMap, HashSet};

use flake_ast::{
    AssignOp, BinOp, Block, Expr, FnDecl, InterpPart, Item, Literal, MatchArm, Pattern, Program,
    Stmt, UnOp,
};
use flake_parser::{ModuleGraph, import_alias, qualify};

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
    local_types: HashSet<String>,
    imported_types: HashMap<String, String>,
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
        if let Some(q) = self.imported_fns.get(&format!("{alias}.{field}")) {
            return Some(q.clone());
        }
        let module = self.imports.get(alias)?;
        if let Some(exports) = self.exported.get(alias) {
            if !exports.contains(field) {
                return None;
            }
        }
        Some(qualify(module, field))
    }

    fn type_global(&self, name: &str) -> String {
        if let Some((alias, item)) = name.split_once('.') {
            if let Some(module) = self.imports.get(alias) {
                return qualify(module, item);
            }
        }
        if self.local_types.contains(name) {
            if let Some(prefix) = &self.prefix {
                return qualify(prefix, name);
            }
        }
        self.imported_types
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    fn enum_variants(&self, target: &Expr) -> Option<&[String]> {
        match target {
            Expr::Ident(id) => self.enums.get(&id.name).map(Vec::as_slice),
            Expr::Field { target, field, .. } => {
                if let Expr::Ident(module) = target.as_ref() {
                    if self.imports.contains_key(&module.name) {
                        let qualified = format!("{}.{}", module.name, field.name);
                        return self.enums.get(&qualified).map(Vec::as_slice);
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
    consts: &'a HashMap<String, flake_ast::ConstValue>,
}

impl<'a> FnCompiler<'a> {
    fn new(
        params: Vec<String>,
        names: &'a Names,
        consts: &'a HashMap<String, flake_ast::ConstValue>,
    ) -> Self {
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
            consts,
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
        let previous = self.chunk.replace_span(stmt.span());
        let result = self.compile_stmt_inner(stmt);
        self.chunk.set_span(previous);
        result
    }

    fn compile_stmt_inner(&mut self, stmt: &Stmt) -> Result<(), VmError> {
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
        let previous = self.chunk.replace_span(expr.span());
        let result = self.compile_expr_inner(expr);
        self.chunk.set_span(previous);
        result
    }

    fn compile_expr_inner(&mut self, expr: &Expr) -> Result<(), VmError> {
        match expr {
            Expr::Literal { value, .. } => {
                self.emit_literal(value);
                Ok(())
            }
            Expr::Ident(id) => {
                if let Some(slot) = self.resolve_local(&id.name) {
                    self.chunk.emit(Op::GetLocal(slot));
                } else if let Some(value) = self.consts.get(&id.name) {
                    self.emit_const_value(value);
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
                    .add_constant(Value::from_string(self.names.type_global(&name.name)));
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
                    if let Expr::Ident(id) = target.as_ref() {
                        if let Some(global) = self.names.field_global(&id.name, &field.name) {
                            let c = self.chunk.add_constant(Value::from_string(global));
                            self.chunk.emit(Op::GetGlobal(c));
                            for arg in args {
                                self.compile_expr(arg)?;
                            }
                            self.chunk.emit(Op::Call(args.len() as u8));
                            return Ok(());
                        }
                    }
                    self.compile_expr(target)?;
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    let c = self
                        .chunk
                        .add_constant(Value::from_string(field.name.clone()));
                    self.chunk.emit(Op::CallMethod(c, args.len() as u8));
                    return Ok(());
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
                    if let Expr::Ident(id) = target.as_ref() {
                        if let Some(global) = self.names.field_global(&id.name, &field.name) {
                            let c = self.chunk.add_constant(Value::from_string(global));
                            self.chunk.emit(Op::GetGlobal(c));
                            for arg in args {
                                self.compile_expr(arg)?;
                            }
                            self.chunk.emit(Op::Spawn(args.len() as u8));
                            return Ok(());
                        }
                    }
                    self.compile_expr(target)?;
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    let c = self
                        .chunk
                        .add_constant(Value::from_string(field.name.clone()));
                    self.chunk.emit(Op::SpawnMethod(c, args.len() as u8));
                    return Ok(());
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
            Expr::Try { expr, .. } => self.compile_try(expr),
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
            Expr::Nursery { body, .. } => {
                self.chunk.emit(Op::EnterNursery);
                self.compile_block_value(body, true)?;
                self.chunk.emit(Op::ExitNursery);
                Ok(())
            }
        }
    }

    fn emit_const_value(&mut self, value: &flake_ast::ConstValue) {
        match value {
            flake_ast::ConstValue::Nil => {
                self.chunk.emit(Op::Nil);
            }
            flake_ast::ConstValue::Bool(true) => {
                self.chunk.emit(Op::True);
            }
            flake_ast::ConstValue::Bool(false) => {
                self.chunk.emit(Op::False);
            }
            flake_ast::ConstValue::Int(n) => {
                let c = self.chunk.add_constant(Value::Int(*n));
                self.chunk.emit(Op::Constant(c));
            }
            flake_ast::ConstValue::Float(n) => {
                let c = self.chunk.add_constant(Value::Float(*n));
                self.chunk.emit(Op::Constant(c));
            }
            flake_ast::ConstValue::String(s) => {
                let c = self.chunk.add_constant(Value::from_string(s.clone()));
                self.chunk.emit(Op::Constant(c));
            }
        }
    }

    fn emit_literal(&mut self, value: &Literal) {
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
            Literal::Int(value) => {
                let constant = self.chunk.add_constant(Value::Int(*value));
                self.chunk.emit(Op::Constant(constant));
            }
            Literal::Float(value) => {
                let constant = self.chunk.add_constant(Value::Float(*value));
                self.chunk.emit(Op::Constant(constant));
            }
            Literal::String(value) => {
                let constant = self.chunk.add_constant(Value::from_string(value.clone()));
                self.chunk.emit(Op::Constant(constant));
            }
        }
    }

    fn compile_try(&mut self, expr: &Expr) -> Result<(), VmError> {
        self.compile_expr(expr)?;
        self.push_scope();
        let src_slot = self.add_local("__try_result".into());
        self.chunk.emit(Op::SetLocal(src_slot));
        self.chunk.emit(Op::Pop);

        self.chunk.emit(Op::GetLocal(src_slot));
        let tag_index = self.chunk.add_constant(Value::Int(0));
        self.chunk.emit(Op::Constant(tag_index));
        self.chunk.emit(Op::GetIndex);
        let ok_tag = self.chunk.add_constant(Value::Int(0));
        self.chunk.emit(Op::Constant(ok_tag));
        self.chunk.emit(Op::Eq);
        let propagate = self.chunk.emit(Op::JumpIfFalse(0));
        self.chunk.emit(Op::Pop);

        self.chunk.emit(Op::GetLocal(src_slot));
        let value_index = self.chunk.add_constant(Value::Int(1));
        self.chunk.emit(Op::Constant(value_index));
        self.chunk.emit(Op::GetIndex);
        let done = self.chunk.emit(Op::Jump(0));

        let propagate_target = self.chunk.ops.len();
        self.chunk.patch_jump(propagate, propagate_target);
        self.chunk.emit(Op::Pop);
        self.chunk.emit(Op::GetLocal(src_slot));
        self.chunk.emit(Op::Return);

        let done_target = self.chunk.ops.len();
        self.chunk.patch_jump(done, done_target);
        self.pop_scope();
        Ok(())
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

        let mut end_jumps = Vec::new();
        for arm in arms {
            let is_catch_all = matches!(&arm.pattern, Pattern::Wildcard { .. })
                || matches!(&arm.pattern, Pattern::Ident(id) if id.name == "_" || (!id.name.chars().next().is_some_and(|c| c.is_uppercase()) && !self.names.enums.values().any(|vs| vs.iter().any(|n| n == &id.name))));

            self.push_scope();
            let mut miss_jumps = Vec::new();
            self.compile_pattern_test(src_slot, &arm.pattern, &mut miss_jumps)?;
            self.compile_expr(&arm.body)?;
            self.pop_scope();
            end_jumps.push(self.chunk.emit(Op::Jump(0)));

            let miss_target = self.chunk.ops.len();
            for j in miss_jumps {
                self.chunk.patch_jump(j, miss_target);
            }
            if !is_catch_all {
                self.chunk.emit(Op::Pop);
            } else {
                break;
            }
        }
        self.chunk.emit(Op::Nil);
        let end = self.chunk.ops.len();
        for jump in end_jumps {
            self.chunk.patch_jump(jump, end);
        }
        self.pop_scope();
        Ok(())
    }

    fn compile_pattern_test(
        &mut self,
        val_slot: u16,
        pat: &Pattern,
        miss_jumps: &mut Vec<usize>,
    ) -> Result<(), VmError> {
        match pat {
            Pattern::Wildcard { .. } => Ok(()),
            Pattern::Ident(id) => {
                if id.name == "_" {
                    Ok(())
                } else if id.name.chars().next().is_some_and(|c| c.is_uppercase())
                    && self
                        .names
                        .enums
                        .values()
                        .any(|vs| vs.iter().any(|n| n == &id.name))
                {
                    let tag_val = self
                        .names
                        .enums
                        .values()
                        .find_map(|vs| vs.iter().position(|n| n == &id.name))
                        .unwrap_or(0) as i64;
                    self.chunk.emit(Op::GetLocal(val_slot));
                    let tag_index = self.chunk.add_constant(Value::Int(0));
                    self.chunk.emit(Op::Constant(tag_index));
                    self.chunk.emit(Op::GetIndex);
                    let want = self.chunk.add_constant(Value::Int(tag_val));
                    self.chunk.emit(Op::Constant(want));
                    self.chunk.emit(Op::Eq);
                    miss_jumps.push(self.chunk.emit(Op::JumpIfFalse(0)));
                    self.chunk.emit(Op::Pop);
                    Ok(())
                } else {
                    let slot = self.add_local(id.name.clone());
                    self.chunk.emit(Op::GetLocal(val_slot));
                    self.chunk.emit(Op::SetLocal(slot));
                    self.chunk.emit(Op::Pop);
                    Ok(())
                }
            }
            Pattern::Literal { value, .. } => {
                self.chunk.emit(Op::GetLocal(val_slot));
                self.emit_literal(value);
                self.chunk.emit(Op::Eq);
                miss_jumps.push(self.chunk.emit(Op::JumpIfFalse(0)));
                self.chunk.emit(Op::Pop);
                Ok(())
            }
            Pattern::List { patterns, .. } => {
                let len_name = self.chunk.add_constant(Value::from_string("len"));
                self.chunk.emit(Op::GetGlobal(len_name));
                self.chunk.emit(Op::GetLocal(val_slot));
                self.chunk.emit(Op::Call(1));
                let want = self.chunk.add_constant(Value::Int(patterns.len() as i64));
                self.chunk.emit(Op::Constant(want));
                self.chunk.emit(Op::Eq);
                miss_jumps.push(self.chunk.emit(Op::JumpIfFalse(0)));
                self.chunk.emit(Op::Pop);

                for (i, p) in patterns.iter().enumerate() {
                    self.chunk.emit(Op::GetLocal(val_slot));
                    let idx = self.chunk.add_constant(Value::Int(i as i64));
                    self.chunk.emit(Op::Constant(idx));
                    self.chunk.emit(Op::GetIndex);
                    let elem_slot = self.add_local(format!("__elem_{i}"));
                    self.chunk.emit(Op::SetLocal(elem_slot));
                    self.chunk.emit(Op::Pop);
                    self.compile_pattern_test(elem_slot, p, miss_jumps)?;
                }
                Ok(())
            }
            Pattern::Variant {
                ty,
                variant,
                fields,
                ..
            } => {
                let tag_val = if let Some(t) = ty {
                    self.names
                        .enums
                        .get(&t.name)
                        .and_then(|vs| vs.iter().position(|n| n == &variant.name))
                        .unwrap_or(0) as i64
                } else {
                    self.names
                        .enums
                        .values()
                        .find_map(|vs| vs.iter().position(|n| n == &variant.name))
                        .unwrap_or(0) as i64
                };
                self.chunk.emit(Op::GetLocal(val_slot));
                let tag_index = self.chunk.add_constant(Value::Int(0));
                self.chunk.emit(Op::Constant(tag_index));
                self.chunk.emit(Op::GetIndex);
                let want = self.chunk.add_constant(Value::Int(tag_val));
                self.chunk.emit(Op::Constant(want));
                self.chunk.emit(Op::Eq);
                miss_jumps.push(self.chunk.emit(Op::JumpIfFalse(0)));
                self.chunk.emit(Op::Pop);

                for (fi, field_pat) in fields.iter().enumerate() {
                    self.chunk.emit(Op::GetLocal(val_slot));
                    let idx = self.chunk.add_constant(Value::Int((fi + 1) as i64));
                    self.chunk.emit(Op::Constant(idx));
                    self.chunk.emit(Op::GetIndex);
                    let f_slot = self.add_local(format!("__field_{fi}"));
                    self.chunk.emit(Op::SetLocal(f_slot));
                    self.chunk.emit(Op::Pop);
                    self.compile_pattern_test(f_slot, field_pat, miss_jumps)?;
                }
                Ok(())
            }
        }
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
            local_types: type_names(program),
            imported_types: HashMap::new(),
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
    let local_types = type_names(&module.program);
    let mut imported_types = HashMap::new();
    let mut enums = enums_from(&module.program);
    for item in &module.program.items {
        if let Item::Import(import) = item {
            let alias = import_alias(import).to_string();
            exported.entry(alias.clone()).or_insert_with(HashSet::new);
            if let Some(imported) = graph.imported(module, import) {
                let module_name = imported.name.clone();
                imports.insert(alias.clone(), module_name.clone());
                for (item, origin) in graph.exported_items(imported) {
                    let origin_name = origin.name.clone();
                    match item {
                        Item::Fn(func) => {
                            let canonical = qualify(&origin_name, &func.name.name);
                            if graph.unqualified_import_is_unambiguous(module, &func.name.name) {
                                imported_fns.insert(func.name.name.clone(), canonical.clone());
                            }
                            imported_fns.insert(format!("{alias}.{}", func.name.name), canonical);
                            exported
                                .get_mut(&alias)
                                .unwrap()
                                .insert(func.name.name.clone());
                        }
                        Item::Enum(en) => {
                            let variants: Vec<_> =
                                en.variants.iter().map(|v| v.name.name.clone()).collect();
                            enums.insert(format!("{alias}.{}", en.name.name), variants.clone());
                            enums.insert(qualify(&origin_name, &en.name.name), variants.clone());
                            if graph.unqualified_import_is_unambiguous(module, &en.name.name) {
                                enums.insert(en.name.name.clone(), variants);
                            }
                        }
                        Item::Struct(st) => {
                            imported_types.insert(
                                format!("{alias}.{}", st.name.name),
                                qualify(&origin_name, &st.name.name),
                            );
                            imported_types.insert(
                                qualify(&origin_name, &st.name.name),
                                qualify(&origin_name, &st.name.name),
                            );
                            if graph.unqualified_import_is_unambiguous(module, &st.name.name) {
                                imported_types.insert(
                                    st.name.name.clone(),
                                    qualify(&origin_name, &st.name.name),
                                );
                            }
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
        local_types,
        imported_types,
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

fn type_names(program: &Program) -> HashSet<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(st) => Some(st.name.name.clone()),
            _ => None,
        })
        .collect()
}

fn compile_with_names(program: &Program, names: &Names) -> Result<Compiled, VmError> {
    let consts = flake_ast::collect_const_values(program).unwrap_or_default();
    let mut functions = Vec::new();
    for item in &program.items {
        match item {
            Item::Fn(func) => functions.push(compile_fn(func, names, &consts)?),
            Item::Impl(imp) => {
                let type_ctor = impl_type_ctor(&imp.ty);
                for method in &imp.methods {
                    functions.push(compile_method(method, &type_ctor, names, &consts)?);
                }
            }
            Item::Import(_)
            | Item::Struct(_)
            | Item::Type(_)
            | Item::Enum(_)
            | Item::Trait(_)
            | Item::Const(_) => {}
        }
    }
    Ok(Compiled { functions })
}

fn compile_fn(
    func: &FnDecl,
    names: &Names,
    consts: &HashMap<String, flake_ast::ConstValue>,
) -> Result<Function, VmError> {
    let params: Vec<String> = func.params.iter().map(|p| p.name.name.clone()).collect();
    let arity = params.len() as u8;
    let mut compiler = FnCompiler::new(params, names, consts);
    compiler.chunk.set_span(func.span);
    compiler.compile_block_value(&func.body, false)?;
    compiler.chunk.emit(Op::Return);
    Ok(compiler.finish(names.global(&func.name.name), arity))
}

fn compile_method(
    func: &FnDecl,
    type_ctor: &str,
    names: &Names,
    consts: &HashMap<String, flake_ast::ConstValue>,
) -> Result<Function, VmError> {
    let params: Vec<String> = func.params.iter().map(|p| p.name.name.clone()).collect();
    let arity = params.len() as u8;
    let mut compiler = FnCompiler::new(params, names, consts);
    compiler.chunk.set_span(func.span);
    compiler.compile_block_value(&func.body, false)?;
    compiler.chunk.emit(Op::Return);
    let fn_name = names.global(&format!("{type_ctor}_{}", func.name.name));
    Ok(compiler.finish(fn_name, arity))
}

fn impl_type_ctor(ty: &flake_ast::TypeExpr) -> String {
    match ty {
        flake_ast::TypeExpr::Named { name, .. } => name.name.clone(),
        flake_ast::TypeExpr::List { .. } => "List".to_string(),
        flake_ast::TypeExpr::Dyn { .. } => "dyn".to_string(),
        flake_ast::TypeExpr::Optional { inner, .. } => impl_type_ctor(inner),
        flake_ast::TypeExpr::Owned { inner, .. } => impl_type_ctor(inner),
        flake_ast::TypeExpr::Ref { inner, .. } => impl_type_ctor(inner),
        flake_ast::TypeExpr::Mut { inner, .. } => impl_type_ctor(inner),
        flake_ast::TypeExpr::Fn { .. } => "Function".to_string(),
    }
}
