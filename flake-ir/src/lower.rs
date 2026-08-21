//! AST → Flake IR.

use std::collections::{HashMap, HashSet};

use flake_ast::{
    AssignOp, BinOp as AstBin, Block as AstBlock, Expr, FnDecl, InterpPart, Item, Literal, Program,
    Source, Stmt, TypeExpr, UnOp as AstUn,
};
use flake_parser::{import_alias, load_graph, qualify, ModuleGraph};

use crate::error::IrError;
use crate::ir::{
    BasicBlock, BinOp, BlockId, Callee, Const, Function, Inst, Local, LocalId, Module, StructDef,
    UnOp,
};
use crate::ty::IrType;

struct LoopCtx {
    continue_block: BlockId,
    break_block: BlockId,
}

struct Builder {
    locals: Vec<Local>,
    blocks: Vec<BasicBlock>,
    current: BlockId,
    loops: Vec<LoopCtx>,
    next_local: u32,
    next_block: u32,
    names: Names,
}

impl Builder {
    fn new(names: Names) -> Self {
        let entry = BlockId(0);
        Self {
            locals: Vec::new(),
            blocks: vec![BasicBlock {
                id: entry,
                insts: Vec::new(),
            }],
            current: entry,
            loops: Vec::new(),
            next_local: 0,
            next_block: 1,
            names,
        }
    }

    fn alloc(&mut self, name: Option<String>, ty: IrType) -> LocalId {
        let id = LocalId(self.next_local);
        self.next_local += 1;
        self.locals.push(Local { id, name, ty });
        id
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        self.blocks.push(BasicBlock {
            id,
            insts: Vec::new(),
        });
        id
    }

    fn switch(&mut self, id: BlockId) {
        self.current = id;
    }

    fn emit(&mut self, inst: Inst) {
        let block = self
            .blocks
            .iter_mut()
            .find(|b| b.id == self.current)
            .expect("current block");
        if block.insts.last().is_some_and(Inst::is_terminator) {
            return;
        }
        block.insts.push(inst);
    }

    fn sealed(&self) -> bool {
        self.blocks
            .iter()
            .find(|b| b.id == self.current)
            .is_some_and(|b| b.insts.last().is_some_and(Inst::is_terminator))
    }

    fn const_val(&mut self, value: Const, ty: IrType) -> LocalId {
        let dest = self.alloc(None, ty);
        self.emit(Inst::LoadConst { dest, value });
        dest
    }

    fn nil(&mut self) -> LocalId {
        self.const_val(Const::Nil, IrType::Nil)
    }
}

pub fn lower(source: &Source) -> Result<Module, IrError> {
    let graph = load_graph(source)?;
    Ok(lower_graph(&graph))
}

pub fn lower_program(name: &str, program: &Program) -> Module {
    lower_program_with(
        name,
        program,
        &Names {
            prefix: None,
            imports: HashMap::new(),
            local_fns: fn_names(program),
            imported_fns: HashMap::new(),
        },
    )
}

pub fn lower_graph(graph: &ModuleGraph) -> Module {
    let mut structs = Vec::new();
    let mut functions = Vec::new();
    let entry = graph.entry().name.as_str();
    for module in &graph.modules {
        let names = names_for(graph, module, module.name == entry);
        let part = lower_program_with(&module.name, &module.program, &names);
        structs.extend(part.structs);
        functions.extend(part.functions);
    }
    Module {
        name: graph.entry().name.clone(),
        functions,
        structs,
    }
}

#[derive(Clone)]
struct Names {
    prefix: Option<String>,
    imports: HashMap<String, String>,
    local_fns: HashSet<String>,
    imported_fns: HashMap<String, String>,
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
        self.imports.get(alias).map(|module| qualify(module, field))
    }
}

fn fn_names(program: &Program) -> HashSet<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Fn(f) => Some(f.name.name.clone()),
            _ => None,
        })
        .collect()
}

fn names_for(graph: &ModuleGraph, module: &flake_parser::LoadedModule, is_entry: bool) -> Names {
    let local_fns = fn_names(&module.program);
    let mut imports = HashMap::new();
    let mut imported_fns = HashMap::new();
    for item in &module.program.items {
        if let Item::Import(import) = item {
            let module_name = import.path.name.clone();
            imports.insert(import_alias(import).to_string(), module_name.clone());
            if let Some(imported) = graph.get(&module_name) {
                for item in &imported.program.items {
                    if let Item::Fn(func) = item {
                        imported_fns
                            .entry(func.name.name.clone())
                            .or_insert_with(|| qualify(&module_name, &func.name.name));
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
    }
}

fn lower_program_with(name: &str, program: &Program, names: &Names) -> Module {
    let mut structs = Vec::new();
    let mut functions = Vec::new();
    for item in &program.items {
        match item {
            Item::Struct(st) => {
                structs.push(StructDef {
                    name: st.name.name.clone(),
                    fields: st
                        .fields
                        .iter()
                        .map(|f| (f.name.name.clone(), lower_type(Some(&f.ty))))
                        .collect(),
                });
            }
            Item::Fn(func) => functions.push(lower_fn(func, names)),
            Item::Type(_) | Item::Import(_) => {}
        }
    }
    Module {
        name: name.to_string(),
        functions,
        structs,
    }
}

fn lower_fn(func: &FnDecl, names: &Names) -> Function {
    let mut b = Builder::new(names.clone());
    let mut params = Vec::new();
    for p in &func.params {
        let ty = lower_type(p.ty.as_ref());
        params.push(b.alloc(Some(p.name.name.clone()), ty));
    }
    let ret = lower_type(func.return_type.as_ref());
    let result = lower_block_value(&mut b, &func.body);
    if !b.sealed() {
        b.emit(Inst::Return {
            value: Some(result),
        });
    }
    for block in &mut b.blocks {
        if !block.insts.last().is_some_and(Inst::is_terminator) {
            block.insts.push(Inst::Return { value: None });
        }
    }

    Function {
        name: names.global(&func.name.name),
        params,
        ret,
        effects: func.effects.names().map(str::to_string).collect(),
        effects_specified: func.effects.specified,
        strict: func.strict,
        owned: func.owned,
        locals: b.locals,
        blocks: b.blocks,
        entry: BlockId(0),
    }
}

fn lower_block_value(b: &mut Builder, block: &AstBlock) -> LocalId {
    for stmt in &block.stmts {
        lower_stmt(b, stmt);
        if b.sealed() {
            return b.nil();
        }
    }
    if let Some(tail) = &block.tail {
        lower_expr(b, tail)
    } else {
        b.nil()
    }
}

fn lower_block_stmt(b: &mut Builder, block: &AstBlock) {
    let _ = lower_block_value(b, block);
}

fn lower_stmt(b: &mut Builder, stmt: &Stmt) {
    match stmt {
        Stmt::Let(s) | Stmt::Var(s) => {
            let val = lower_expr(b, &s.value);
            let ty =
                s.ty.as_ref()
                    .map(|t| lower_type(Some(t)))
                    .unwrap_or_else(|| {
                        b.locals
                            .iter()
                            .find(|l| l.id == val)
                            .map(|l| l.ty.clone())
                            .unwrap_or(IrType::Unknown)
                    });
            let dest = b.alloc(Some(s.name.name.clone()), ty);
            b.emit(Inst::Move { dest, src: val });
        }
        Stmt::Return { value, .. } => {
            let v = match value {
                Some(e) => Some(lower_expr(b, e)),
                None => None,
            };
            b.emit(Inst::Return { value: v });
        }
        Stmt::Expr(e) => {
            let _ = lower_expr(b, e);
        }
        Stmt::Break { .. } => {
            if let Some(ctx) = b.loops.last() {
                let target = ctx.break_block;
                b.emit(Inst::Jump { target });
            }
        }
        Stmt::Continue { .. } => {
            if let Some(ctx) = b.loops.last() {
                let target = ctx.continue_block;
                b.emit(Inst::Jump { target });
            }
        }
        Stmt::While { cond, body, .. } => lower_while(b, cond, body),
        Stmt::Loop { body, .. } => lower_loop(b, body),
        Stmt::For {
            name, iter, body, ..
        } => lower_for(b, &name.name, iter, body),
    }
}

fn lower_while(b: &mut Builder, cond: &Expr, body: &AstBlock) {
    let header = b.new_block();
    let body_b = b.new_block();
    let exit = b.new_block();
    b.emit(Inst::Jump { target: header });
    b.switch(header);
    let c = lower_expr(b, cond);
    b.emit(Inst::Branch {
        cond: c,
        then_block: body_b,
        else_block: exit,
    });
    b.loops.push(LoopCtx {
        continue_block: header,
        break_block: exit,
    });
    b.switch(body_b);
    lower_block_stmt(b, body);
    if !b.sealed() {
        b.emit(Inst::Jump { target: header });
    }
    b.loops.pop();
    b.switch(exit);
}

fn lower_loop(b: &mut Builder, body: &AstBlock) {
    let start = b.new_block();
    let exit = b.new_block();
    b.emit(Inst::Jump { target: start });
    b.loops.push(LoopCtx {
        continue_block: start,
        break_block: exit,
    });
    b.switch(start);
    lower_block_stmt(b, body);
    if !b.sealed() {
        b.emit(Inst::Jump { target: start });
    }
    b.loops.pop();
    b.switch(exit);
}

fn lower_for(b: &mut Builder, name: &str, iter: &Expr, body: &AstBlock) {
    let src = lower_expr(b, iter);
    let it = b.alloc(None, IrType::Iter);
    b.emit(Inst::MakeIter { dest: it, src });
    let header = b.new_block();
    let body_b = b.new_block();
    let exit = b.new_block();
    b.emit(Inst::Jump { target: header });
    b.switch(header);
    let value = b.alloc(Some(name.to_string()), IrType::Dyn);
    let more = b.alloc(None, IrType::Bool);
    b.emit(Inst::IterNext {
        value,
        more,
        iter: it,
    });
    b.emit(Inst::Branch {
        cond: more,
        then_block: body_b,
        else_block: exit,
    });
    b.loops.push(LoopCtx {
        continue_block: header,
        break_block: exit,
    });
    b.switch(body_b);
    lower_block_stmt(b, body);
    if !b.sealed() {
        b.emit(Inst::Jump { target: header });
    }
    b.loops.pop();
    b.switch(exit);
}

fn lower_expr(b: &mut Builder, expr: &Expr) -> LocalId {
    match expr {
        Expr::Literal { value, .. } => match value {
            Literal::Nil => b.const_val(Const::Nil, IrType::Nil),
            Literal::Bool(v) => b.const_val(Const::Bool(*v), IrType::Bool),
            Literal::Int(n) => b.const_val(Const::Int(*n), IrType::Int),
            Literal::Float(n) => b.const_val(Const::Float(*n), IrType::Float),
            Literal::String(s) => b.const_val(Const::String(s.clone()), IrType::String),
        },
        Expr::Ident(id) => lookup(b, &id.name).unwrap_or_else(|| {
            // treat as global / native / function reference
            b.alloc(Some(id.name.clone()), IrType::Func)
        }),
        Expr::Interpolated { parts, .. } => {
            let mut items = Vec::new();
            for part in parts {
                match part {
                    InterpPart::Text(t) => {
                        items.push(b.const_val(Const::String(t.clone()), IrType::String));
                    }
                    InterpPart::Expr(e) => items.push(lower_expr(b, e)),
                }
            }
            let dest = b.alloc(None, IrType::String);
            b.emit(Inst::Concat { dest, parts: items });
            dest
        }
        Expr::List { elements, .. } => {
            let items: Vec<_> = elements.iter().map(|e| lower_expr(b, e)).collect();
            let dest = b.alloc(None, IrType::List(Box::new(IrType::Dyn)));
            b.emit(Inst::MakeList { dest, items });
            dest
        }
        Expr::Map { entries, .. } => {
            let mut keys = Vec::new();
            let mut values = Vec::new();
            for (k, v) in entries {
                keys.push(lower_expr(b, k));
                values.push(lower_expr(b, v));
            }
            let dest = b.alloc(
                None,
                IrType::Map(Box::new(IrType::Dyn), Box::new(IrType::Dyn)),
            );
            b.emit(Inst::MakeMap { dest, keys, values });
            dest
        }
        Expr::StructInit { name, fields, .. } => {
            let fields: Vec<_> = fields
                .iter()
                .map(|(f, v)| (f.name.clone(), lower_expr(b, v)))
                .collect();
            let dest = b.alloc(None, IrType::Struct(name.name.clone()));
            b.emit(Inst::MakeStruct {
                dest,
                name: name.name.clone(),
                fields,
            });
            dest
        }
        Expr::Unary { op, expr, .. } => {
            let src = lower_expr(b, expr);
            match op {
                AstUn::Neg => {
                    let dest = b.alloc(None, IrType::Int);
                    b.emit(Inst::Unary {
                        dest,
                        op: UnOp::Neg,
                        src,
                    });
                    dest
                }
                AstUn::Not => {
                    let dest = b.alloc(None, IrType::Bool);
                    b.emit(Inst::Unary {
                        dest,
                        op: UnOp::Not,
                        src,
                    });
                    dest
                }
                AstUn::Ref | AstUn::RefMut => src,
            }
        }
        Expr::Binary {
            op, left, right, ..
        } => lower_binary(b, *op, left, right),
        Expr::Range { start, end, .. } => {
            let s = lower_expr(b, start);
            let e = lower_expr(b, end);
            let dest = b.alloc(None, IrType::Range);
            b.emit(Inst::MakeRange {
                dest,
                start: s,
                end: e,
            });
            dest
        }
        Expr::Assign {
            op, target, value, ..
        } => lower_assign(b, *op, target, value),
        Expr::Call { callee, args, .. } => {
            let arg_ids: Vec<_> = args.iter().map(|a| lower_expr(b, a)).collect();
            let callee = match callee.as_ref() {
                Expr::Ident(id) => {
                    if lookup(b, &id.name).is_some() {
                        Callee::Local(lookup(b, &id.name).unwrap())
                    } else {
                        Callee::Static(b.names.global(&id.name))
                    }
                }
                Expr::Field { target, field, .. } => {
                    if let Expr::Ident(id) = target.as_ref() {
                        if let Some(global) = b.names.field_global(&id.name, &field.name) {
                            Callee::Static(global)
                        } else {
                            Callee::Local(lower_expr(b, callee))
                        }
                    } else {
                        Callee::Local(lower_expr(b, callee))
                    }
                }
                other => Callee::Local(lower_expr(b, other)),
            };
            let dest_ty = match &callee {
                Callee::Static(name) => native_result_ty(name),
                Callee::Local(_) => IrType::Dyn,
            };
            let dest = b.alloc(None, dest_ty);
            b.emit(Inst::Call {
                dest: Some(dest),
                callee,
                args: arg_ids,
            });
            dest
        }
        Expr::Index { target, index, .. } => {
            let obj = lower_expr(b, target);
            let idx = lower_expr(b, index);
            let dest_ty = match b.locals.iter().find(|l| l.id == obj).map(|l| &l.ty) {
                Some(IrType::String) => IrType::String,
                Some(IrType::List(elem)) => (**elem).clone(),
                Some(IrType::Map(_, value)) => (**value).clone(),
                _ => IrType::Dyn,
            };
            let dest = b.alloc(None, dest_ty);
            b.emit(Inst::GetIndex {
                dest,
                obj,
                index: idx,
            });
            dest
        }
        Expr::Field { target, field, .. } => {
            let obj = lower_expr(b, target);
            let dest = b.alloc(None, IrType::Dyn);
            b.emit(Inst::GetField {
                dest,
                obj,
                field: field.name.clone(),
            });
            dest
        }
        Expr::If {
            cond,
            then_block,
            else_block,
            ..
        } => lower_if(b, cond, then_block, else_block.as_deref()),
        Expr::Block(block) => lower_block_value(b, block),
    }
}

fn lower_if(
    b: &mut Builder,
    cond: &Expr,
    then_block: &AstBlock,
    else_block: Option<&Expr>,
) -> LocalId {
    let c = lower_expr(b, cond);
    let t_bb = b.new_block();
    let e_bb = b.new_block();
    let join = b.new_block();
    let dest = b.alloc(None, IrType::Dyn);
    b.emit(Inst::Branch {
        cond: c,
        then_block: t_bb,
        else_block: e_bb,
    });
    b.switch(t_bb);
    let tv = lower_block_value(b, then_block);
    if !b.sealed() {
        b.emit(Inst::Move { dest, src: tv });
        b.emit(Inst::Jump { target: join });
    }
    b.switch(e_bb);
    let ev = match else_block {
        Some(e) => lower_expr(b, e),
        None => b.nil(),
    };
    if !b.sealed() {
        b.emit(Inst::Move { dest, src: ev });
        b.emit(Inst::Jump { target: join });
    }
    b.switch(join);
    dest
}

fn lower_binary(b: &mut Builder, op: AstBin, left: &Expr, right: &Expr) -> LocalId {
    if matches!(op, AstBin::And | AstBin::Or) {
        return lower_short_circuit(b, op, left, right);
    }
    let lhs = lower_expr(b, left);
    let rhs = lower_expr(b, right);
    let dest = b.alloc(
        None,
        match op {
            AstBin::Eq | AstBin::Ne | AstBin::Lt | AstBin::Le | AstBin::Gt | AstBin::Ge => {
                IrType::Bool
            }
            _ => IrType::Int,
        },
    );
    b.emit(Inst::Binary {
        dest,
        op: ast_bin(op),
        lhs,
        rhs,
    });
    dest
}

fn lower_short_circuit(b: &mut Builder, op: AstBin, left: &Expr, right: &Expr) -> LocalId {
    let dest = b.alloc(None, IrType::Bool);
    let lhs = lower_expr(b, left);
    let rhs_bb = b.new_block();
    let join = b.new_block();
    match op {
        AstBin::And => {
            b.emit(Inst::Move { dest, src: lhs });
            b.emit(Inst::Branch {
                cond: lhs,
                then_block: rhs_bb,
                else_block: join,
            });
        }
        AstBin::Or => {
            b.emit(Inst::Move { dest, src: lhs });
            b.emit(Inst::Branch {
                cond: lhs,
                then_block: join,
                else_block: rhs_bb,
            });
        }
        _ => unreachable!(),
    }
    b.switch(rhs_bb);
    let rhs = lower_expr(b, right);
    b.emit(Inst::Move { dest, src: rhs });
    b.emit(Inst::Jump { target: join });
    b.switch(join);
    dest
}

fn lower_assign(b: &mut Builder, op: AssignOp, target: &Expr, value: &Expr) -> LocalId {
    let rhs = lower_expr(b, value);
    let stored = if op == AssignOp::Assign {
        rhs
    } else {
        let cur = lower_lvalue_load(b, target);
        let dest = b.alloc(None, IrType::Dyn);
        b.emit(Inst::Binary {
            dest,
            op: assign_bin(op),
            lhs: cur,
            rhs,
        });
        dest
    };
    lower_lvalue_store(b, target, stored);
    stored
}

fn lower_lvalue_load(b: &mut Builder, target: &Expr) -> LocalId {
    match target {
        Expr::Ident(id) => {
            lookup(b, &id.name).unwrap_or_else(|| b.alloc(Some(id.name.clone()), IrType::Dyn))
        }
        Expr::Index { target, index, .. } => {
            let obj = lower_expr(b, target);
            let idx = lower_expr(b, index);
            let dest = b.alloc(None, IrType::Dyn);
            b.emit(Inst::GetIndex {
                dest,
                obj,
                index: idx,
            });
            dest
        }
        Expr::Field { target, field, .. } => {
            let obj = lower_expr(b, target);
            let dest = b.alloc(None, IrType::Dyn);
            b.emit(Inst::GetField {
                dest,
                obj,
                field: field.name.clone(),
            });
            dest
        }
        _ => b.nil(),
    }
}

fn lower_lvalue_store(b: &mut Builder, target: &Expr, value: LocalId) {
    match target {
        Expr::Ident(id) => {
            if let Some(dest) = lookup(b, &id.name) {
                b.emit(Inst::Move { dest, src: value });
            }
        }
        Expr::Index { target, index, .. } => {
            let obj = lower_expr(b, target);
            let idx = lower_expr(b, index);
            b.emit(Inst::SetIndex {
                obj,
                index: idx,
                value,
            });
        }
        Expr::Field { target, field, .. } => {
            let obj = lower_expr(b, target);
            b.emit(Inst::SetField {
                obj,
                field: field.name.clone(),
                value,
            });
        }
        _ => {}
    }
}

fn lookup(b: &Builder, name: &str) -> Option<LocalId> {
    b.locals
        .iter()
        .rev()
        .find(|l| l.name.as_deref() == Some(name))
        .map(|l| l.id)
}

fn ast_bin(op: AstBin) -> BinOp {
    match op {
        AstBin::Add => BinOp::Add,
        AstBin::Sub => BinOp::Sub,
        AstBin::Mul => BinOp::Mul,
        AstBin::Div => BinOp::Div,
        AstBin::Rem => BinOp::Rem,
        AstBin::Eq => BinOp::Eq,
        AstBin::Ne => BinOp::Ne,
        AstBin::Lt => BinOp::Lt,
        AstBin::Le => BinOp::Le,
        AstBin::Gt => BinOp::Gt,
        AstBin::Ge => BinOp::Ge,
        AstBin::And => BinOp::And,
        AstBin::Or => BinOp::Or,
    }
}

fn assign_bin(op: AssignOp) -> BinOp {
    match op {
        AssignOp::Assign => unreachable!(),
        AssignOp::AddAssign => BinOp::Add,
        AssignOp::SubAssign => BinOp::Sub,
        AssignOp::MulAssign => BinOp::Mul,
        AssignOp::DivAssign => BinOp::Div,
        AssignOp::RemAssign => BinOp::Rem,
    }
}

fn lower_type(ty: Option<&TypeExpr>) -> IrType {
    match ty {
        None => IrType::Unknown,
        Some(TypeExpr::Dyn { .. }) => IrType::Dyn,
        Some(TypeExpr::Named { name, .. }) => match name.name.as_str() {
            "Int" => IrType::Int,
            "Float" => IrType::Float,
            "Bool" => IrType::Bool,
            "String" => IrType::String,
            "Nil" | "Unit" => IrType::Nil,
            "Range" => IrType::Range,
            other => IrType::Struct(other.to_string()),
        },
        Some(TypeExpr::List { element, .. }) => IrType::List(Box::new(lower_type(Some(element)))),
        Some(TypeExpr::Owned { inner, .. })
        | Some(TypeExpr::Mut { inner, .. })
        | Some(TypeExpr::Ref { inner, .. })
        | Some(TypeExpr::Optional { inner, .. }) => lower_type(Some(inner)),
        Some(TypeExpr::Fn { .. }) => IrType::Func,
    }
}

fn native_result_ty(name: &str) -> IrType {
    match name {
        "print" | "push" | "assert" => IrType::Nil,
        "len" | "int" => IrType::Int,
        "str" | "join" | "type_of" | "read_file" => IrType::String,
        "range" => IrType::Range,
        "split" => IrType::List(Box::new(IrType::String)),
        "float" => IrType::Float,
        _ => IrType::Dyn,
    }
}
