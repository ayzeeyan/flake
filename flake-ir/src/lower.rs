//! AST → Flake IR.

use std::collections::{HashMap, HashSet};

use flake_ast::{
    AssignOp, BinOp as AstBin, Block as AstBlock, Expr, FnDecl, InterpPart, Item, Literal, Program,
    Source, Stmt, TypeExpr, UnOp as AstUn,
};
use flake_parser::{ModuleGraph, import_alias, load_graph, qualify};

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

type EnumVariants = Vec<(String, Vec<IrType>)>;
type EnumTable = HashMap<String, EnumVariants>;
type StructFields = Vec<(String, IrType)>;
type StructTable = HashMap<String, StructFields>;

struct Builder {
    locals: Vec<Local>,
    blocks: Vec<BasicBlock>,
    current: BlockId,
    loops: Vec<LoopCtx>,
    next_local: u32,
    next_block: u32,
    names: Names,
    fn_rets: HashMap<String, IrType>,
    enums: EnumTable,
    structs: StructTable,
}

impl Builder {
    fn new(
        names: Names,
        fn_rets: HashMap<String, IrType>,
        enums: EnumTable,
        structs: StructTable,
    ) -> Self {
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
            fn_rets,
            enums,
            structs,
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
    let names = Names {
        prefix: None,
        imports: HashMap::new(),
        local_fns: fn_names(program),
        imported_fns: HashMap::new(),
        exported: HashMap::new(),
        local_types: type_names(program),
        imported_types: HashMap::new(),
    };
    let mut fn_rets = HashMap::new();
    for item in &program.items {
        if let Item::Fn(func) = item {
            fn_rets.insert(
                names.global(&func.name.name),
                names.resolve_type(lower_type(func.return_type.as_ref())),
            );
        }
    }
    let mut module = lower_program_with(
        name,
        program,
        &names,
        &fn_rets,
        &enums_of(program),
        &structs_of(program),
    );
    crate::opt::optimize(&mut module);
    module
}

pub fn lower_graph(graph: &ModuleGraph) -> Module {
    let mut structs = Vec::new();
    let mut functions = Vec::new();
    let entry = graph.entry().name.as_str();
    let fn_rets = collect_fn_rets(graph);
    for module in &graph.modules {
        let names = names_for(graph, module, module.name == entry);
        let enums = enums_for(graph, module, &names);
        let struct_table = structs_for(graph, module, &names);
        let part = lower_program_with(
            &module.name,
            &module.program,
            &names,
            &fn_rets,
            &enums,
            &struct_table,
        );
        structs.extend(part.structs);
        functions.extend(part.functions);
    }
    let mut module = Module {
        name: graph.entry().name.clone(),
        functions,
        structs,
    };
    crate::opt::optimize(&mut module);
    module
}

#[derive(Clone)]
struct Names {
    prefix: Option<String>,
    imports: HashMap<String, String>,
    local_fns: HashSet<String>,
    imported_fns: HashMap<String, String>,
    exported: HashMap<String, HashSet<String>>,
    local_types: HashSet<String>,
    imported_types: HashMap<String, String>,
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

    fn function_global(&self, name: &str) -> Option<String> {
        if self.local_fns.contains(name) {
            return Some(self.global(name));
        }
        self.imported_fns.get(name).cloned()
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

    fn resolve_type(&self, ty: IrType) -> IrType {
        match ty {
            IrType::Struct(name) => IrType::Struct(self.type_global(&name)),
            IrType::List(element) => IrType::List(Box::new(self.resolve_type(*element))),
            IrType::Map(key, value) => IrType::Map(
                Box::new(self.resolve_type(*key)),
                Box::new(self.resolve_type(*value)),
            ),
            IrType::Func(ret) => IrType::Func(Box::new(self.resolve_type(*ret))),
            other => other,
        }
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

fn type_names(program: &Program) -> HashSet<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(st) => Some(st.name.name.clone()),
            Item::Enum(en) => Some(en.name.name.clone()),
            _ => None,
        })
        .collect()
}

fn names_for(graph: &ModuleGraph, module: &flake_parser::LoadedModule, is_entry: bool) -> Names {
    let local_fns = fn_names(&module.program);
    let mut imports = HashMap::new();
    let mut imported_fns = HashMap::new();
    let mut exported = HashMap::new();
    let local_types = type_names(&module.program);
    let mut imported_types = HashMap::new();
    for item in &module.program.items {
        if let Item::Import(import) = item {
            let alias = import_alias(import).to_string();
            exported.entry(alias.clone()).or_insert_with(HashSet::new);
            if let Some(imported) = graph.imported(module, import) {
                let module_name = imported.name.clone();
                imports.insert(alias.clone(), module_name.clone());
                for (item, origin) in graph.exported_items(imported) {
                    let origin_name = origin.name.clone();
                    if let Item::Fn(func) = item {
                        let canonical = qualify(&origin_name, &func.name.name);
                        if graph.unqualified_import_is_unambiguous(module, &func.name.name) {
                            imported_fns.insert(
                                func.name.name.clone(),
                                canonical.clone(),
                            );
                        }
                        imported_fns.insert(
                            format!("{alias}.{}", func.name.name),
                            canonical,
                        );
                        exported
                            .get_mut(&alias)
                            .unwrap()
                            .insert(func.name.name.clone());
                    }
                    let imported_type = match item {
                        Item::Struct(st) => Some(&st.name.name),
                        Item::Enum(en) => Some(&en.name.name),
                        _ => None,
                    };
                    if let Some(name) = imported_type {
                        imported_types.insert(format!("{alias}.{name}"), qualify(&origin_name, name));
                        imported_types.insert(qualify(&origin_name, name), qualify(&origin_name, name));
                        if graph.unqualified_import_is_unambiguous(module, name) {
                            imported_types.insert(name.clone(), qualify(&origin_name, name));
                        }
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
    }
}

fn collect_fn_rets(graph: &ModuleGraph) -> HashMap<String, IrType> {
    let mut fn_rets = HashMap::new();
    let entry = graph.entry().name.as_str();
    for module in &graph.modules {
        let names = names_for(graph, module, module.name == entry);
        for item in &module.program.items {
            if let Item::Fn(func) = item {
                fn_rets.insert(
                    names.global(&func.name.name),
                    names.resolve_type(lower_type(func.return_type.as_ref())),
                );
            }
        }
    }
    fn_rets
}

fn lower_program_with(
    name: &str,
    program: &Program,
    names: &Names,
    fn_rets: &HashMap<String, IrType>,
    enums: &EnumTable,
    structs: &StructTable,
) -> Module {
    let mut module_structs = Vec::new();
    let mut functions = Vec::new();
    for item in &program.items {
        match item {
            Item::Struct(st) => {
                module_structs.push(StructDef {
                    name: names.type_global(&st.name.name),
                    fields: st
                        .fields
                        .iter()
                        .map(|f| {
                            (
                                f.name.name.clone(),
                                names.resolve_type(lower_type(Some(&f.ty))),
                            )
                        })
                        .collect(),
                });
            }
            Item::Fn(func) => functions.push(lower_fn(func, names, fn_rets, enums, structs)),
            Item::Type(_) | Item::Import(_) | Item::Enum(_) => {}
        }
    }
    Module {
        name: name.to_string(),
        functions,
        structs: module_structs,
    }
}

fn structs_of(program: &Program) -> StructTable {
    let mut structs = HashMap::new();
    for item in &program.items {
        if let Item::Struct(st) = item {
            structs.insert(
                st.name.name.clone(),
                st.fields
                    .iter()
                    .map(|f| (f.name.name.clone(), lower_type(Some(&f.ty))))
                    .collect(),
            );
        }
    }
    structs
}

fn structs_for(
    graph: &ModuleGraph,
    module: &flake_parser::LoadedModule,
    names: &Names,
) -> StructTable {
    let mut structs = structs_of(&module.program);
    for item in &module.program.items {
        if let Item::Struct(st) = item {
            if let Some(fields) = structs.get(&st.name.name).cloned() {
                let resolved: Vec<_> = fields
                    .into_iter()
                    .map(|(name, ty)| (name, names.resolve_type(ty)))
                    .collect();
                structs.insert(names.type_global(&st.name.name), resolved);
            }
        }
    }
    for item in &module.program.items {
        if let Item::Import(import) = item {
            if let Some(imported) = graph.imported(module, import) {
                let alias = import_alias(import);
                for (item, origin) in graph.exported_items(imported) {
                    if let Item::Struct(st) = item {
                        let fields: Vec<_> = st
                            .fields
                            .iter()
                            .map(|f| {
                                (
                                     f.name.name.clone(),
                                     names.resolve_type(lower_type(Some(&f.ty))),
                                )
                            })
                            .collect();
                        structs.insert(format!("{alias}.{}", st.name.name), fields.clone());
                        structs.insert(qualify(&origin.name, &st.name.name), fields.clone());
                        if graph.unqualified_import_is_unambiguous(module, &st.name.name) {
                            structs.insert(st.name.name.clone(), fields);
                        }
                    }
                }
            }
        }
    }
    structs
}

fn enums_of(program: &Program) -> EnumTable {
    let mut enums = HashMap::new();
    for item in &program.items {
        if let Item::Enum(en) = item {
            enums.insert(
                en.name.name.clone(),
                en.variants
                    .iter()
                    .map(|v| {
                        (
                            v.name.name.clone(),
                            v.fields
                                .iter()
                                .map(|field| lower_type(Some(field)))
                                .collect(),
                        )
                    })
                    .collect(),
            );
        }
    }
    enums
}

fn enums_for(graph: &ModuleGraph, module: &flake_parser::LoadedModule, names: &Names) -> EnumTable {
    let mut enums = enums_of(&module.program);
    for item in &module.program.items {
        if let Item::Enum(en) = item {
            if let Some(variants) = enums.get(&en.name.name).cloned() {
                enums.insert(names.type_global(&en.name.name), variants);
            }
        }
    }
    for item in &module.program.items {
        if let Item::Import(import) = item {
            if let Some(imported) = graph.imported(module, import) {
                let alias = import_alias(import);
                for (item, origin) in graph.exported_items(imported) {
                    if let Item::Enum(en) = item {
                        let variants: EnumVariants = en
                            .variants
                            .iter()
                            .map(|v| {
                                (
                                    v.name.name.clone(),
                                    v.fields
                                        .iter()
                                        .map(|field| names.resolve_type(lower_type(Some(field))))
                                        .collect(),
                                )
                            })
                            .collect();
                        enums.insert(format!("{alias}.{}", en.name.name), variants.clone());
                        enums.insert(qualify(&origin.name, &en.name.name), variants.clone());
                        if graph.unqualified_import_is_unambiguous(module, &en.name.name) {
                            enums.insert(en.name.name.clone(), variants);
                        }
                    }
                }
            }
        }
    }
    enums
}

fn enum_variants<'a>(b: &'a Builder, target: &Expr) -> Option<(String, &'a EnumVariants)> {
    match target {
        Expr::Ident(id) => b
            .enums
            .get(&id.name)
            .map(|v| (b.names.type_global(&id.name), v)),
        Expr::Field { target, field, .. } => {
            if let Expr::Ident(module) = target.as_ref() {
                if b.names.imports.contains_key(&module.name) {
                    let qualified = format!("{}.{}", module.name, field.name);
                    return b
                        .enums
                        .get(&qualified)
                        .map(|v| (b.names.type_global(&qualified), v));
                }
            }
            None
        }
        _ => None,
    }
}

fn lower_fn(
    func: &FnDecl,
    names: &Names,
    fn_rets: &HashMap<String, IrType>,
    enums: &EnumTable,
    structs: &StructTable,
) -> Function {
    let mut b = Builder::new(
        names.clone(),
        fn_rets.clone(),
        enums.clone(),
        structs.clone(),
    );
    let mut params = Vec::new();
    for p in &func.params {
        let ty = lower_type(p.ty.as_ref());
        params.push(b.alloc(Some(p.name.name.clone()), names.resolve_type(ty)));
    }
    let ret = names.resolve_type(lower_type(func.return_type.as_ref()));
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
                    .map(|t| b.names.resolve_type(lower_type(Some(t))))
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
            let v = value.as_ref().map(|e| lower_expr(b, e));
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
            let name = b
                .names
                .function_global(&id.name)
                .unwrap_or_else(|| id.name.clone());
            let ret = b
                .fn_rets
                .get(&name)
                .cloned()
                .unwrap_or_else(|| native_result_ty(&name));
            let dest = b.alloc(Some(id.name.clone()), IrType::Func(Box::new(ret)));
            b.emit(Inst::LoadFunction { dest, name });
            dest
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
            let element_ty = common_local_type(b, &items);
            let dest = b.alloc(None, IrType::List(Box::new(element_ty)));
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
            let key_ty = common_local_type(b, &keys);
            let value_ty = common_local_type(b, &values);
            let dest = b.alloc(None, IrType::Map(Box::new(key_ty), Box::new(value_ty)));
            b.emit(Inst::MakeMap { dest, keys, values });
            dest
        }
        Expr::StructInit { name, fields, .. } => {
            let fields: Vec<_> = fields
                .iter()
                .map(|(f, v)| (f.name.clone(), lower_expr(b, v)))
                .collect();
            let type_name = b.names.type_global(&name.name);
            let dest = b.alloc(None, IrType::Struct(type_name.clone()));
            b.emit(Inst::MakeStruct {
                dest,
                name: type_name,
                fields,
            });
            dest
        }
        Expr::Unary { op, expr, .. } => {
            let src = lower_expr(b, expr);
            match op {
                AstUn::Neg => {
                    let ty = b
                        .locals
                        .iter()
                        .find(|local| local.id == src)
                        .map(|local| local.ty.clone())
                        .unwrap_or(IrType::Int);
                    let dest = b.alloc(None, ty);
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
            if let Expr::Field { target, field, .. } = callee.as_ref() {
                let found = enum_variants(b, target).and_then(|(ename, vars)| {
                    vars.iter()
                        .position(|(n, _)| n == &field.name)
                        .map(|tag| (ename, tag))
                });
                if let Some((ename, tag)) = found {
                    let mut items = vec![b.const_val(Const::Int(tag as i64), IrType::Int)];
                    items.extend(arg_ids);
                    let dest = b.alloc(None, IrType::Struct(ename));
                    b.emit(Inst::MakeList { dest, items });
                    return dest;
                }
            }
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
                Callee::Static(name) => {
                    let native = native_call_result_ty(b, name, &arg_ids);
                    if !matches!(native, IrType::Dyn) || is_native_name(name) {
                        native
                    } else {
                        b.fn_rets.get(name).cloned().unwrap_or(IrType::Dyn)
                    }
                }
                Callee::Local(id) => b
                    .locals
                    .iter()
                    .find(|local| local.id == *id)
                    .and_then(|local| match &local.ty {
                        IrType::Func(ret) => Some((**ret).clone()),
                        _ => None,
                    })
                    .unwrap_or(IrType::Dyn),
            };
            let dest = b.alloc(None, dest_ty);
            b.emit(Inst::Call {
                dest: Some(dest),
                callee,
                args: arg_ids,
            });
            dest
        }
        Expr::Spawn { call, .. } => {
            if let Expr::Call { callee, args, .. } = call.as_ref() {
                let arg_ids: Vec<_> = args.iter().map(|a| lower_expr(b, a)).collect();
                if let Expr::Field { target, field, .. } = callee.as_ref() {
                    let found = enum_variants(b, target).and_then(|(ename, vars)| {
                        vars.iter()
                            .position(|(n, _)| n == &field.name)
                            .map(|tag| (ename, tag))
                    });
                    if let Some((ename, tag)) = found {
                        let mut items = vec![b.const_val(Const::Int(tag as i64), IrType::Int)];
                        items.extend(arg_ids);
                        let val_dest = b.alloc(None, IrType::Struct(ename.clone()));
                        b.emit(Inst::MakeList { dest: val_dest, items });
                        let dest = b.alloc(None, IrType::Task(Box::new(IrType::Struct(ename))));
                        b.emit(Inst::Spawn {
                            dest,
                            callee: Callee::Local(val_dest),
                            args: vec![],
                        });
                        return dest;
                    }
                }
                let callee_lowered = match callee.as_ref() {
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
                let ret_ty = match &callee_lowered {
                    Callee::Static(name) => {
                        let native = native_call_result_ty(b, name, &arg_ids);
                        if !matches!(native, IrType::Dyn) || is_native_name(name) {
                            native
                        } else {
                            b.fn_rets.get(name).cloned().unwrap_or(IrType::Dyn)
                        }
                    }
                    Callee::Local(id) => b
                        .locals
                        .iter()
                        .find(|local| local.id == *id)
                        .and_then(|local| match &local.ty {
                            IrType::Func(ret) => Some((**ret).clone()),
                            _ => None,
                        })
                        .unwrap_or(IrType::Dyn),
                };
                let dest = b.alloc(None, IrType::Task(Box::new(ret_ty)));
                b.emit(Inst::Spawn {
                    dest,
                    callee: callee_lowered,
                    args: arg_ids,
                });
                dest
            } else {
                let val = lower_expr(b, call);
                let val_ty = b
                    .locals
                    .iter()
                    .find(|l| l.id == val)
                    .map(|l| l.ty.clone())
                    .unwrap_or(IrType::Dyn);
                let dest = b.alloc(None, IrType::Task(Box::new(val_ty)));
                b.emit(Inst::Spawn {
                    dest,
                    callee: Callee::Local(val),
                    args: vec![],
                });
                dest
            }
        }
        Expr::Await { task, .. } => {
            let task_id = lower_expr(b, task);
            let ret_ty = match b.locals.iter().find(|l| l.id == task_id).map(|l| &l.ty) {
                Some(IrType::Task(ret)) => (**ret).clone(),
                _ => IrType::Dyn,
            };
            let dest = b.alloc(None, ret_ty);
            b.emit(Inst::Await { dest, task: task_id });
            dest
        }
        Expr::Try { expr, .. } => lower_try(b, expr),
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
            let found = enum_variants(b, target).and_then(|(ename, vars)| {
                vars.iter()
                    .position(|(n, _)| n == &field.name)
                    .map(|tag| (ename, tag))
            });
            if let Some((ename, tag)) = found {
                let t = b.const_val(Const::Int(tag as i64), IrType::Int);
                let dest = b.alloc(None, IrType::Struct(ename));
                b.emit(Inst::MakeList {
                    dest,
                    items: vec![t],
                });
                return dest;
            }
            if let Expr::Ident(module) = target.as_ref() {
                if let Some(name) = b.names.field_global(&module.name, &field.name) {
                    if let Some(ret) = b.fn_rets.get(&name).cloned() {
                        let dest = b.alloc(
                            Some(format!("{}.{}", module.name, field.name)),
                            IrType::Func(Box::new(ret)),
                        );
                        b.emit(Inst::LoadFunction { dest, name });
                        return dest;
                    }
                }
            }
            let obj = lower_expr(b, target);
            let dest_ty = match b.locals.iter().find(|l| l.id == obj).map(|l| &l.ty) {
                Some(IrType::Struct(name)) => b
                    .structs
                    .get(name)
                    .and_then(|fields| fields.iter().find(|(n, _)| n == &field.name))
                    .map(|(_, ty)| ty.clone())
                    .unwrap_or(IrType::Dyn),
                _ => IrType::Dyn,
            };
            let dest = b.alloc(None, dest_ty);
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
        Expr::Block(block) | Expr::Nursery { body: block, .. } => lower_block_value(b, block),
        Expr::Match {
            scrutinee, arms, ..
        } => lower_match(b, scrutinee, arms),
    }
}

fn lower_match(b: &mut Builder, scrutinee: &Expr, arms: &[flake_ast::MatchArm]) -> LocalId {
    let src = lower_expr(b, scrutinee);
    let dest = b.alloc(None, IrType::Dyn);
    let n = b.nil();
    b.emit(Inst::Move { dest, src: n });
    let exit = b.new_block();
    for arm in arms {
        let is_catch_all = matches!(&arm.pattern, flake_ast::Pattern::Wildcard { .. })
            || matches!(&arm.pattern, flake_ast::Pattern::Ident(id) if id.name == "_" || (!id.name.chars().next().is_some_and(|c| c.is_uppercase()) && !b.enums.values().any(|vs| vs.iter().any(|(n, _)| n == &id.name))));

        let next_arm = b.new_block();
        lower_pattern_test(b, src, &arm.pattern, next_arm);
        let val = lower_expr(b, &arm.body);
        if !b.sealed() {
            b.emit(Inst::Move { dest, src: val });
            b.emit(Inst::Jump { target: exit });
        }
        if is_catch_all {
            break;
        }
        b.switch(next_arm);
    }
    if !b.sealed() {
        b.emit(Inst::Jump { target: exit });
    }
    b.switch(exit);
    dest
}

fn lower_pattern_test(
    b: &mut Builder,
    val: LocalId,
    pat: &flake_ast::Pattern,
    fail_block: BlockId,
) {
    match pat {
        flake_ast::Pattern::Wildcard { .. } => {}
        flake_ast::Pattern::Ident(id) => {
            if id.name == "_" {
                // do nothing
            } else if id.name.chars().next().is_some_and(|c| c.is_uppercase())
                && b.enums.values().any(|vs| vs.iter().any(|(n, _)| n == &id.name))
            {
                let tag_val = b
                    .enums
                    .values()
                    .find_map(|vs| vs.iter().position(|(n, _)| n == &id.name))
                    .unwrap_or(0) as i64;
                let zero = b.const_val(Const::Int(0), IrType::Int);
                let tag = b.alloc(None, IrType::Int);
                b.emit(Inst::GetIndex {
                    dest: tag,
                    obj: val,
                    index: zero,
                });
                let want = b.const_val(Const::Int(tag_val), IrType::Int);
                let cmp = b.alloc(None, IrType::Bool);
                b.emit(Inst::Binary {
                    dest: cmp,
                    op: BinOp::Eq,
                    lhs: tag,
                    rhs: want,
                });
                let pass = b.new_block();
                b.emit(Inst::Branch {
                    cond: cmp,
                    then_block: pass,
                    else_block: fail_block,
                });
                b.switch(pass);
            } else {
                let var_ty = b
                    .locals
                    .iter()
                    .find(|l| l.id == val)
                    .map(|l| l.ty.clone())
                    .unwrap_or(IrType::Dyn);
                let slot = b.alloc(Some(id.name.clone()), var_ty);
                b.emit(Inst::Move { dest: slot, src: val });
            }
        }
        flake_ast::Pattern::Literal { value, .. } => {
            let want = lower_literal(b, value);
            let cmp = b.alloc(None, IrType::Bool);
            b.emit(Inst::Binary {
                dest: cmp,
                op: BinOp::Eq,
                lhs: val,
                rhs: want,
            });
            let pass = b.new_block();
            b.emit(Inst::Branch {
                cond: cmp,
                then_block: pass,
                else_block: fail_block,
            });
            b.switch(pass);
        }
        flake_ast::Pattern::List { patterns, .. } => {
            let len_fn = Callee::Static("len".to_string());
            let len_res = b.alloc(None, IrType::Int);
            b.emit(Inst::Call {
                dest: Some(len_res),
                callee: len_fn,
                args: vec![val],
            });
            let want_len = b.const_val(Const::Int(patterns.len() as i64), IrType::Int);
            let cmp = b.alloc(None, IrType::Bool);
            b.emit(Inst::Binary {
                dest: cmp,
                op: BinOp::Eq,
                lhs: len_res,
                rhs: want_len,
            });
            let pass = b.new_block();
            b.emit(Inst::Branch {
                cond: cmp,
                then_block: pass,
                else_block: fail_block,
            });
            b.switch(pass);

            for (i, p) in patterns.iter().enumerate() {
                let idx = b.const_val(Const::Int(i as i64), IrType::Int);
                let elem = b.alloc(None, IrType::Dyn);
                b.emit(Inst::GetIndex {
                    dest: elem,
                    obj: val,
                    index: idx,
                });
                lower_pattern_test(b, elem, p, fail_block);
            }
        }
        flake_ast::Pattern::Variant {
            variant,
            fields,
            ty,
            ..
        } => {
            let zero = b.const_val(Const::Int(0), IrType::Int);
            let tag = b.alloc(None, IrType::Int);
            b.emit(Inst::GetIndex {
                dest: tag,
                obj: val,
                index: zero,
            });
            let (tag_val, field_types) = if let Some(t) = ty {
                b.enums
                    .get(&t.name)
                    .and_then(|variants| {
                        variants
                            .iter()
                            .enumerate()
                            .find(|(_, (name, _))| name == &variant.name)
                    })
                    .map(|(tag, (_, fields))| (tag as i64, fields.clone()))
                    .unwrap_or((0, Vec::new()))
            } else {
                b.enums
                    .values()
                    .find_map(|variants| {
                        variants
                            .iter()
                            .enumerate()
                            .find(|(_, (name, _))| name == &variant.name)
                            .map(|(tag, (_, fields))| (tag as i64, fields.clone()))
                    })
                    .unwrap_or((0, Vec::new()))
            };
            let want = b.const_val(Const::Int(tag_val), IrType::Int);
            let cmp = b.alloc(None, IrType::Bool);
            b.emit(Inst::Binary {
                dest: cmp,
                op: BinOp::Eq,
                lhs: tag,
                rhs: want,
            });
            let pass = b.new_block();
            b.emit(Inst::Branch {
                cond: cmp,
                then_block: pass,
                else_block: fail_block,
            });
            b.switch(pass);

            for (fi, field_pat) in fields.iter().enumerate() {
                let field_ty = field_types.get(fi).cloned().unwrap_or(IrType::Dyn);
                let idx = b.const_val(Const::Int((fi + 1) as i64), IrType::Int);
                let f_val = b.alloc(None, field_ty);
                b.emit(Inst::GetIndex {
                    dest: f_val,
                    obj: val,
                    index: idx,
                });
                lower_pattern_test(b, f_val, field_pat, fail_block);
            }
        }
    }
}

fn lower_try(b: &mut Builder, expr: &Expr) -> LocalId {
    let src = lower_expr(b, expr);
    let dest_ty = b
        .locals
        .iter()
        .find(|local| local.id == src)
        .and_then(|local| match &local.ty {
            IrType::Struct(name) => b.enums.get(name),
            _ => None,
        })
        .and_then(|variants| variants.iter().find(|(name, _)| name == "Ok"))
        .and_then(|(_, fields)| fields.first())
        .cloned()
        .unwrap_or(IrType::Dyn);
    let dest = b.alloc(None, dest_ty);
    let tag_index = b.const_val(Const::Int(0), IrType::Int);
    let tag = b.alloc(None, IrType::Int);
    b.emit(Inst::GetIndex {
        dest: tag,
        obj: src,
        index: tag_index,
    });
    let ok_tag = b.const_val(Const::Int(0), IrType::Int);
    let is_ok = b.alloc(None, IrType::Bool);
    b.emit(Inst::Binary {
        dest: is_ok,
        op: BinOp::Eq,
        lhs: tag,
        rhs: ok_tag,
    });
    let ok_block = b.new_block();
    let err_block = b.new_block();
    let exit = b.new_block();
    b.emit(Inst::Branch {
        cond: is_ok,
        then_block: ok_block,
        else_block: err_block,
    });

    b.switch(ok_block);
    let value_index = b.const_val(Const::Int(1), IrType::Int);
    b.emit(Inst::GetIndex {
        dest,
        obj: src,
        index: value_index,
    });
    b.emit(Inst::Jump { target: exit });

    b.switch(err_block);
    b.emit(Inst::Return { value: Some(src) });

    b.switch(exit);
    dest
}

fn lower_literal(b: &mut Builder, value: &Literal) -> LocalId {
    match value {
        Literal::Nil => b.const_val(Const::Nil, IrType::Nil),
        Literal::Bool(value) => b.const_val(Const::Bool(*value), IrType::Bool),
        Literal::Int(value) => b.const_val(Const::Int(*value), IrType::Int),
        Literal::Float(value) => b.const_val(Const::Float(*value), IrType::Float),
        Literal::String(value) => b.const_val(Const::String(value.clone()), IrType::String),
    }
}

fn common_local_type(b: &Builder, locals: &[LocalId]) -> IrType {
    let Some(first) = locals
        .first()
        .and_then(|id| b.locals.iter().find(|local| local.id == *id))
    else {
        return IrType::Dyn;
    };
    if locals.iter().all(|id| {
        b.locals
            .iter()
            .find(|local| local.id == *id)
            .is_some_and(|local| local.ty == first.ty)
    }) {
        first.ty.clone()
    } else {
        IrType::Dyn
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
    let lhs_ty = b.locals.iter().find(|l| l.id == lhs).map(|l| &l.ty);
    let rhs_ty = b.locals.iter().find(|l| l.id == rhs).map(|l| &l.ty);
    let dest_ty = match op {
        AstBin::Eq | AstBin::Ne | AstBin::Lt | AstBin::Le | AstBin::Gt | AstBin::Ge => {
            IrType::Bool
        }
        AstBin::Add if matches!(lhs_ty, Some(IrType::String)) || matches!(rhs_ty, Some(IrType::String)) => {
            IrType::String
        }
        AstBin::Add if matches!(lhs_ty, Some(IrType::List(_))) => {
            lhs_ty.cloned().unwrap_or(IrType::List(Box::new(IrType::Dyn)))
        }
        AstBin::Add if matches!(rhs_ty, Some(IrType::List(_))) => {
            rhs_ty.cloned().unwrap_or(IrType::List(Box::new(IrType::Dyn)))
        }
        _ if matches!(lhs_ty, Some(IrType::Float)) || matches!(rhs_ty, Some(IrType::Float)) => {
            IrType::Float
        }
        _ => IrType::Int,
    };
    let dest = b.alloc(None, dest_ty);
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
        Some(TypeExpr::Named { name, args, .. }) => match name.name.as_str() {
            "Int" => IrType::Int,
            "Float" => IrType::Float,
            "Bool" => IrType::Bool,
            "String" => IrType::String,
            "Nil" | "Unit" => IrType::Nil,
            "Range" => IrType::Range,
            "List" => args
                .first()
                .map(|element| IrType::List(Box::new(lower_type(Some(element)))))
                .unwrap_or_else(|| IrType::List(Box::new(IrType::Dyn))),
            "Map" => {
                let key = args
                    .first()
                    .map(|key| lower_type(Some(key)))
                    .unwrap_or(IrType::Dyn);
                let value = args
                    .get(1)
                    .map(|value| lower_type(Some(value)))
                    .unwrap_or(IrType::Dyn);
                IrType::Map(Box::new(key), Box::new(value))
            }
            "Task" => args
                .first()
                .map(|result| lower_type(Some(result)))
                .unwrap_or(IrType::Dyn),
            other => IrType::Struct(other.to_string()),
        },
        Some(TypeExpr::List { element, .. }) => IrType::List(Box::new(lower_type(Some(element)))),
        Some(TypeExpr::Owned { inner, .. })
        | Some(TypeExpr::Mut { inner, .. })
        | Some(TypeExpr::Ref { inner, .. })
        | Some(TypeExpr::Optional { inner, .. }) => lower_type(Some(inner)),
        Some(TypeExpr::Fn { ret, .. }) => IrType::Func(Box::new(
            ret.as_deref()
                .map(|ret| lower_type(Some(ret)))
                .unwrap_or(IrType::Nil),
        )),
    }
}

fn is_native_name(name: &str) -> bool {
    matches!(
        name,
        "print"
            | "len"
            | "push"
            | "pop"
            | "str"
            | "int"
            | "float"
            | "type_of"
            | "assert"
            | "read_file"
            | "write_file"
            | "abs"
            | "min"
            | "max"
            | "range"
            | "join"
            | "split"
            | "contains"
            | "starts_with"
            | "ends_with"
            | "first"
            | "last"
            | "trim"
            | "upper"
            | "lower"
            | "file_exists"
            | "env"
            | "cwd"
            | "remove_file"
            | "keys"
            | "values"
            | "entries"
            | "is_empty"
            | "has_key"
            | "cancel"
            | "is_cancelled"
            | "is_completed"
            | "task_status"
    )
}

fn native_result_ty(name: &str) -> IrType {
    match name {
        "print" | "push" | "assert" | "write_file" | "remove_file" | "cancel" => IrType::Nil,
        "len" | "int" => IrType::Int,
        "str" | "join" | "type_of" | "read_file" | "trim" | "upper" | "lower" | "env" | "cwd"
        | "task_status" => IrType::String,
        "contains"
        | "starts_with"
        | "ends_with"
        | "file_exists"
        | "is_empty"
        | "has_key"
        | "is_cancelled"
        | "is_completed" => IrType::Bool,
        "range" => IrType::Range,
        "split" => IrType::List(Box::new(IrType::String)),
        "keys" | "values" => IrType::List(Box::new(IrType::Dyn)),
        "entries" => IrType::List(Box::new(IrType::List(Box::new(IrType::Dyn)))),
        "float" => IrType::Float,
        _ => IrType::Dyn,
    }
}

fn native_call_result_ty(b: &Builder, name: &str, args: &[LocalId]) -> IrType {
    if matches!(name, "abs" | "min" | "max") {
        return args
            .first()
            .and_then(|id| b.locals.iter().find(|local| local.id == *id))
            .map(|local| local.ty.clone())
            .unwrap_or(IrType::Dyn);
    }
    if name == "keys" {
        return args
            .first()
            .and_then(|id| b.locals.iter().find(|local| local.id == *id))
            .and_then(|local| match &local.ty {
                IrType::Map(k, _) => Some(IrType::List(k.clone())),
                _ => None,
            })
            .unwrap_or_else(|| IrType::List(Box::new(IrType::Dyn)));
    }
    if name == "values" {
        return args
            .first()
            .and_then(|id| b.locals.iter().find(|local| local.id == *id))
            .and_then(|local| match &local.ty {
                IrType::Map(_, v) => Some(IrType::List(v.clone())),
                _ => None,
            })
            .unwrap_or_else(|| IrType::List(Box::new(IrType::Dyn)));
    }
    if name == "entries" {
        return IrType::List(Box::new(IrType::List(Box::new(IrType::Dyn))));
    }
    native_result_ty(name)
}
