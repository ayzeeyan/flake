//! Type checking and local inference with gradual `dyn`.

use std::collections::{HashMap, HashSet};

use flake_ast::{
    AssignOp, BinOp, Block, Expr, FnDecl, InterpPart, Item, Literal, Program, Source, Span, Stmt,
    TypeExpr, UnOp,
};
use flake_parser::{ModuleGraph, import_alias, is_exported, load_graph};

use crate::effects::{Effect, EffectSet};
use crate::error::{CheckError, TypeError};
use crate::ty::Type;

struct Checker {
    subst: Vec<Option<Type>>,
    scopes: Vec<HashMap<String, Type>>,
    structs: HashMap<String, Type>,
    aliases: HashMap<String, Type>,
    functions: HashMap<String, Type>,
    enums: HashMap<String, Type>,
    current_returns: Vec<Type>,
}

impl Checker {
    fn new() -> Self {
        let mut this = Self {
            subst: Vec::new(),
            scopes: vec![HashMap::new()],
            structs: HashMap::new(),
            aliases: HashMap::new(),
            functions: HashMap::new(),
            enums: HashMap::new(),
            current_returns: Vec::new(),
        };
        this.install_builtins();
        this
    }

    fn fresh(&mut self) -> Type {
        let id = self.subst.len() as u32;
        self.subst.push(None);
        Type::Var(id)
    }

    fn install_builtins(&mut self) {
        let mk = |params: Vec<Type>, ret: Type, effects: &[&str]| {
            Type::function(
                params,
                ret,
                EffectSet::from_names(effects.iter().copied(), true),
            )
        };
        self.functions
            .insert("print".into(), mk(vec![Type::Dyn], Type::Nil, &["io"]));
        self.functions
            .insert("len".into(), mk(vec![Type::Dyn], Type::Int, &[]));
        self.functions.insert(
            "push".into(),
            mk(
                vec![Type::list(Type::Dyn), Type::Dyn],
                Type::Nil,
                &["alloc"],
            ),
        );
        self.functions.insert(
            "pop".into(),
            mk(vec![Type::list(Type::Dyn)], Type::Dyn, &[]),
        );
        self.functions
            .insert("str".into(), mk(vec![Type::Dyn], Type::String, &["alloc"]));
        self.functions
            .insert("int".into(), mk(vec![Type::Dyn], Type::Int, &[]));
        self.functions
            .insert("float".into(), mk(vec![Type::Dyn], Type::Float, &[]));
        self.functions.insert(
            "type_of".into(),
            mk(vec![Type::Dyn], Type::String, &["alloc"]),
        );
        self.functions
            .insert("assert".into(), mk(vec![Type::Bool], Type::Nil, &["panic"]));
        self.functions.insert(
            "read_file".into(),
            mk(vec![Type::String], Type::String, &["io", "alloc"]),
        );
        self.functions
            .insert("abs".into(), mk(vec![Type::Dyn], Type::Dyn, &[]));
        self.functions
            .insert("min".into(), mk(vec![Type::Dyn, Type::Dyn], Type::Dyn, &[]));
        self.functions
            .insert("max".into(), mk(vec![Type::Dyn, Type::Dyn], Type::Dyn, &[]));
        self.functions
            .insert("range".into(), mk(vec![Type::Int], Type::Range, &[]));
        self.functions.insert(
            "join".into(),
            mk(
                vec![Type::list(Type::Dyn), Type::String],
                Type::String,
                &["alloc"],
            ),
        );
        self.functions.insert(
            "split".into(),
            mk(
                vec![Type::String, Type::String],
                Type::list(Type::String),
                &["alloc"],
            ),
        );
        self.functions.insert(
            "write_file".into(),
            mk(vec![Type::String, Type::Dyn], Type::Nil, &["io"]),
        );
        self.functions.insert(
            "contains".into(),
            mk(vec![Type::Dyn, Type::Dyn], Type::Bool, &[]),
        );
        self.functions.insert(
            "starts_with".into(),
            mk(vec![Type::String, Type::String], Type::Bool, &[]),
        );
        self.functions.insert(
            "ends_with".into(),
            mk(vec![Type::String, Type::String], Type::Bool, &[]),
        );
        self.functions
            .insert("first".into(), mk(vec![Type::Dyn], Type::Dyn, &[]));
        self.functions
            .insert("last".into(), mk(vec![Type::Dyn], Type::Dyn, &[]));
        self.functions.insert(
            "trim".into(),
            mk(vec![Type::String], Type::String, &["alloc"]),
        );
        self.functions.insert(
            "upper".into(),
            mk(vec![Type::String], Type::String, &["alloc"]),
        );
        self.functions.insert(
            "lower".into(),
            mk(vec![Type::String], Type::String, &["alloc"]),
        );
        self.functions.insert(
            "file_exists".into(),
            mk(vec![Type::String], Type::Bool, &["io"]),
        );
        self.functions
            .insert("env".into(), mk(vec![Type::String], Type::String, &["io"]));
        self.functions
            .insert("cwd".into(), mk(vec![], Type::String, &["io", "alloc"]));
        self.functions.insert(
            "remove_file".into(),
            mk(vec![Type::String], Type::Nil, &["io"]),
        );
    }

    fn resolve(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(id) => match self.subst.get(*id as usize).and_then(|t| t.as_ref()) {
                Some(t) => self.resolve(t),
                None => ty.clone(),
            },
            Type::List(e) => Type::list(self.resolve(e)),
            Type::Map(k, v) => Type::Map(Box::new(self.resolve(k)), Box::new(self.resolve(v))),
            Type::Task(result) => Type::Task(Box::new(self.resolve(result))),
            Type::Optional(i) => Type::Optional(Box::new(self.resolve(i))),
            Type::Owned(i) => Type::Owned(Box::new(self.resolve(i))),
            Type::Mut(i) => Type::Mut(Box::new(self.resolve(i))),
            Type::Ref { mutable, inner } => Type::Ref {
                mutable: *mutable,
                inner: Box::new(self.resolve(inner)),
            },
            Type::Fn {
                params,
                ret,
                effects,
            } => Type::Fn {
                params: params.iter().map(|p| self.resolve(p)).collect(),
                ret: Box::new(self.resolve(ret)),
                effects: effects.clone(),
            },
            other => other.clone(),
        }
    }

    fn unify(&mut self, a: &Type, b: &Type, span: Span) -> Result<Type, TypeError> {
        let a = self.resolve(a).without_ownership();
        let b = self.resolve(b).without_ownership();
        match (a, b) {
            (Type::Dyn, other) | (other, Type::Dyn) => Ok(other),
            (Type::Var(i), Type::Var(j)) if i == j => Ok(Type::Var(i)),
            (Type::Var(i), other) => self.bind(i, other, span),
            (other, Type::Var(i)) => self.bind(i, other, span),
            (Type::Nil, Type::Nil) => Ok(Type::Nil),
            (Type::Bool, Type::Bool) => Ok(Type::Bool),
            (Type::Int, Type::Int) => Ok(Type::Int),
            (Type::Float, Type::Float) => Ok(Type::Float),
            (Type::String, Type::String) => Ok(Type::String),
            (Type::Range, Type::Range) => Ok(Type::Range),
            (Type::List(x), Type::List(y)) => Ok(Type::list(self.unify(&x, &y, span)?)),
            (Type::Map(k1, v1), Type::Map(k2, v2)) => Ok(Type::Map(
                Box::new(self.unify(&k1, &k2, span)?),
                Box::new(self.unify(&v1, &v2, span)?),
            )),
            (Type::Task(a), Type::Task(b)) => Ok(Type::Task(Box::new(self.unify(&a, &b, span)?))),
            (Type::Optional(x), Type::Optional(y)) => {
                Ok(Type::Optional(Box::new(self.unify(&x, &y, span)?)))
            }
            (Type::Optional(x), y) | (y, Type::Optional(x)) => self.unify(&x, &y, span),
            (Type::Struct { name: n1, .. }, Type::Struct { name: n2, .. }) if n1 == n2 => {
                Ok(Type::Struct {
                    name: n1,
                    fields: Vec::new(),
                })
            }
            (
                Type::Enum {
                    name: n1,
                    variants: v1,
                },
                Type::Enum { name: n2, .. },
            ) if n1 == n2 => Ok(Type::Enum {
                name: n1,
                variants: v1,
            }),
            (
                Type::Fn {
                    params: p1,
                    ret: r1,
                    effects: e1,
                },
                Type::Fn {
                    params: p2,
                    ret: r2,
                    effects: e2,
                },
            ) => {
                if p1.len() != p2.len() {
                    return Err(TypeError::new(
                        span,
                        format!(
                            "function types have different arities ({} vs {})",
                            p1.len(),
                            p2.len()
                        ),
                    ));
                }
                let mut params = Vec::new();
                for (a, b) in p1.iter().zip(p2.iter()) {
                    params.push(self.unify(a, b, span)?);
                }
                let ret = self.unify(&r1, &r2, span)?;
                Ok(Type::Fn {
                    params,
                    ret: Box::new(ret),
                    effects: if e1.specified { e1 } else { e2 },
                })
            }
            (a, b) if format!("{a}") == format!("{b}") => Ok(a),
            (a, b) => Err(TypeError::new(
                span,
                format!("type mismatch: expected {a}, found {b}"),
            )),
        }
    }

    fn bind(&mut self, id: u32, ty: Type, span: Span) -> Result<Type, TypeError> {
        if occurs(id, &ty) {
            return Err(TypeError::new(span, "infinite type"));
        }
        self.subst[id as usize] = Some(ty.clone());
        Ok(ty)
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, name: String, ty: Type) {
        self.scopes.last_mut().unwrap().insert(name, ty);
    }

    fn install_imports(
        &mut self,
        graph: &ModuleGraph,
        module: &flake_parser::LoadedModule,
    ) -> Result<(), TypeError> {
        for item in &module.program.items {
            let Item::Import(import) = item else {
                continue;
            };
            let Some(imported) = graph.get(&import.path.name) else {
                return Err(TypeError::new(
                    import.span,
                    format!("unresolved import `{}`", import.path.name),
                ));
            };
            let alias = import_alias(import).to_string();
            let mut members = Vec::new();
            for item in &imported.program.items {
                if !is_exported(item, &imported.program) {
                    continue;
                }
                match item {
                    Item::Struct(st) => {
                        let mut fields = Vec::new();
                        for field in &st.fields {
                            fields.push((field.name.name.clone(), self.lower_type(&field.ty)?));
                        }
                        let ty = Type::Struct {
                            name: st.name.name.clone(),
                            fields: fields.clone(),
                        };
                        self.structs.insert(st.name.name.clone(), ty.clone());
                        members.push((st.name.name.clone(), ty));
                    }
                    Item::Type(alias_item) => {
                        let ty = self.lower_type(&alias_item.ty)?;
                        self.aliases.insert(alias_item.name.name.clone(), ty);
                    }
                    Item::Fn(func) => {
                        let ty = self.lower_fn_type(func)?;
                        if !self.functions.contains_key(&func.name.name) {
                            self.functions.insert(func.name.name.clone(), ty.clone());
                        }
                        members.push((func.name.name.clone(), ty));
                    }
                    Item::Enum(en) => {
                        let mut variants = Vec::new();
                        for v in &en.variants {
                            let fields = v
                                .fields
                                .iter()
                                .map(|t| self.lower_type(t))
                                .collect::<Result<Vec<_>, _>>()?;
                            variants.push((v.name.name.clone(), fields));
                        }
                        let ty = Type::Enum {
                            name: en.name.name.clone(),
                            variants,
                        };
                        self.enums.insert(en.name.name.clone(), ty.clone());
                        members.push((en.name.name.clone(), ty));
                    }
                    Item::Import(_) => {}
                }
            }
            self.define(
                alias,
                Type::Module {
                    name: imported.name.clone(),
                    members,
                },
            );
        }
        Ok(())
    }

    fn lookup(&self, name: &str) -> Option<Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(t) = scope.get(name) {
                return Some(t.clone());
            }
        }
        self.functions
            .get(name)
            .cloned()
            .or_else(|| self.enums.get(name).cloned())
            .or_else(|| self.structs.get(name).cloned())
    }

    fn known_names(&self) -> Vec<String> {
        let mut names = HashSet::new();
        for scope in &self.scopes {
            names.extend(scope.keys().cloned());
        }
        names.extend(self.functions.keys().cloned());
        names.extend(self.enums.keys().cloned());
        names.extend(self.structs.keys().cloned());
        names.into_iter().collect()
    }

    fn suggest(&self, name: &str) -> Option<String> {
        suggest_name(name, &self.known_names())
    }

    fn check_program(&mut self, program: &Program) -> Result<(), TypeError> {
        for item in &program.items {
            match item {
                Item::Type(alias) => {
                    let ty = self.lower_type(&alias.ty)?;
                    self.aliases.insert(alias.name.name.clone(), ty);
                }
                Item::Struct(st) => {
                    let mut fields = Vec::new();
                    for field in &st.fields {
                        fields.push((field.name.name.clone(), self.lower_type(&field.ty)?));
                    }
                    let ty = Type::Struct {
                        name: st.name.name.clone(),
                        fields,
                    };
                    self.structs.insert(st.name.name.clone(), ty);
                }
                Item::Enum(en) => {
                    if en.variants.is_empty() {
                        return Err(TypeError::with_help(
                            en.span,
                            format!("enum `{}` has no variants", en.name.name),
                            "declare at least one variant",
                        ));
                    }
                    let mut variants = Vec::new();
                    let mut seen = HashSet::new();
                    for v in &en.variants {
                        if !seen.insert(v.name.name.as_str()) {
                            return Err(TypeError::with_help(
                                v.name.span,
                                format!(
                                    "duplicate variant `{}` in enum `{}`",
                                    v.name.name, en.name.name
                                ),
                                "variant names must be unique within an enum",
                            ));
                        }
                        let fields = v
                            .fields
                            .iter()
                            .map(|t| self.lower_type(t))
                            .collect::<Result<Vec<_>, _>>()?;
                        variants.push((v.name.name.clone(), fields));
                    }
                    let ty = Type::Enum {
                        name: en.name.name.clone(),
                        variants,
                    };
                    self.enums.insert(en.name.name.clone(), ty);
                }
                Item::Fn(_) | Item::Import(_) => {}
            }
        }

        for item in &program.items {
            if let Item::Fn(func) = item {
                let ty = self.lower_fn_type(func)?;
                self.functions.insert(func.name.name.clone(), ty);
            }
        }

        for item in &program.items {
            match item {
                Item::Fn(func) => self.check_fn(func)?,
                Item::Import(_) | Item::Struct(_) | Item::Enum(_) | Item::Type(_) => {}
            }
        }
        self.check_effects(program)?;
        crate::ownership::check_ownership(program)?;
        Ok(())
    }

    fn bind_pattern(&mut self, pat: &flake_ast::Pattern, ty: &Type) -> Result<(), TypeError> {
        match pat {
            flake_ast::Pattern::Wildcard { .. } => Ok(()),
            flake_ast::Pattern::Literal { value, span } => {
                self.unify(ty, &literal_type(value), *span)?;
                Ok(())
            }
            flake_ast::Pattern::Ident(id) => {
                self.define(id.name.clone(), ty.clone());
                Ok(())
            }
            flake_ast::Pattern::Variant {
                ty: enum_name,
                variant,
                binds,
                span,
            } => {
                let enum_ty = if let Some(n) = enum_name {
                    self.enums.get(&n.name).cloned().ok_or_else(|| {
                        TypeError::new(*span, format!("unknown enum `{}`", n.name))
                    })?
                } else {
                    self.resolve(ty)
                };
                self.unify(ty, &enum_ty, *span)?;
                let Type::Enum { name, variants } = enum_ty.without_ownership() else {
                    return Err(TypeError::with_help(
                        *span,
                        "match variant on a non-enum value",
                        "use `Enum.Variant` patterns only when matching an enum",
                    ));
                };
                let Some((_, fields)) = variants.iter().find(|(n, _)| n == &variant.name) else {
                    let listed: Vec<_> = variants.iter().map(|(n, _)| n.as_str()).collect();
                    return Err(TypeError::with_help(
                        variant.span,
                        format!("enum `{name}` has no variant `{}`", variant.name),
                        format!("available variants: {}", listed.join(", ")),
                    ));
                };
                if binds.len() != fields.len() {
                    return Err(TypeError::with_help(
                        *span,
                        format!(
                            "variant `{}` expects {} field(s), got {}",
                            variant.name,
                            fields.len(),
                            binds.len()
                        ),
                        "bind each tuple field, or use `_` for a field you do not need",
                    ));
                }
                for (b, ft) in binds.iter().zip(fields.iter()) {
                    if b.name != "_" {
                        self.define(b.name.clone(), ft.clone());
                    }
                }
                Ok(())
            }
        }
    }

    fn check_match_exhaustive(
        &self,
        scrut_ty: &Type,
        arms: &[flake_ast::MatchArm],
        span: Span,
    ) -> Result<(), TypeError> {
        let mut catch_all = false;
        let mut seen = HashSet::new();
        for arm in arms {
            if catch_all {
                return Err(TypeError::with_help(
                    arm.span,
                    "unreachable match arm",
                    "a previous `_` or identifier pattern already matches every value",
                ));
            }
            match &arm.pattern {
                flake_ast::Pattern::Wildcard { .. } | flake_ast::Pattern::Ident(_) => {
                    catch_all = true;
                }
                pattern => {
                    if let Some(key) = pattern_key(pattern) {
                        if !seen.insert(key) {
                            return Err(TypeError::with_help(
                                arm.span,
                                "unreachable duplicate match arm",
                                "remove the duplicate pattern",
                            ));
                        }
                    }
                }
            }
        }
        if catch_all {
            return Ok(());
        }
        match self.resolve(scrut_ty).without_ownership() {
            Type::Enum { name, variants } => {
                let covered: HashSet<&str> = arms
                    .iter()
                    .filter_map(|arm| match &arm.pattern {
                        flake_ast::Pattern::Variant { variant, .. } => Some(variant.name.as_str()),
                        _ => None,
                    })
                    .collect();
                let missing: Vec<&str> = variants
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .filter(|n| !covered.contains(n))
                    .collect();
                if missing.is_empty() {
                    Ok(())
                } else {
                    Err(TypeError::with_help(
                        span,
                        format!(
                            "non-exhaustive match on `{name}`: missing {}",
                            missing.join(", ")
                        ),
                        "add a `_` arm or cover the remaining variants",
                    ))
                }
            }
            Type::Bool => {
                let true_covered = arms.iter().any(|arm| {
                    matches!(
                        arm.pattern,
                        flake_ast::Pattern::Literal {
                            value: Literal::Bool(true),
                            ..
                        }
                    )
                });
                let false_covered = arms.iter().any(|arm| {
                    matches!(
                        arm.pattern,
                        flake_ast::Pattern::Literal {
                            value: Literal::Bool(false),
                            ..
                        }
                    )
                });
                if true_covered && false_covered {
                    Ok(())
                } else {
                    let missing = match (true_covered, false_covered) {
                        (false, false) => "`true` and `false`",
                        (false, true) => "`true`",
                        (true, false) => "`false`",
                        (true, true) => unreachable!(),
                    };
                    Err(TypeError::with_help(
                        span,
                        format!("non-exhaustive match on Bool: missing {missing}"),
                        "cover both `true` and `false`, or add a `_` arm",
                    ))
                }
            }
            Type::Nil
                if arms.iter().any(|arm| {
                    matches!(
                        arm.pattern,
                        flake_ast::Pattern::Literal {
                            value: Literal::Nil,
                            ..
                        }
                    )
                }) =>
            {
                Ok(())
            }
            Type::Dyn | Type::Var(_) => Ok(()),
            other => Err(TypeError::with_help(
                span,
                format!("non-exhaustive match on {other}"),
                "add a `_` or identifier arm",
            )),
        }
    }

    fn lower_fn_type(&mut self, func: &FnDecl) -> Result<Type, TypeError> {
        let mut params = Vec::new();
        for p in &func.params {
            params.push(match &p.ty {
                Some(t) => self.lower_type(t)?,
                None => self.fresh(),
            });
        }
        let ret = match &func.return_type {
            Some(t) => self.lower_type(t)?,
            None => self.fresh(),
        };
        let effects = EffectSet::from_names(func.effects.names(), func.effects.specified);
        Ok(Type::function(params, ret, effects))
    }

    fn lower_type(&mut self, ty: &TypeExpr) -> Result<Type, TypeError> {
        Ok(match ty {
            TypeExpr::Dyn { .. } => Type::Dyn,
            TypeExpr::Named { name, args, span } => match name.name.as_str() {
                "Int" => Type::Int,
                "Float" => Type::Float,
                "Bool" => Type::Bool,
                "String" => Type::String,
                "Nil" | "Unit" => Type::Nil,
                "Range" => Type::Range,
                "dyn" => Type::Dyn,
                "List" => {
                    let elem = match args.as_slice() {
                        [e] => self.lower_type(e)?,
                        [] => Type::Dyn,
                        _ => {
                            return Err(TypeError::new(*span, "List expects one type argument"));
                        }
                    };
                    Type::list(elem)
                }
                "Map" => {
                    let (k, v) = match args.as_slice() {
                        [k, v] => (self.lower_type(k)?, self.lower_type(v)?),
                        [v] => (Type::String, self.lower_type(v)?),
                        [] => (Type::Dyn, Type::Dyn),
                        _ => {
                            return Err(TypeError::new(
                                *span,
                                "Map expects one or two type arguments",
                            ));
                        }
                    };
                    Type::Map(Box::new(k), Box::new(v))
                }
                "Task" => {
                    let result = match args.as_slice() {
                        [result] => self.lower_type(result)?,
                        _ => {
                            return Err(TypeError::new(
                                *span,
                                "Task expects exactly one type argument",
                            ));
                        }
                    };
                    Type::Task(Box::new(result))
                }
                other => {
                    if let Some(alias) = self.aliases.get(other) {
                        alias.clone()
                    } else if let Some(st) = self.structs.get(other) {
                        st.clone()
                    } else if let Some(en) = self.enums.get(other) {
                        en.clone()
                    } else {
                        Type::Struct {
                            name: other.to_string(),
                            fields: Vec::new(),
                        }
                    }
                }
            },
            TypeExpr::List { element, .. } => Type::list(self.lower_type(element)?),
            TypeExpr::Optional { inner, .. } => Type::Optional(Box::new(self.lower_type(inner)?)),
            TypeExpr::Owned { inner, .. } => Type::Owned(Box::new(self.lower_type(inner)?)),
            TypeExpr::Ref { mutable, inner, .. } => Type::Ref {
                mutable: *mutable,
                inner: Box::new(self.lower_type(inner)?),
            },
            TypeExpr::Mut { inner, .. } => Type::Mut(Box::new(self.lower_type(inner)?)),
            TypeExpr::Fn {
                params,
                ret,
                effects,
                ..
            } => {
                let params = params
                    .iter()
                    .map(|p| self.lower_type(p))
                    .collect::<Result<Vec<_>, _>>()?;
                let ret = match ret {
                    Some(r) => self.lower_type(r)?,
                    None => Type::Nil,
                };
                let effects = EffectSet::from_names(effects.names(), effects.specified);
                Type::function(params, ret, effects)
            }
        })
    }

    fn check_fn(&mut self, func: &FnDecl) -> Result<(), TypeError> {
        let Type::Fn { params, ret, .. } = self
            .functions
            .get(&func.name.name)
            .cloned()
            .ok_or_else(|| TypeError::new(func.span, "internal: missing function type"))?
        else {
            return Err(TypeError::new(func.span, "internal: not a function type"));
        };
        self.push_scope();
        for (param, ty) in func.params.iter().zip(params.iter()) {
            self.define(param.name.name.clone(), ty.clone());
        }
        self.current_returns.push((*ret).clone());
        let body_ty = self.check_block(&func.body)?;
        if contains_task(&self.resolve(&body_ty).without_ownership()) {
            return Err(TypeError::with_help(
                func.body.span,
                "task handle cannot escape its spawning function",
                "await the task inside this function and return its result instead",
            ));
        }
        if !block_definitely_returns(&func.body) {
            self.unify(&body_ty, &ret, func.body.span)?;
        }
        self.current_returns.pop();
        self.pop_scope();
        Ok(())
    }

    fn check_block(&mut self, block: &Block) -> Result<Type, TypeError> {
        self.push_scope();
        for stmt in &block.stmts {
            self.check_stmt(stmt)?;
        }
        let ty = if let Some(tail) = &block.tail {
            self.check_expr(tail)?
        } else {
            Type::Nil
        };
        self.pop_scope();
        Ok(ty)
    }

    fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), TypeError> {
        match stmt {
            Stmt::Let(s) => {
                let ty = self.check_binding(&s.ty, &s.value)?;
                self.define(s.name.name.clone(), ty);
            }
            Stmt::Var(s) => {
                let ty = self.check_binding(&s.ty, &s.value)?;
                self.define(s.name.name.clone(), ty);
            }
            Stmt::Return { value, span } => {
                let ty = if let Some(v) = value {
                    let ty = self.check_expr(v)?;
                    if contains_task(&self.resolve(&ty).without_ownership()) {
                        return Err(TypeError::with_help(
                            *span,
                            "task handle cannot escape its spawning function",
                            "await the task before returning",
                        ));
                    }
                    ty
                } else {
                    Type::Nil
                };
                if let Some(expected) = self.current_returns.last().cloned() {
                    self.unify(&expected, &ty, *span)?;
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::While { cond, body, span } => {
                let c = self.check_expr(cond)?;
                self.unify(&c, &Type::Bool, *span)?;
                self.check_block(body)?;
            }
            Stmt::For {
                name, iter, body, ..
            } => {
                let it = self.check_expr(iter)?;
                let elem = self.iterator_elem(&it, iter.span())?;
                self.push_scope();
                self.define(name.name.clone(), elem);
                self.check_block(body)?;
                self.pop_scope();
            }
            Stmt::Loop { body, .. } => {
                self.check_block(body)?;
            }
            Stmt::Expr(e) => {
                self.check_expr(e)?;
            }
        }
        Ok(())
    }

    fn check_binding(&mut self, ann: &Option<TypeExpr>, value: &Expr) -> Result<Type, TypeError> {
        if let Some(ann) = ann {
            let ann = self.lower_type(ann)?;
            self.check_expr_expected(value, Some(&ann))
        } else {
            self.check_expr(value)
        }
    }

    fn check_expr_expected(
        &mut self,
        expr: &Expr,
        expected: Option<&Type>,
    ) -> Result<Type, TypeError> {
        let expected = expected.map(|t| self.resolve(t).without_ownership());
        match (expr, expected.as_ref()) {
            (Expr::List { elements, .. }, Some(Type::List(elem))) => {
                for e in elements {
                    self.check_expr_expected(e, Some(elem))?;
                }
                Ok(Type::list((**elem).clone()))
            }
            (Expr::Map { entries, .. }, Some(Type::Map(key, value))) => {
                self.ensure_map_key(key, expr.span())?;
                for (entry_key, entry_value) in entries {
                    self.check_expr_expected(entry_key, Some(key))?;
                    self.check_expr_expected(entry_value, Some(value))?;
                }
                Ok(Type::Map(key.clone(), value.clone()))
            }
            (other, Some(exp)) => {
                let t = self.check_expr(other)?;
                self.unify(&t, exp, other.span())
            }
            (other, None) => self.check_expr(other),
        }
    }

    fn iterator_elem(&mut self, ty: &Type, span: Span) -> Result<Type, TypeError> {
        match self.resolve(ty).without_ownership() {
            Type::List(e) => Ok(*e),
            Type::Range => Ok(Type::Int),
            Type::String => Ok(Type::String),
            Type::Dyn | Type::Var(_) => Ok(Type::Dyn),
            other => Err(TypeError::new(span, format!("cannot iterate over {other}"))),
        }
    }

    fn ensure_map_key(&self, ty: &Type, span: Span) -> Result<(), TypeError> {
        match self.resolve(ty).without_ownership() {
            Type::String | Type::Int | Type::Bool | Type::Dyn | Type::Var(_) => Ok(()),
            other => Err(TypeError::with_help(
                span,
                format!("{other} cannot be used as a map key"),
                "map keys must be String, Int, or Bool",
            )),
        }
    }

    fn check_expr(&mut self, expr: &Expr) -> Result<Type, TypeError> {
        match expr {
            Expr::Literal { value, .. } => Ok(match value {
                Literal::Nil => Type::Nil,
                Literal::Bool(_) => Type::Bool,
                Literal::Int(_) => Type::Int,
                Literal::Float(_) => Type::Float,
                Literal::String(_) => Type::String,
            }),
            Expr::Ident(id) => self.lookup(&id.name).ok_or_else(|| {
                let msg = format!("undefined variable `{}`", id.name);
                match self.suggest(&id.name) {
                    Some(alt) => {
                        TypeError::with_help(id.span, msg, format!("did you mean `{alt}`?"))
                    }
                    None => TypeError::new(id.span, msg),
                }
            }),
            Expr::Interpolated { parts, .. } => {
                for part in parts {
                    if let InterpPart::Expr(e) = part {
                        self.check_expr(e)?;
                    }
                }
                Ok(Type::String)
            }
            Expr::List { elements, .. } => {
                let mut elem = Type::Dyn;
                let mut saw = false;
                for e in elements {
                    let t = self.check_expr(e)?;
                    if !saw {
                        elem = t;
                        saw = true;
                    } else {
                        elem = self.unify(&elem, &t, e.span())?;
                    }
                }
                Ok(Type::list(elem))
            }
            Expr::Map { entries, .. } => {
                let mut key = Type::Dyn;
                let mut val = Type::Dyn;
                let mut saw = false;
                for (k, v) in entries {
                    let kt = self.check_expr(k)?;
                    self.ensure_map_key(&kt, k.span())?;
                    let vt = self.check_expr(v)?;
                    if !saw {
                        key = kt;
                        val = vt;
                        saw = true;
                    } else {
                        key = self.unify(&key, &kt, k.span())?;
                        val = self.unify(&val, &vt, v.span())?;
                    }
                }
                Ok(Type::Map(Box::new(key), Box::new(val)))
            }
            Expr::Unary { op, expr, span } => {
                let t = self.check_expr(expr)?;
                match op {
                    UnOp::Neg => match self.resolve(&t) {
                        Type::Int | Type::Float | Type::Dyn | Type::Var(_) => Ok(t),
                        other => Err(TypeError::new(*span, format!("cannot negate {other}"))),
                    },
                    UnOp::Not => {
                        self.unify(&t, &Type::Bool, *span)?;
                        Ok(Type::Bool)
                    }
                    UnOp::Ref => Ok(Type::Ref {
                        mutable: false,
                        inner: Box::new(t),
                    }),
                    UnOp::RefMut => Ok(Type::Ref {
                        mutable: true,
                        inner: Box::new(t),
                    }),
                }
            }
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => self.check_binary(*op, left, right, *span),
            Expr::Assign {
                op,
                target,
                value,
                span,
            } => {
                let t_ty = self.check_expr(target)?;
                let v_ty = self.check_expr(value)?;
                if *op == AssignOp::Assign {
                    self.unify(&t_ty, &v_ty, *span)
                } else {
                    self.check_binary(assign_bin(*op), target, value, *span)
                }
            }
            Expr::Call { callee, args, span } => {
                let fty = self.check_expr(callee)?;
                let fty = self.resolve(&fty);
                match fty.without_ownership() {
                    Type::Fn { params, ret, .. } => {
                        // `print` is variadic in the interpreter; accept any arity.
                        let variadic_print = matches!(
                            callee.as_ref(),
                            Expr::Ident(id) if matches!(
                                id.name.as_str(),
                                "print" | "assert" | "min" | "max" | "range"
                            )
                        );
                        if !variadic_print && params.len() != args.len() {
                            return Err(TypeError::new(
                                *span,
                                format!(
                                    "expected {} argument(s), got {}",
                                    params.len(),
                                    args.len()
                                ),
                            ));
                        }
                        for (i, arg) in args.iter().enumerate() {
                            let at = self.check_expr(arg)?;
                            if let Some(pt) = params.get(i) {
                                self.unify(pt, &at, arg.span())?;
                            }
                        }
                        Ok(*ret)
                    }
                    Type::Dyn | Type::Var(_) => {
                        for arg in args {
                            self.check_expr(arg)?;
                        }
                        Ok(Type::Dyn)
                    }
                    other => Err(TypeError::new(*span, format!("cannot call {other}"))),
                }
            }
            Expr::Spawn { call, span } => {
                if !matches!(call.as_ref(), Expr::Call { .. }) {
                    return Err(TypeError::with_help(
                        *span,
                        "`spawn` expects a function call",
                        "pass the work and its arguments as `spawn work(args...)`",
                    ));
                }
                let result = self.check_expr(call)?;
                Ok(Type::Task(Box::new(result)))
            }
            Expr::Await { task, span } => {
                let task_ty = self.check_expr(task)?;
                match self.resolve(&task_ty).without_ownership() {
                    Type::Task(result) => Ok(*result),
                    Type::Dyn | Type::Var(_) => Ok(Type::Dyn),
                    other => Err(TypeError::with_help(
                        *span,
                        format!("cannot await {other}"),
                        "`await` accepts a Task[T] returned by `spawn`",
                    )),
                }
            }
            Expr::Try { expr, span } => {
                let result_ty = self.check_expr(expr)?;
                let resolved = self.resolve(&result_ty).without_ownership();
                let Type::Enum { name, variants } = &resolved else {
                    return Err(TypeError::with_help(
                        *span,
                        format!("`?` expects a Result-like enum, found {resolved}"),
                        "use an enum with `Ok(value)` and `Err(error)` variants",
                    ));
                };
                let (Some((ok_name, ok_fields)), Some((err_name, err_fields))) =
                    (variants.first(), variants.get(1))
                else {
                    return Err(TypeError::with_help(
                        *span,
                        format!("enum `{name}` is not Result-like"),
                        "`?` requires exactly `Ok(value)` followed by `Err(error)`",
                    ));
                };
                if variants.len() != 2
                    || ok_name != "Ok"
                    || err_name != "Err"
                    || ok_fields.len() != 1
                    || err_fields.len() != 1
                {
                    return Err(TypeError::with_help(
                        *span,
                        format!("enum `{name}` is not Result-like"),
                        "declare exactly `Ok(value)` followed by `Err(error)`",
                    ));
                }
                let Some(return_ty) = self.current_returns.last().cloned() else {
                    return Err(TypeError::new(
                        *span,
                        "`?` can only be used inside a function",
                    ));
                };
                self.unify(&return_ty, &resolved, *span).map_err(|_| {
                    TypeError::with_help(
                        *span,
                        format!("cannot propagate `{name}.Err` from this function"),
                        format!("change the function return type to `{name}` or handle the error with `match`"),
                    )
                })?;
                Ok(ok_fields[0].clone())
            }
            Expr::Index {
                target,
                index,
                span,
            } => {
                let t = self.check_expr(target)?;
                let i = self.check_expr(index)?;
                match self.resolve(&t).without_ownership() {
                    Type::List(elem) => {
                        self.unify(&i, &Type::Int, *span)?;
                        Ok(*elem)
                    }
                    Type::Map(k, v) => {
                        self.unify(&i, &k, *span)?;
                        Ok(*v)
                    }
                    Type::String => {
                        self.unify(&i, &Type::Int, *span)?;
                        Ok(Type::String)
                    }
                    Type::Dyn | Type::Var(_) => Ok(Type::Dyn),
                    other => Err(TypeError::new(*span, format!("cannot index {other}"))),
                }
            }
            Expr::Field { target, field, .. } => {
                let t = self.check_expr(target)?;
                match self.resolve(&t).without_ownership() {
                    Type::Struct { name, fields } => {
                        if let Some((_, ty)) = fields.iter().find(|(n, _)| n == &field.name) {
                            Ok(ty.clone())
                        } else if fields.is_empty() {
                            if let Some(Type::Struct { fields, .. }) = self.structs.get(&name) {
                                fields
                                    .iter()
                                    .find(|(n, _)| n == &field.name)
                                    .map(|(_, ty)| ty.clone())
                                    .ok_or_else(|| {
                                        TypeError::new(
                                            field.span,
                                            format!("no field `{}` on {name}", field.name),
                                        )
                                    })
                            } else {
                                Ok(Type::Dyn)
                            }
                        } else {
                            Err(TypeError::new(
                                field.span,
                                format!("no field `{}` on {name}", field.name),
                            ))
                        }
                    }
                    Type::Module { name, members } => members
                        .iter()
                        .find(|(n, _)| n == &field.name)
                        .map(|(_, ty)| ty.clone())
                        .ok_or_else(|| {
                            TypeError::with_help(
                                field.span,
                                format!("module `{name}` has no export `{}`", field.name),
                                format!(
                                    "if `{}` exists in `{name}.flk`, mark it `pub`",
                                    field.name
                                ),
                            )
                        }),
                    Type::Enum { name, variants } => {
                        let Some((_, fields)) = variants.iter().find(|(n, _)| n == &field.name)
                        else {
                            let listed: Vec<_> = variants.iter().map(|(n, _)| n.as_str()).collect();
                            return Err(TypeError::with_help(
                                field.span,
                                format!("enum `{name}` has no variant `{}`", field.name),
                                format!("available variants: {}", listed.join(", ")),
                            ));
                        };
                        if fields.is_empty() {
                            Ok(Type::Enum { name, variants })
                        } else {
                            Ok(Type::function(
                                fields.clone(),
                                Type::Enum { name, variants },
                                crate::effects::EffectSet::from_names(
                                    std::iter::empty::<&str>(),
                                    false,
                                ),
                            ))
                        }
                    }
                    Type::Dyn | Type::Var(_) => Ok(Type::Dyn),
                    other => Err(TypeError::new(
                        field.span,
                        format!("cannot access field `{}` on {other}", field.name),
                    )),
                }
            }
            Expr::Range { start, end, span } => {
                let s = self.check_expr(start)?;
                let e = self.check_expr(end)?;
                self.unify(&s, &Type::Int, *span)?;
                self.unify(&e, &Type::Int, *span)?;
                Ok(Type::Range)
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                span,
            } => {
                let c = self.check_expr(cond)?;
                self.unify(&c, &Type::Bool, *span)?;
                let then_ty = self.check_block(then_block)?;
                if let Some(els) = else_block {
                    let else_ty = self.check_expr(els)?;
                    self.unify(&then_ty, &else_ty, *span)
                } else {
                    Ok(then_ty)
                }
            }
            Expr::Block(b) => self.check_block(b),
            Expr::Match {
                scrutinee,
                arms,
                span,
            } => {
                let scrut_ty = self.check_expr(scrutinee)?;
                if arms.is_empty() {
                    return Err(TypeError::new(*span, "`match` needs at least one arm"));
                }
                let mut result: Option<Type> = None;
                for arm in arms {
                    self.push_scope();
                    self.bind_pattern(&arm.pattern, &scrut_ty)?;
                    let body_ty = self.check_expr(&arm.body)?;
                    self.pop_scope();
                    result = Some(match result {
                        None => body_ty,
                        Some(prev) => self.unify(&prev, &body_ty, arm.span)?,
                    });
                }
                self.check_match_exhaustive(&scrut_ty, arms, *span)?;
                Ok(result.unwrap())
            }
            Expr::StructInit { name, fields, span } => {
                let st = self.structs.get(&name.name).cloned().ok_or_else(|| {
                    TypeError::new(*span, format!("unknown struct `{}`", name.name))
                })?;
                let Type::Struct {
                    fields: decl,
                    name: n,
                } = st
                else {
                    return Err(TypeError::new(*span, "not a struct"));
                };
                for (field, value) in fields {
                    let vt = self.check_expr(value)?;
                    if let Some((_, dt)) = decl.iter().find(|(nm, _)| nm == &field.name) {
                        self.unify(dt, &vt, value.span())?;
                    } else {
                        return Err(TypeError::new(
                            field.span,
                            format!("unknown field `{}` on {n}", field.name),
                        ));
                    }
                }
                Ok(Type::Struct {
                    name: n,
                    fields: decl,
                })
            }
        }
    }

    fn check_binary(
        &mut self,
        op: BinOp,
        left: &Expr,
        right: &Expr,
        span: Span,
    ) -> Result<Type, TypeError> {
        let l = self.check_expr(left)?;
        let r = self.check_expr(right)?;
        match op {
            BinOp::Add => {
                let l = self.resolve(&l);
                let r = self.resolve(&r);
                match (l.without_ownership(), r.without_ownership()) {
                    (Type::String, _) | (_, Type::String) => Ok(Type::String),
                    (Type::List(a), Type::List(b)) => Ok(Type::list(self.unify(&a, &b, span)?)),
                    (Type::Int, Type::Int) => Ok(Type::Int),
                    (Type::Float, Type::Float)
                    | (Type::Int, Type::Float)
                    | (Type::Float, Type::Int) => Ok(Type::Float),
                    (Type::Dyn, _) | (_, Type::Dyn) => Ok(Type::Dyn),
                    (Type::Var(_), Type::Var(_)) => {
                        self.unify(&l, &Type::Int, span)?;
                        self.unify(&r, &Type::Int, span)?;
                        Ok(Type::Int)
                    }
                    (Type::Var(_), other) | (other, Type::Var(_)) => {
                        self.unify(&Type::Int, &other, span)?;
                        Ok(Type::Int)
                    }
                    (a, b) => Err(TypeError::new(span, format!("cannot add {a} and {b}"))),
                }
            }
            BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => numeric_result(self, &l, &r, span),
            BinOp::Eq | BinOp::Ne => {
                self.unify(&l, &r, span)?;
                Ok(Type::Bool)
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let _ = numeric_result(self, &l, &r, span)?;
                Ok(Type::Bool)
            }
            BinOp::And | BinOp::Or => {
                self.unify(&l, &Type::Bool, span)?;
                self.unify(&r, &Type::Bool, span)?;
                Ok(Type::Bool)
            }
        }
    }

    fn check_effects(&mut self, program: &Program) -> Result<(), TypeError> {
        let mut visiting = HashSet::new();
        let mut done = HashSet::new();
        let fns: Vec<&FnDecl> = program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Fn(f) => Some(f),
                _ => None,
            })
            .collect();
        for func in &fns {
            self.infer_fn_effects(func, &fns, &mut visiting, &mut done)?;
        }
        Ok(())
    }

    fn infer_fn_effects(
        &mut self,
        func: &FnDecl,
        fns: &[&FnDecl],
        visiting: &mut HashSet<String>,
        done: &mut HashSet<String>,
    ) -> Result<EffectSet, TypeError> {
        if let Some(existing) = done.get(&func.name.name) {
            let _ = existing;
            return Ok(self.fn_effects(&func.name.name));
        }
        if visiting.contains(&func.name.name) {
            return Ok(self.fn_effects(&func.name.name));
        }
        visiting.insert(func.name.name.clone());

        let mut used = EffectSet::unspecified();
        self.collect_block_effects(&func.body, fns, visiting, done, &mut used)?;

        visiting.remove(&func.name.name);
        done.insert(func.name.name.clone());

        let declared = self.fn_effects(&func.name.name);
        let allowed = if func.name.name == "main" && !declared.specified {
            EffectSet::top_level()
        } else if declared.specified {
            declared.clone()
        } else {
            used.clone()
        };

        if !used.is_subset(&allowed) {
            let extra: Vec<_> = used
                .iter()
                .filter(|e| !allowed.contains(e))
                .map(|e| e.to_string())
                .collect();
            return Err(TypeError::new(
                func.name.span,
                format!(
                    "function `{}` performs effects [{}] not declared in `{}`",
                    func.name.name,
                    extra.join(" + "),
                    if declared.specified {
                        declared.to_string()
                    } else {
                        "pure".into()
                    }
                ),
            ));
        }

        let stored = if declared.specified {
            declared
        } else if func.name.name == "main" {
            used
        } else {
            used
        };
        self.set_fn_effects(&func.name.name, stored.clone());
        Ok(stored)
    }

    fn fn_effects(&self, name: &str) -> EffectSet {
        match self.functions.get(name) {
            Some(Type::Fn { effects, .. }) => effects.clone(),
            _ => EffectSet::unspecified(),
        }
    }

    fn set_fn_effects(&mut self, name: &str, effects: EffectSet) {
        if let Some(Type::Fn { effects: slot, .. }) = self.functions.get_mut(name) {
            *slot = effects;
        }
    }

    fn collect_block_effects(
        &mut self,
        block: &Block,
        fns: &[&FnDecl],
        visiting: &mut HashSet<String>,
        done: &mut HashSet<String>,
        used: &mut EffectSet,
    ) -> Result<(), TypeError> {
        for stmt in &block.stmts {
            self.collect_stmt_effects(stmt, fns, visiting, done, used)?;
        }
        if let Some(tail) = &block.tail {
            self.collect_expr_effects(tail, fns, visiting, done, used)?;
        }
        Ok(())
    }

    fn collect_stmt_effects(
        &mut self,
        stmt: &Stmt,
        fns: &[&FnDecl],
        visiting: &mut HashSet<String>,
        done: &mut HashSet<String>,
        used: &mut EffectSet,
    ) -> Result<(), TypeError> {
        match stmt {
            Stmt::Let(s) | Stmt::Var(s) => {
                self.collect_expr_effects(&s.value, fns, visiting, done, used)
            }
            Stmt::Return { value: Some(v), .. } => {
                self.collect_expr_effects(v, fns, visiting, done, used)
            }
            Stmt::Return { .. } | Stmt::Break { .. } | Stmt::Continue { .. } => Ok(()),
            Stmt::While { cond, body, .. } => {
                self.collect_expr_effects(cond, fns, visiting, done, used)?;
                self.collect_block_effects(body, fns, visiting, done, used)
            }
            Stmt::For { iter, body, .. } => {
                self.collect_expr_effects(iter, fns, visiting, done, used)?;
                self.collect_block_effects(body, fns, visiting, done, used)
            }
            Stmt::Loop { body, .. } => self.collect_block_effects(body, fns, visiting, done, used),
            Stmt::Expr(e) => self.collect_expr_effects(e, fns, visiting, done, used),
        }
    }

    fn collect_expr_effects(
        &mut self,
        expr: &Expr,
        fns: &[&FnDecl],
        visiting: &mut HashSet<String>,
        done: &mut HashSet<String>,
        used: &mut EffectSet,
    ) -> Result<(), TypeError> {
        match expr {
            Expr::Call { callee, args, .. } => {
                for arg in args {
                    self.collect_expr_effects(arg, fns, visiting, done, used)?;
                }
                self.collect_expr_effects(callee, fns, visiting, done, used)?;
                if let Expr::Ident(id) = callee.as_ref() {
                    let callee_effects =
                        if let Some(f) = fns.iter().find(|f| f.name.name == id.name) {
                            self.infer_fn_effects(f, fns, visiting, done)?
                        } else {
                            self.fn_effects(&id.name)
                        };
                    used.union_with(&callee_effects);
                } else {
                    match self.check_expr(callee) {
                        Ok(ty) => match self.resolve(&ty) {
                            Type::Fn { effects, .. } => used.union_with(&effects),
                            Type::Dyn | Type::Var(_) => used.union_with(&EffectSet::top_level()),
                            _ => {}
                        },
                        Err(_) => {}
                    }
                }
                Ok(())
            }
            Expr::Spawn { call, .. } => {
                used.insert(Effect::Conc);
                self.collect_expr_effects(call, fns, visiting, done, used)
            }
            Expr::Await { task, .. } => {
                used.insert(Effect::Conc);
                self.collect_expr_effects(task, fns, visiting, done, used)
            }
            Expr::Try { expr, .. } => self.collect_expr_effects(expr, fns, visiting, done, used),
            Expr::Interpolated { parts, .. } => {
                for part in parts {
                    if let InterpPart::Expr(e) = part {
                        self.collect_expr_effects(e, fns, visiting, done, used)?;
                    }
                }
                Ok(())
            }
            Expr::List { elements, .. } => {
                for e in elements {
                    self.collect_expr_effects(e, fns, visiting, done, used)?;
                }
                Ok(())
            }
            Expr::Map { entries, .. } => {
                for (k, v) in entries {
                    self.collect_expr_effects(k, fns, visiting, done, used)?;
                    self.collect_expr_effects(v, fns, visiting, done, used)?;
                }
                Ok(())
            }
            Expr::Unary { expr, .. } => self.collect_expr_effects(expr, fns, visiting, done, used),
            Expr::Binary { left, right, .. }
            | Expr::Assign {
                target: left,
                value: right,
                ..
            } => {
                self.collect_expr_effects(left, fns, visiting, done, used)?;
                self.collect_expr_effects(right, fns, visiting, done, used)
            }
            Expr::Index { target, index, .. } => {
                self.collect_expr_effects(target, fns, visiting, done, used)?;
                self.collect_expr_effects(index, fns, visiting, done, used)
            }
            Expr::Field { target, .. } => {
                self.collect_expr_effects(target, fns, visiting, done, used)
            }
            Expr::Range { start, end, .. } => {
                self.collect_expr_effects(start, fns, visiting, done, used)?;
                self.collect_expr_effects(end, fns, visiting, done, used)
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.collect_expr_effects(cond, fns, visiting, done, used)?;
                self.collect_block_effects(then_block, fns, visiting, done, used)?;
                if let Some(els) = else_block {
                    self.collect_expr_effects(els, fns, visiting, done, used)?;
                }
                Ok(())
            }
            Expr::Block(b) => self.collect_block_effects(b, fns, visiting, done, used),
            Expr::StructInit { fields, .. } => {
                for (_, v) in fields {
                    self.collect_expr_effects(v, fns, visiting, done, used)?;
                }
                Ok(())
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.collect_expr_effects(scrutinee, fns, visiting, done, used)?;
                for arm in arms {
                    self.collect_expr_effects(&arm.body, fns, visiting, done, used)?;
                }
                Ok(())
            }
            Expr::Literal { .. } | Expr::Ident(_) => Ok(()),
        }
    }
}

fn literal_type(literal: &Literal) -> Type {
    match literal {
        Literal::Nil => Type::Nil,
        Literal::Bool(_) => Type::Bool,
        Literal::Int(_) => Type::Int,
        Literal::Float(_) => Type::Float,
        Literal::String(_) => Type::String,
    }
}

fn pattern_key(pattern: &flake_ast::Pattern) -> Option<String> {
    match pattern {
        flake_ast::Pattern::Literal { value, .. } => Some(match value {
            Literal::Nil => "literal:nil".into(),
            Literal::Bool(value) => format!("literal:bool:{value}"),
            Literal::Int(value) => format!("literal:int:{value}"),
            Literal::Float(value) => format!("literal:float:{}", value.to_bits()),
            Literal::String(value) => format!("literal:string:{value:?}"),
        }),
        flake_ast::Pattern::Variant { ty, variant, .. } => Some(format!(
            "variant:{}:{}",
            ty.as_ref().map_or("", |name| name.name.as_str()),
            variant.name
        )),
        flake_ast::Pattern::Wildcard { .. } | flake_ast::Pattern::Ident(_) => None,
    }
}

fn numeric_result(
    checker: &mut Checker,
    l: &Type,
    r: &Type,
    span: Span,
) -> Result<Type, TypeError> {
    let l = checker.resolve(l).without_ownership();
    let r = checker.resolve(r).without_ownership();
    match (l, r) {
        (Type::Int, Type::Int) => Ok(Type::Int),
        (Type::Float, Type::Float) | (Type::Int, Type::Float) | (Type::Float, Type::Int) => {
            Ok(Type::Float)
        }
        (Type::Dyn, _) | (_, Type::Dyn) => Ok(Type::Dyn),
        (Type::Var(i), Type::Var(j)) => {
            checker.unify(&Type::Var(i), &Type::Int, span)?;
            checker.unify(&Type::Var(j), &Type::Int, span)?;
            Ok(Type::Int)
        }
        (Type::Var(i), other) | (other, Type::Var(i)) => {
            checker.unify(&Type::Int, &other, span)?;
            checker.unify(&Type::Var(i), &Type::Int, span)?;
            Ok(Type::Int)
        }
        (a, b) => Err(TypeError::new(
            span,
            format!("cannot apply arithmetic to {a} and {b}"),
        )),
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

fn suggest_name(name: &str, candidates: &[String]) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for cand in candidates {
        if cand == name {
            continue;
        }
        let d = edit_distance(name, cand);
        if d == 0 || d > 2 {
            continue;
        }
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, cand.as_str()));
        }
    }
    best.map(|(_, s)| s.to_string())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > 2 {
        return 3;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn occurs(id: u32, ty: &Type) -> bool {
    match ty {
        Type::Var(j) => *j == id,
        Type::List(e) | Type::Task(e) | Type::Optional(e) | Type::Owned(e) | Type::Mut(e) => {
            occurs(id, e)
        }
        Type::Ref { inner, .. } => occurs(id, inner),
        Type::Map(k, v) => occurs(id, k) || occurs(id, v),
        Type::Fn { params, ret, .. } => params.iter().any(|p| occurs(id, p)) || occurs(id, ret),
        Type::Module { members, .. } => members.iter().any(|(_, t)| occurs(id, t)),
        _ => false,
    }
}

fn contains_task(ty: &Type) -> bool {
    match ty {
        Type::Task(_) => true,
        Type::List(inner)
        | Type::Optional(inner)
        | Type::Owned(inner)
        | Type::Mut(inner)
        | Type::Ref { inner, .. } => contains_task(inner),
        Type::Map(key, value) => contains_task(key) || contains_task(value),
        Type::Struct { fields, .. } => fields.iter().any(|(_, field)| contains_task(field)),
        Type::Enum { variants, .. } => variants
            .iter()
            .any(|(_, fields)| fields.iter().any(contains_task)),
        Type::Nil
        | Type::Bool
        | Type::Int
        | Type::Float
        | Type::String
        | Type::Fn { .. }
        | Type::Range
        | Type::Dyn
        | Type::Var(_)
        | Type::Module { .. } => false,
    }
}

fn block_definitely_returns(block: &Block) -> bool {
    block.stmts.iter().any(stmt_definitely_returns)
}

fn stmt_definitely_returns(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return { .. } => true,
        Stmt::Expr(expr) => expr_definitely_returns(expr),
        Stmt::Let(_)
        | Stmt::Var(_)
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::While { .. }
        | Stmt::For { .. }
        | Stmt::Loop { .. } => false,
    }
}

fn expr_definitely_returns(expr: &Expr) -> bool {
    match expr {
        Expr::Block(block) => block_definitely_returns(block),
        Expr::If {
            then_block,
            else_block: Some(else_expr),
            ..
        } => block_definitely_returns(then_block) && expr_definitely_returns(else_expr),
        Expr::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|arm| expr_definitely_returns(&arm.body))
        }
        _ => false,
    }
}

/// Parse and type-check `source`, including `import`ed files.
pub fn check(source: &Source) -> Result<Program, CheckError> {
    let graph = load_graph(source)?;
    check_graph(&graph)?;
    Ok(graph.entry().program.clone())
}

fn check_graph(graph: &ModuleGraph) -> Result<(), CheckError> {
    let entry_name = graph.entry().source.name().to_string();
    for module in &graph.modules {
        let mut checker = Checker::new();
        let map_err = |e: TypeError| {
            if module.source.name() == entry_name {
                CheckError::Type(e)
            } else {
                CheckError::TypeIn {
                    origin: module.source.clone(),
                    error: e,
                }
            }
        };
        checker.install_imports(graph, module).map_err(&map_err)?;
        checker.check_program(&module.program).map_err(map_err)?;
    }
    Ok(())
}

/// Type-check an already-parsed program.
pub fn check_program(program: &Program) -> Result<(), TypeError> {
    Checker::new().check_program(program)
}

/// Convenience wrapper for tests and tools.
pub fn check_str(text: &str) -> Result<Program, CheckError> {
    check(&Source::new("<input>", text))
}
