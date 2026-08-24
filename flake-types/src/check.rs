//! Type checking and local inference with gradual `dyn`.

use std::collections::{HashMap, HashSet};

use flake_ast::{
    AssignOp, BinOp, Block, Expr, FnDecl, InterpPart, Item, Literal, Program, Source, Span, Stmt,
    TypeExpr, UnOp,
};
use flake_parser::{ModuleGraph, import_alias, load_graph};

use crate::effects::{Effect, EffectSet};
use crate::error::{CheckError, TypeError};
use crate::ty::Type;

struct Checker {
    subst: Vec<Option<Type>>,
    scopes: Vec<HashMap<String, Type>>,
    structs: HashMap<String, Type>,
    aliases: HashMap<String, Type>,
    functions: HashMap<String, Type>,
    overloaded_builtins: HashSet<String>,
    enums: HashMap<String, Type>,
    ambiguous_imports: HashMap<String, Vec<String>>,
    type_context: Option<String>,
    current_returns: Vec<Type>,
    nursery_scopes: Vec<usize>,
}

impl Checker {
    fn new() -> Self {
        let mut this = Self {
            subst: Vec::new(),
            scopes: vec![HashMap::new()],
            structs: HashMap::new(),
            aliases: HashMap::new(),
            functions: HashMap::new(),
            overloaded_builtins: HashSet::new(),
            enums: HashMap::new(),
            ambiguous_imports: HashMap::new(),
            type_context: None,
            current_returns: Vec::new(),
            nursery_scopes: Vec::new(),
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
        self.functions.insert(
            "keys".into(),
            mk(
                vec![Type::Map(Box::new(Type::Dyn), Box::new(Type::Dyn))],
                Type::list(Type::Dyn),
                &["alloc"],
            ),
        );
        self.functions.insert(
            "values".into(),
            mk(
                vec![Type::Map(Box::new(Type::Dyn), Box::new(Type::Dyn))],
                Type::list(Type::Dyn),
                &["alloc"],
            ),
        );
        self.functions.insert(
            "entries".into(),
            mk(
                vec![Type::Map(Box::new(Type::Dyn), Box::new(Type::Dyn))],
                Type::list(Type::list(Type::Dyn)),
                &["alloc"],
            ),
        );
        self.functions.insert(
            "is_empty".into(),
            mk(vec![Type::Dyn], Type::Bool, &[]),
        );
        self.functions.insert(
            "has_key".into(),
            mk(vec![Type::Dyn, Type::Dyn], Type::Bool, &[]),
        );
        self.functions.insert(
            "cancel".into(),
            mk(vec![Type::Task(Box::new(Type::Dyn))], Type::Nil, &["conc"]),
        );
        self.functions.insert(
            "is_cancelled".into(),
            mk(vec![Type::Task(Box::new(Type::Dyn))], Type::Bool, &[]),
        );
        self.overloaded_builtins.extend(
            [
                "print",
                "assert",
                "abs",
                "min",
                "max",
                "range",
                "keys",
                "values",
                "entries",
                "cancel",
                "is_cancelled",
            ]
            .into_iter()
            .map(str::to_string),
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
        self.ambiguous_imports = graph.ambiguous_imports(module);
        let mut resolved_imports = Vec::new();
        for item in &module.program.items {
            let Item::Import(import) = item else {
                continue;
            };
            let Some(imported) = graph.imported(module, import) else {
                return Err(TypeError::new(
                    import.span,
                    format!("unresolved import `{}`", import.path.name),
                ));
            };
            let alias = import_alias(import).to_string();
            resolved_imports.push((alias.clone(), imported));

            // Predeclare nominal types from every import before lowering any
            // public signature. Import order must not affect name resolution.
            for (item, origin) in graph.exported_items(imported) {
                match item {
                    Item::Struct(st) => {
                        let ty = Type::Struct {
                            name: flake_parser::qualify(&origin.name, &st.name.name),
                            fields: Vec::new(),
                        };
                        self.structs
                            .insert(format!("{alias}.{}", st.name.name), ty.clone());
                        if graph.unqualified_import_is_unambiguous(module, &st.name.name) {
                            self.structs.insert(st.name.name.clone(), ty.clone());
                        }
                    }
                    Item::Enum(en) => {
                        let ty = Type::Enum {
                            name: flake_parser::qualify(&origin.name, &en.name.name),
                            variants: Vec::new(),
                        };
                        self.enums
                            .insert(format!("{alias}.{}", en.name.name), ty.clone());
                        if graph.unqualified_import_is_unambiguous(module, &en.name.name) {
                            self.enums.insert(en.name.name.clone(), ty.clone());
                        }
                    }
                    Item::Fn(_) | Item::Type(_) | Item::Import(_) => {}
                }
            }
        }

        let mut module_members: HashMap<String, Vec<(String, Type)>> = HashMap::new();
        for (alias, imported) in &resolved_imports {
            self.type_context = Some(alias.clone());
            for (item, origin) in graph.exported_items(imported) {
                match item {
                    Item::Type(alias_item) => {
                        let ty = self.lower_type(&alias_item.ty)?;
                        self.aliases
                            .insert(format!("{alias}.{}", alias_item.name.name), ty.clone());
                        if graph.unqualified_import_is_unambiguous(module, &alias_item.name.name) {
                            self.aliases.insert(alias_item.name.name.clone(), ty);
                        }
                    }
                    Item::Struct(st) => {
                        let fields = st
                            .fields
                            .iter()
                            .map(|field| Ok((field.name.name.clone(), self.lower_type(&field.ty)?)))
                            .collect::<Result<Vec<_>, TypeError>>()?;
                        let ty = Type::Struct {
                            name: flake_parser::qualify(&origin.name, &st.name.name),
                            fields,
                        };
                        self.structs
                            .insert(format!("{alias}.{}", st.name.name), ty.clone());
                        if graph.unqualified_import_is_unambiguous(module, &st.name.name) {
                            self.structs.insert(st.name.name.clone(), ty.clone());
                        }
                        module_members
                            .entry(alias.clone())
                            .or_default()
                            .push((st.name.name.clone(), ty));
                    }
                    Item::Enum(en) => {
                        let variants = en
                            .variants
                            .iter()
                            .map(|variant| {
                                let fields = variant
                                    .fields
                                    .iter()
                                    .map(|field| self.lower_type(field))
                                    .collect::<Result<Vec<_>, _>>()?;
                                Ok((variant.name.name.clone(), fields))
                            })
                            .collect::<Result<Vec<_>, TypeError>>()?;
                        let ty = Type::Enum {
                            name: flake_parser::qualify(&origin.name, &en.name.name),
                            variants,
                        };
                        self.enums
                            .insert(format!("{alias}.{}", en.name.name), ty.clone());
                        if graph.unqualified_import_is_unambiguous(module, &en.name.name) {
                            self.enums.insert(en.name.name.clone(), ty.clone());
                        }
                        module_members
                            .entry(alias.clone())
                            .or_default()
                            .push((en.name.name.clone(), ty));
                    }
                    Item::Fn(_) | Item::Import(_) => {}
                }
            }
        }

        for (alias, imported) in &resolved_imports {
            self.type_context = Some(alias.clone());
            for (item, _origin) in graph.exported_items(imported) {
                let Item::Fn(func) = item else {
                    continue;
                };
                let ty = self.lower_fn_type(func)?;
                if graph.unqualified_import_is_unambiguous(module, &func.name.name)
                    && !self.functions.contains_key(&func.name.name)
                {
                    self.functions.insert(func.name.name.clone(), ty.clone());
                }
                module_members
                    .entry(alias.clone())
                    .or_default()
                    .push((func.name.name.clone(), ty));
            }
        }
        self.type_context = None;

        for (alias, imported) in resolved_imports {
            self.define(
                alias.clone(),
                Type::Module {
                    name: imported.name.clone(),
                    members: module_members.remove(&alias).unwrap_or_default(),
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

    fn lookup_scope_depth(&self, name: &str) -> Option<usize> {
        for (depth, scope) in self.scopes.iter().enumerate().rev() {
            if scope.contains_key(name) {
                return Some(depth);
            }
        }
        None
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

    fn ambiguous_import_error(&self, name: &str, span: Span) -> Option<TypeError> {
        let aliases = self.ambiguous_imports.get(name)?;
        let choices = aliases
            .iter()
            .map(|alias| format!("`{alias}.{name}`"))
            .collect::<Vec<_>>()
            .join(" or ");
        Some(TypeError::with_help(
            span,
            format!("ambiguous imported name `{name}`"),
            format!("use a qualified name: {choices}"),
        ))
    }

    fn check_program(&mut self, program: &Program) -> Result<(), TypeError> {
        validate_public_api(program)?;
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
                self.overloaded_builtins.remove(&func.name.name);
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
                let resolved = self.resolve(ty).without_ownership();
                if let Type::Enum { variants, .. } = &resolved {
                    if id.name.chars().next().is_some_and(|c| c.is_uppercase())
                        && variants.iter().any(|(v, f)| v == &id.name && f.is_empty())
                    {
                        return Ok(());
                    }
                }
                self.define(id.name.clone(), ty.clone());
                Ok(())
            }
            flake_ast::Pattern::List { patterns, span } => {
                let resolved = self.resolve(ty).without_ownership();
                let elem_ty = match resolved {
                    Type::List(elem) => *elem,
                    Type::Dyn | Type::Var(_) => Type::Dyn,
                    other => {
                        return Err(TypeError::with_help(
                            *span,
                            format!("cannot match list pattern on `{other}`"),
                            "list patterns `[...]` can only match on List values",
                        ));
                    }
                };
                for p in patterns {
                    self.bind_pattern(p, &elem_ty)?;
                }
                Ok(())
            }
            flake_ast::Pattern::Variant {
                ty: enum_name,
                variant,
                fields: sub_pats,
                span,
            } => {
                let enum_ty = if let Some(n) = enum_name {
                    self.enums.get(&n.name).cloned().ok_or_else(|| {
                        TypeError::new(*span, format!("unknown enum `{}`", n.name))
                    })?
                } else {
                    let resolved = self.resolve(ty);
                    match resolved.without_ownership() {
                        Type::Enum { .. } => resolved,
                        _ => {
                            let candidates: Vec<_> = self
                                .enums
                                .values()
                                .filter(|t| match t.without_ownership() {
                                    Type::Enum { variants, .. } => {
                                        variants.iter().any(|(v, _)| v == &variant.name)
                                    }
                                    _ => false,
                                })
                                .cloned()
                                .collect();
                            if candidates.len() == 1 {
                                candidates[0].clone()
                            } else {
                                resolved
                            }
                        }
                    }
                };
                self.unify(ty, &enum_ty, *span)?;
                let Type::Enum { name, variants } = enum_ty.without_ownership() else {
                    return Err(TypeError::with_help(
                        *span,
                        "match variant on a non-enum value",
                        "use `Enum.Variant` patterns only when matching an enum",
                    ));
                };
                let Some((_, field_types)) = variants.iter().find(|(n, _)| n == &variant.name) else {
                    let listed: Vec<_> = variants.iter().map(|(n, _)| n.as_str()).collect();
                    return Err(TypeError::with_help(
                        variant.span,
                        format!("enum `{name}` has no variant `{}`", variant.name),
                        format!("available variants: {}", listed.join(", ")),
                    ));
                };
                if sub_pats.len() != field_types.len() {
                    return Err(TypeError::with_help(
                        *span,
                        format!(
                            "variant `{}` expects {} field(s), got {}",
                            variant.name,
                            field_types.len(),
                            sub_pats.len()
                        ),
                        "match each tuple field, or use `_` for a field you do not need",
                    ));
                }
                for (sub_pat, ft) in sub_pats.iter().zip(field_types.iter()) {
                    self.bind_pattern(sub_pat, ft)?;
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
        let resolved_scrut = self.resolve(scrut_ty).without_ownership();
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
                flake_ast::Pattern::Wildcard { .. } => {
                    catch_all = true;
                }
                flake_ast::Pattern::Ident(id) => {
                    if let Type::Enum { variants, .. } = &resolved_scrut {
                        if id.name.chars().next().is_some_and(|c| c.is_uppercase())
                            && variants.iter().any(|(v, f)| v == &id.name && f.is_empty())
                        {
                            // It's a 0-field variant match
                            if let Some(key) = pattern_key(&arm.pattern) {
                                if !seen.insert(key) {
                                    return Err(TypeError::with_help(
                                        arm.span,
                                        "unreachable duplicate match arm",
                                        "remove the duplicate pattern",
                                    ));
                                }
                            }
                            continue;
                        }
                    }
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
        match resolved_scrut {
            Type::Enum { name, variants } => {
                let mut covered = HashSet::new();
                for arm in arms {
                    match &arm.pattern {
                        flake_ast::Pattern::Variant { variant, .. } => {
                            covered.insert(variant.name.as_str());
                        }
                        flake_ast::Pattern::Ident(id)
                            if variants.iter().any(|(v, f)| v == &id.name && f.is_empty()) =>
                        {
                            covered.insert(id.name.as_str());
                        }
                        _ => {}
                    }
                }
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
                    let contextual = self.type_context.as_ref().and_then(|alias| {
                        let qualified = format!("{alias}.{other}");
                        self.aliases
                            .get(&qualified)
                            .or_else(|| self.structs.get(&qualified))
                            .or_else(|| self.enums.get(&qualified))
                            .cloned()
                    });
                    if let Some(contextual) = contextual {
                        contextual
                    } else if let Some(alias) = self.aliases.get(other) {
                        alias.clone()
                    } else if let Some(st) = self.structs.get(other) {
                        st.clone()
                    } else if let Some(en) = self.enums.get(other) {
                        en.clone()
                    } else if let Some(error) = self.ambiguous_import_error(other, *span) {
                        return Err(error);
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
                if let Some(error) = self.ambiguous_import_error(&id.name, id.span) {
                    return error;
                }
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
                if let Some(&current_nursery_scope) = self.nursery_scopes.last() {
                    if let Some(root_var) = root_variable_name(target) {
                        if let Some(var_depth) = self.lookup_scope_depth(root_var) {
                            if var_depth < current_nursery_scope {
                                let resolved_val = self.resolve(&v_ty).without_ownership();
                                if contains_task(&resolved_val) {
                                    return Err(TypeError::with_help(
                                        *span,
                                        "cannot assign task handle to variable defined outside the nursery",
                                        "tasks must be awaited or handled within their enclosing nursery",
                                    ));
                                }
                            }
                        }
                    }
                }
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
                        if let Some(name) = self.overloaded_builtin_name(callee) {
                            return self.check_overloaded_builtin(name, args, *span);
                        }
                        if params.len() != args.len() {
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
                                    "if `{}` exists in module `{name}`, mark it `pub`",
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
            Expr::Nursery { body, span } => {
                let nursery_scope = self.scopes.len();
                self.nursery_scopes.push(nursery_scope);
                let res = self.check_block(body);
                self.nursery_scopes.pop();
                let res = res?;
                let resolved = self.resolve(&res).without_ownership();
                if contains_task(&resolved) {
                    return Err(TypeError::with_help(
                        *span,
                        "task handle cannot escape its nursery",
                        "await or drop the task before leaving the nursery block",
                    ));
                }
                Ok(res)
            }
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

    fn overloaded_builtin_name<'a>(&self, callee: &'a Expr) -> Option<&'a str> {
        let Expr::Ident(id) = callee else {
            return None;
        };
        if self.scopes.iter().any(|scope| scope.contains_key(&id.name)) {
            return None;
        }
        self.overloaded_builtins
            .contains(&id.name)
            .then_some(id.name.as_str())
    }

    fn check_overloaded_builtin(
        &mut self,
        name: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<Type, TypeError> {
        match name {
            "print" => {
                for arg in args {
                    self.check_expr(arg)?;
                }
                Ok(Type::Nil)
            }
            "assert" => {
                self.check_arity_range(name, args.len(), 1, Some(2), span)?;
                let condition = self.check_expr(&args[0])?;
                self.unify(&Type::Bool, &condition, args[0].span())?;
                if let Some(message) = args.get(1) {
                    let message_ty = self.check_expr(message)?;
                    self.unify(&Type::String, &message_ty, message.span())?;
                }
                Ok(Type::Nil)
            }
            "abs" => {
                self.check_arity_range(name, args.len(), 1, Some(1), span)?;
                self.check_numeric_arg(name, &args[0])
            }
            "min" | "max" => {
                self.check_arity_range(name, args.len(), 2, None, span)?;
                let mut result = self.check_numeric_arg(name, &args[0])?;
                for arg in &args[1..] {
                    let ty = self.check_numeric_arg(name, arg)?;
                    result = self.unify(&result, &ty, arg.span())?;
                }
                Ok(self.resolve(&result))
            }
            "range" => {
                self.check_arity_range(name, args.len(), 1, Some(2), span)?;
                for arg in args {
                    let ty = self.check_expr(arg)?;
                    self.unify(&Type::Int, &ty, arg.span())?;
                }
                Ok(Type::Range)
            }
            "keys" => {
                self.check_arity_range(name, args.len(), 1, Some(1), span)?;
                let map_ty = self.check_expr(&args[0])?;
                match self.resolve(&map_ty).without_ownership() {
                    Type::Map(k, _) => Ok(Type::List(k)),
                    Type::Dyn | Type::Var(_) => Ok(Type::List(Box::new(Type::Dyn))),
                    other => Err(TypeError::with_help(
                        args[0].span(),
                        format!("keys() expected Map, found {other}"),
                        "pass a Map[K, V] to keys()",
                    )),
                }
            }
            "values" => {
                self.check_arity_range(name, args.len(), 1, Some(1), span)?;
                let map_ty = self.check_expr(&args[0])?;
                match self.resolve(&map_ty).without_ownership() {
                    Type::Map(_, v) => Ok(Type::List(v)),
                    Type::Dyn | Type::Var(_) => Ok(Type::List(Box::new(Type::Dyn))),
                    other => Err(TypeError::with_help(
                        args[0].span(),
                        format!("values() expected Map, found {other}"),
                        "pass a Map[K, V] to values()",
                    )),
                }
            }
            "entries" => {
                self.check_arity_range(name, args.len(), 1, Some(1), span)?;
                let map_ty = self.check_expr(&args[0])?;
                match self.resolve(&map_ty).without_ownership() {
                    Type::Map(_k, _v) => Ok(Type::List(Box::new(Type::List(Box::new(Type::Dyn))))),
                    Type::Dyn | Type::Var(_) => Ok(Type::List(Box::new(Type::List(Box::new(Type::Dyn))))),
                    other => Err(TypeError::with_help(
                        args[0].span(),
                        format!("entries() expected Map, found {other}"),
                        "pass a Map[K, V] to entries()",
                    )),
                }
            }
            "cancel" => {
                self.check_arity_range(name, args.len(), 1, Some(1), span)?;
                let task_ty = self.check_expr(&args[0])?;
                match self.resolve(&task_ty).without_ownership() {
                    Type::Task(_) | Type::Dyn | Type::Var(_) => Ok(Type::Nil),
                    other => Err(TypeError::with_help(
                        args[0].span(),
                        format!("cancel() expected Task, found {other}"),
                        "pass a Task handle to cancel()",
                    )),
                }
            }
            "is_cancelled" => {
                self.check_arity_range(name, args.len(), 1, Some(1), span)?;
                let task_ty = self.check_expr(&args[0])?;
                match self.resolve(&task_ty).without_ownership() {
                    Type::Task(_) | Type::Dyn | Type::Var(_) => Ok(Type::Bool),
                    other => Err(TypeError::with_help(
                        args[0].span(),
                        format!("is_cancelled() expected Task, found {other}"),
                        "pass a Task handle to is_cancelled()",
                    )),
                }
            }
            _ => unreachable!("only overloaded builtins reach this method"),
        }
    }

    fn check_numeric_arg(&mut self, name: &str, arg: &Expr) -> Result<Type, TypeError> {
        let ty = self.check_expr(arg)?;
        let resolved = self.resolve(&ty).without_ownership();
        match resolved {
            Type::Int | Type::Float | Type::Dyn | Type::Var(_) => Ok(resolved),
            other => Err(TypeError::with_help(
                arg.span(),
                format!("{name}() expected Int or Float, found {other}"),
                "numeric helpers accept only homogeneous Int or Float arguments",
            )),
        }
    }

    fn check_arity_range(
        &self,
        name: &str,
        actual: usize,
        minimum: usize,
        maximum: Option<usize>,
        span: Span,
    ) -> Result<(), TypeError> {
        let valid = actual >= minimum && maximum.is_none_or(|maximum| actual <= maximum);
        if valid {
            return Ok(());
        }
        let expected = match maximum {
            Some(maximum) if minimum == maximum => format!("{minimum}"),
            Some(maximum) => format!("{minimum} or {maximum}"),
            None => format!("at least {minimum}"),
        };
        Err(TypeError::with_help(
            span,
            format!("{name}() expected {expected} argument(s), got {actual}"),
            format!("use one of the supported `{name}(...)` forms"),
        ))
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
            BinOp::Sub | BinOp::Mul | BinOp::Div => numeric_result(self, &l, &r, span),
            BinOp::Rem => {
                let common = self.unify(&l, &r, span)?;
                match self.resolve(&common).without_ownership() {
                    Type::Int => Ok(Type::Int),
                    Type::Float => Ok(Type::Float),
                    Type::Dyn => Ok(Type::Dyn),
                    Type::Var(id) => {
                        self.unify(&Type::Var(id), &Type::Int, span)?;
                        Ok(Type::Int)
                    }
                    other => Err(TypeError::new(
                        span,
                        format!("remainder expects two Ints or two Floats, found {other}"),
                    )),
                }
            }
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

        let stored = if declared.specified { declared } else { used };
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
                } else if let Ok(ty) = self.check_expr(callee) {
                    match self.resolve(&ty) {
                        Type::Fn { effects, .. } => used.union_with(&effects),
                        Type::Dyn | Type::Var(_) => used.union_with(&EffectSet::top_level()),
                        _ => {}
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
            Expr::Nursery { body, .. } => {
                used.insert(Effect::Conc);
                self.collect_block_effects(body, fns, visiting, done, used)
            }
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

fn validate_public_api(program: &Program) -> Result<(), TypeError> {
    let private_types: HashSet<&str> = program
        .items
        .iter()
        .filter(|item| !item.is_pub())
        .filter_map(|item| match item {
            Item::Struct(st) => Some(st.name.name.as_str()),
            Item::Enum(en) => Some(en.name.name.as_str()),
            Item::Type(alias) => Some(alias.name.name.as_str()),
            Item::Fn(_) | Item::Import(_) => None,
        })
        .collect();
    if private_types.is_empty() {
        return Ok(());
    }

    for item in &program.items {
        if !item.is_pub() {
            continue;
        }
        let (owner, types): (String, Vec<&TypeExpr>) = match item {
            Item::Fn(function) => {
                let mut types: Vec<_> = function
                    .params
                    .iter()
                    .filter_map(|param| param.ty.as_ref())
                    .collect();
                types.extend(function.return_type.iter());
                (format!("function `{}`", function.name.name), types)
            }
            Item::Struct(st) => (
                format!("struct `{}`", st.name.name),
                st.fields.iter().map(|field| &field.ty).collect(),
            ),
            Item::Enum(en) => (
                format!("enum `{}`", en.name.name),
                en.variants
                    .iter()
                    .flat_map(|variant| variant.fields.iter())
                    .collect(),
            ),
            Item::Type(alias) => (format!("type alias `{}`", alias.name.name), vec![&alias.ty]),
            Item::Import(_) => continue,
        };
        for ty in types {
            if let Some((name, span)) = private_type_in(ty, &private_types) {
                return Err(TypeError::with_help(
                    span,
                    format!("public {owner} exposes private type `{name}`"),
                    format!("mark `{name}` `pub`, or keep {owner} private"),
                ));
            }
        }
    }
    Ok(())
}

fn private_type_in<'a>(ty: &'a TypeExpr, private_types: &HashSet<&str>) -> Option<(&'a str, Span)> {
    match ty {
        TypeExpr::Named { name, args, .. } => {
            if !name.name.contains('.') && private_types.contains(name.name.as_str()) {
                return Some((name.name.as_str(), name.span));
            }
            args.iter()
                .find_map(|argument| private_type_in(argument, private_types))
        }
        TypeExpr::List { element, .. }
        | TypeExpr::Optional { inner: element, .. }
        | TypeExpr::Owned { inner: element, .. }
        | TypeExpr::Ref { inner: element, .. }
        | TypeExpr::Mut { inner: element, .. } => private_type_in(element, private_types),
        TypeExpr::Fn { params, ret, .. } => params
            .iter()
            .find_map(|param| private_type_in(param, private_types))
            .or_else(|| {
                ret.as_deref()
                    .and_then(|return_type| private_type_in(return_type, private_types))
            }),
        TypeExpr::Dyn { .. } => None,
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
        flake_ast::Pattern::Variant {
            ty,
            variant,
            fields,
            ..
        } => {
            let mut key = format!(
                "variant:{}:{}",
                ty.as_ref().map_or("", |name| name.name.as_str()),
                variant.name
            );
            if !fields.is_empty() {
                key.push('(');
                for (i, f) in fields.iter().enumerate() {
                    if i > 0 {
                        key.push(',');
                    }
                    if let Some(sk) = pattern_key(f) {
                        key.push_str(&sk);
                    } else {
                        key.push('_');
                    }
                }
                key.push(')');
            }
            Some(key)
        }
        flake_ast::Pattern::List { patterns, .. } => {
            let mut key = "list:[".to_string();
            for (i, p) in patterns.iter().enumerate() {
                if i > 0 {
                    key.push(',');
                }
                if let Some(sk) = pattern_key(p) {
                    key.push_str(&sk);
                } else {
                    key.push('_');
                }
            }
            key.push(']');
            Some(key)
        }
        flake_ast::Pattern::Wildcard { .. } => None,
        flake_ast::Pattern::Ident(id) => {
            if id.name.chars().next().is_some_and(|c| c.is_uppercase()) {
                Some(format!("variant::_:{}", id.name))
            } else {
                None
            }
        }
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

fn root_variable_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Ident(id) => Some(id.name.as_str()),
        Expr::Field { target, .. } | Expr::Index { target, .. } => root_variable_name(target),
        _ => None,
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
        Expr::Block(block) | Expr::Nursery { body: block, .. } => block_definitely_returns(block),
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
