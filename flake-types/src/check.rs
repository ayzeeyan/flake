//! Type checking and local inference with gradual `dyn`.

use std::collections::{HashMap, HashSet};

use flake_ast::{
    AssignOp, BinOp, Block, Expr, FnDecl, ImplDecl, InterpPart, Item, Literal, Program, Source,
    Span, Stmt, TraitDecl, TypeExpr, TypeParam, UnOp,
};
use flake_parser::{ModuleGraph, import_alias, load_graph};

use crate::effects::{Effect, EffectSet};
use crate::error::{CheckError, TypeError};
use crate::ty::Type;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TraitMethodDef {
    pub name: String,
    pub type_params: Vec<String>,
    pub param_bounds: Vec<(String, Vec<String>)>,
    pub params: Vec<Type>,
    pub ret: Type,
    pub effects: flake_ast::EffectSet,
    pub span: Span,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TraitDef {
    pub name: String,
    pub methods: Vec<TraitMethodDef>,
    pub span: Span,
}

struct Checker {
    subst: Vec<Option<Type>>,
    scopes: Vec<HashMap<String, Type>>,
    structs: HashMap<String, Type>,
    struct_type_params: HashMap<String, Vec<String>>,
    aliases: HashMap<String, Type>,
    alias_type_params: HashMap<String, Vec<String>>,
    functions: HashMap<String, Type>,
    fn_type_params: HashMap<String, Vec<String>>,
    overloaded_builtins: HashSet<String>,
    enums: HashMap<String, Type>,
    enum_type_params: HashMap<String, Vec<String>>,
    fn_param_bounds: HashMap<String, Vec<(String, Vec<String>)>>,
    struct_param_bounds: HashMap<String, Vec<(String, Vec<String>)>>,
    enum_param_bounds: HashMap<String, Vec<(String, Vec<String>)>>,
    alias_param_bounds: HashMap<String, Vec<(String, Vec<String>)>>,
    traits: HashMap<String, TraitDef>,
    impls: Vec<TraitImpl>,
    ambiguous_imports: HashMap<String, Vec<String>>,
    type_context: Option<String>,
    current_returns: Vec<Type>,
    nursery_scopes: Vec<usize>,
    active_type_params: HashSet<String>,
    active_param_bounds: HashMap<String, Vec<String>>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct TraitImpl {
    trait_name: String,
    type_ctor: String,
    target_type: Type,
    param_bounds: Vec<(String, Vec<String>)>,
    methods: HashMap<String, FnDecl>,
}

fn substitute_type(ty: &Type, mapping: &HashMap<String, Type>) -> Type {
    match ty {
        Type::Param(name) => mapping.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::List(elem) => Type::List(Box::new(substitute_type(elem, mapping))),
        Type::Map(k, v) => Type::Map(
            Box::new(substitute_type(k, mapping)),
            Box::new(substitute_type(v, mapping)),
        ),
        Type::Task(res) => Type::Task(Box::new(substitute_type(res, mapping))),
        Type::Optional(inner) => Type::Optional(Box::new(substitute_type(inner, mapping))),
        Type::Owned(inner) => Type::Owned(Box::new(substitute_type(inner, mapping))),
        Type::Mut(inner) => Type::Mut(Box::new(substitute_type(inner, mapping))),
        Type::Ref { mutable, inner } => Type::Ref {
            mutable: *mutable,
            inner: Box::new(substitute_type(inner, mapping)),
        },
        Type::Fn { params, ret, effects } => Type::Fn {
            params: params.iter().map(|p| substitute_type(p, mapping)).collect(),
            ret: Box::new(substitute_type(ret, mapping)),
            effects: effects.clone(),
        },
        Type::Struct { name, fields } => Type::Struct {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(n, t)| (n.clone(), substitute_type(t, mapping)))
                .collect(),
        },
        Type::Enum { name, variants } => Type::Enum {
            name: name.clone(),
            variants: variants
                .iter()
                .map(|(n, ts)| (n.clone(), ts.iter().map(|t| substitute_type(t, mapping)).collect()))
                .collect(),
        },
        Type::Module { name, members } => Type::Module {
            name: name.clone(),
            members: members
                .iter()
                .map(|(n, t)| (n.clone(), substitute_type(t, mapping)))
                .collect(),
        },
        other => other.clone(),
    }
}

fn type_param_bound_list(params: &[TypeParam]) -> Vec<(String, Vec<String>)> {
    params
        .iter()
        .map(|p| {
            (
                p.name.name.clone(),
                p.bounds.iter().map(|b| b.name.clone()).collect(),
            )
        })
        .collect()
}

fn primitive_ctor(ty: &Type) -> &'static str {
    match ty {
        Type::Int => "Int",
        Type::Float => "Float",
        Type::String => "String",
        Type::Bool => "Bool",
        Type::Nil => "Nil",
        _ => "",
    }
}

fn impl_target(ty: &TypeExpr) -> Result<(String, Vec<String>), TypeError> {
    match ty {
        TypeExpr::Named { name, args, .. } => {
            let type_args = args
                .iter()
                .filter_map(|arg| match arg {
                    TypeExpr::Named {
                        name: arg_name,
                        args: nested,
                        ..
                    } if nested.is_empty() => Some(arg_name.name.clone()),
                    _ => None,
                })
                .collect();
            Ok((name.name.clone(), type_args))
        }
        TypeExpr::List { element, .. } => {
            let type_args = match element.as_ref() {
                TypeExpr::Named {
                    name,
                    args,
                    ..
                } if args.is_empty() => vec![name.name.clone()],
                _ => Vec::new(),
            };
            Ok(("List".into(), type_args))
        }
        other => Err(TypeError::new(
            other.span(),
            "cannot implement a trait for this type",
        )),
    }
}

fn recover_param_mapping(original: &Type, inst: &Type) -> HashMap<String, Type> {
    let mut mapping = HashMap::new();
    recover_params(original, inst, &mut mapping);
    mapping
}

fn recover_params(original: &Type, inst: &Type, mapping: &mut HashMap<String, Type>) {
    match (original, inst) {
        (Type::Param(name), other) => {
            mapping.entry(name.clone()).or_insert_with(|| other.clone());
        }
        (Type::List(a), Type::List(b))
        | (Type::Task(a), Type::Task(b))
        | (Type::Optional(a), Type::Optional(b))
        | (Type::Owned(a), Type::Owned(b))
        | (Type::Mut(a), Type::Mut(b)) => recover_params(a, b, mapping),
        (Type::Ref { inner: a, .. }, Type::Ref { inner: b, .. }) => recover_params(a, b, mapping),
        (Type::Map(ak, av), Type::Map(bk, bv)) => {
            recover_params(ak, bk, mapping);
            recover_params(av, bv, mapping);
        }
        (Type::Struct { fields: fa, .. }, Type::Struct { fields: fb, .. }) => {
            for ((_, a), (_, b)) in fa.iter().zip(fb.iter()) {
                recover_params(a, b, mapping);
            }
        }
        (Type::Enum { variants: va, .. }, Type::Enum { variants: vb, .. }) => {
            for ((_, a), (_, b)) in va.iter().zip(vb.iter()) {
                for (x, y) in a.iter().zip(b.iter()) {
                    recover_params(x, y, mapping);
                }
            }
        }
        (Type::Fn { params: pa, ret: ra, .. }, Type::Fn { params: pb, ret: rb, .. }) => {
            for (a, b) in pa.iter().zip(pb.iter()) {
                recover_params(a, b, mapping);
            }
            recover_params(ra, rb, mapping);
        }
        _ => {}
    }
}

fn collect_type_params(ty: &Type, set: &mut HashSet<String>) {
    match ty {
        Type::Param(name) => {
            set.insert(name.clone());
        }
        Type::List(elem) => collect_type_params(elem, set),
        Type::Map(k, v) => {
            collect_type_params(k, set);
            collect_type_params(v, set);
        }
        Type::Task(r) => collect_type_params(r, set),
        Type::Optional(i) => collect_type_params(i, set),
        Type::Owned(i) | Type::Mut(i) => collect_type_params(i, set),
        Type::Ref { inner, .. } => collect_type_params(inner, set),
        Type::Fn { params, ret, .. } => {
            for p in params {
                collect_type_params(p, set);
            }
            collect_type_params(ret, set);
        }
        Type::Struct { fields, .. } => {
            for (_, t) in fields {
                collect_type_params(t, set);
            }
        }
        Type::Enum { variants, .. } => {
            for (_, ts) in variants {
                for t in ts {
                    collect_type_params(t, set);
                }
            }
        }
        Type::Module { members, .. } => {
            for (_, t) in members {
                collect_type_params(t, set);
            }
        }
        _ => {}
    }
}

impl Checker {
    fn new() -> Self {
        let mut this = Self {
            subst: Vec::new(),
            scopes: vec![HashMap::new()],
            structs: HashMap::new(),
            struct_type_params: HashMap::new(),
            aliases: HashMap::new(),
            alias_type_params: HashMap::new(),
            functions: HashMap::new(),
            fn_type_params: HashMap::new(),
            overloaded_builtins: HashSet::new(),
            enums: HashMap::new(),
            enum_type_params: HashMap::new(),
            fn_param_bounds: HashMap::new(),
            struct_param_bounds: HashMap::new(),
            enum_param_bounds: HashMap::new(),
            alias_param_bounds: HashMap::new(),
            traits: ["Eq", "Ord", "Hash"]
                .into_iter()
                .map(|s| {
                    (
                        s.to_string(),
                        TraitDef {
                            name: s.to_string(),
                            methods: Vec::new(),
                            span: Span::DUMMY,
                        },
                    )
                })
                .collect(),
            impls: Vec::new(),
            ambiguous_imports: HashMap::new(),
            type_context: None,
            current_returns: Vec::new(),
            nursery_scopes: Vec::new(),
            active_type_params: HashSet::new(),
            active_param_bounds: HashMap::new(),
        };
        this.install_builtins();
        this
    }

    fn fresh(&mut self) -> Type {
        let id = self.subst.len() as u32;
        self.subst.push(None);
        Type::Var(id)
    }

    fn instantiate_generic(&mut self, ty: &Type) -> Type {
        let mut params = HashSet::new();
        collect_type_params(ty, &mut params);
        if params.is_empty() {
            return ty.clone();
        }
        let mut mapping = HashMap::new();
        for p in params {
            mapping.insert(p, self.fresh());
        }
        substitute_type(ty, &mapping)
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
        self.functions
            .insert("is_empty".into(), mk(vec![Type::Dyn], Type::Bool, &[]));
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
        self.functions.insert(
            "is_completed".into(),
            mk(vec![Type::Task(Box::new(Type::Dyn))], Type::Bool, &[]),
        );
        self.functions.insert(
            "task_status".into(),
            mk(vec![Type::Task(Box::new(Type::Dyn))], Type::String, &[]),
        );
        self.functions.insert(
            "args".into(),
            mk(vec![], Type::list(Type::String), &["io", "alloc"]),
        );
        self.functions.insert(
            "list_dir".into(),
            mk(
                vec![Type::String],
                Type::list(Type::String),
                &["io", "alloc"],
            ),
        );
        self.functions.insert(
            "is_dir".into(),
            mk(vec![Type::String], Type::Bool, &["io"]),
        );
        self.functions.insert(
            "is_file".into(),
            mk(vec![Type::String], Type::Bool, &["io"]),
        );
        self.functions.insert(
            "append_file".into(),
            mk(vec![Type::String, Type::Dyn], Type::Nil, &["io"]),
        );
        self.functions.insert(
            "create_dir".into(),
            mk(vec![Type::String], Type::Bool, &["io"]),
        );
        self.functions.insert(
            "run_cmd".into(),
            mk(
                vec![Type::String],
                Type::list(Type::Dyn),
                &["io", "alloc"],
            ),
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
                "is_completed",
                "task_status",
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
            Type::Struct { name, fields } => Type::Struct {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|(n, t)| (n.clone(), self.resolve(t)))
                    .collect(),
            },
            Type::Enum { name, variants } => Type::Enum {
                name: name.clone(),
                variants: variants
                    .iter()
                    .map(|(n, ts)| (n.clone(), ts.iter().map(|t| self.resolve(t)).collect()))
                    .collect(),
            },
            Type::Param(p) => Type::Param(p.clone()),
            other => other.clone(),
        }
    }

    fn unify(&mut self, a: &Type, b: &Type, span: Span) -> Result<Type, TypeError> {
        let a_full = self.resolve(a);
        let b_full = self.resolve(b);
        match (&a_full, &b_full) {
            (Type::Var(i), Type::Var(j)) if i == j => return Ok(a_full),
            (Type::Var(i), _) => return self.bind(*i, b_full, span),
            (_, Type::Var(i)) => return self.bind(*i, a_full, span),
            _ => {}
        }
        let a = a_full.without_ownership();
        let b = b_full.without_ownership();
        match (a, b) {
            (Type::Dyn, other) | (other, Type::Dyn) => Ok(other),
            (Type::Param(p1), Type::Param(p2)) if p1 == p2 => Ok(Type::Param(p1)),
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
            (
                Type::Struct {
                    name: n1,
                    fields: f1,
                },
                Type::Struct {
                    name: n2,
                    fields: f2,
                },
            ) if n1 == n2 => {
                if f1.is_empty() {
                    Ok(Type::Struct {
                        name: n1,
                        fields: f2,
                    })
                } else if f2.is_empty() {
                    Ok(Type::Struct {
                        name: n1,
                        fields: f1,
                    })
                } else if f1.len() == f2.len() {
                    let mut unified_fields = Vec::new();
                    for ((name1, ty1), (name2, ty2)) in f1.iter().zip(f2.iter()) {
                        if name1 != name2 {
                            return Err(TypeError::new(
                                span,
                                format!("mismatched struct field `{name1}` vs `{name2}` on {n1}"),
                            ));
                        }
                        let u = self.unify(ty1, ty2, span)?;
                        unified_fields.push((name1.clone(), u));
                    }
                    Ok(Type::Struct {
                        name: n1,
                        fields: unified_fields,
                    })
                } else {
                    Ok(Type::Struct {
                        name: n1,
                        fields: f1,
                    })
                }
            }
            (
                Type::Enum {
                    name: n1,
                    variants: v1,
                },
                Type::Enum {
                    name: n2,
                    variants: v2,
                },
            ) if n1 == n2 => {
                if v1.is_empty() {
                    Ok(Type::Enum {
                        name: n1,
                        variants: v2,
                    })
                } else if v2.is_empty() {
                    Ok(Type::Enum {
                        name: n1,
                        variants: v1,
                    })
                } else if v1.len() == v2.len() {
                    let mut unified_variants = Vec::new();
                    for ((name1, types1), (name2, types2)) in v1.iter().zip(v2.iter()) {
                        if name1 != name2 || types1.len() != types2.len() {
                            return Err(TypeError::new(
                                span,
                                format!("mismatched enum variants on {n1}"),
                            ));
                        }
                        let mut u_types = Vec::new();
                        for (t1, t2) in types1.iter().zip(types2.iter()) {
                            u_types.push(self.unify(t1, t2, span)?);
                        }
                        unified_variants.push((name1.clone(), u_types));
                    }
                    Ok(Type::Enum {
                        name: n1,
                        variants: unified_variants,
                    })
                } else {
                    Ok(Type::Enum {
                        name: n1,
                        variants: v1,
                    })
                }
            }
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
                        let tparams: Vec<String> = st.type_params.iter().map(|p| p.name.name.clone()).collect();
                        let bounds = type_param_bound_list(&st.type_params);
                        self.struct_type_params
                            .insert(format!("{alias}.{}", st.name.name), tparams.clone());
                        self.struct_param_bounds
                            .insert(format!("{alias}.{}", st.name.name), bounds.clone());
                        let ty = Type::Struct {
                            name: flake_parser::qualify(&origin.name, &st.name.name),
                            fields: Vec::new(),
                        };
                        self.structs
                            .insert(format!("{alias}.{}", st.name.name), ty.clone());
                        if graph.unqualified_import_is_unambiguous(module, &st.name.name) {
                            self.struct_type_params.insert(st.name.name.clone(), tparams);
                            self.struct_param_bounds.insert(st.name.name.clone(), bounds);
                            self.structs.insert(st.name.name.clone(), ty.clone());
                        }
                    }
                    Item::Enum(en) => {
                        let tparams: Vec<String> = en.type_params.iter().map(|p| p.name.name.clone()).collect();
                        let bounds = type_param_bound_list(&en.type_params);
                        self.enum_type_params
                            .insert(format!("{alias}.{}", en.name.name), tparams.clone());
                        self.enum_param_bounds
                            .insert(format!("{alias}.{}", en.name.name), bounds.clone());
                        let ty = Type::Enum {
                            name: flake_parser::qualify(&origin.name, &en.name.name),
                            variants: Vec::new(),
                        };
                        self.enums
                            .insert(format!("{alias}.{}", en.name.name), ty.clone());
                        if graph.unqualified_import_is_unambiguous(module, &en.name.name) {
                            self.enum_type_params.insert(en.name.name.clone(), tparams);
                            self.enum_param_bounds.insert(en.name.name.clone(), bounds);
                            self.enums.insert(en.name.name.clone(), ty.clone());
                        }
                    }
                    Item::Fn(_) | Item::Type(_) | Item::Import(_) | Item::Trait(_) | Item::Impl(_) => {}
                }
            }
        }

        let mut module_members: HashMap<String, Vec<(String, Type)>> = HashMap::new();
        for (alias, imported) in &resolved_imports {
            self.type_context = Some(alias.clone());
            for (item, origin) in graph.exported_items(imported) {
                match item {
                    Item::Type(alias_item) => {
                        let tparams: Vec<String> = alias_item.type_params.iter().map(|p| p.name.name.clone()).collect();
                        self.alias_type_params
                            .insert(format!("{alias}.{}", alias_item.name.name), tparams.clone());
                        for tp in &tparams {
                            self.active_type_params.insert(tp.clone());
                        }
                        let ty = self.lower_type(&alias_item.ty)?;
                        for tp in &tparams {
                            self.active_type_params.remove(tp);
                        }
                        self.aliases
                            .insert(format!("{alias}.{}", alias_item.name.name), ty.clone());
                        if graph.unqualified_import_is_unambiguous(module, &alias_item.name.name) {
                            self.alias_type_params.insert(alias_item.name.name.clone(), tparams);
                            self.aliases.insert(alias_item.name.name.clone(), ty);
                        }
                    }
                    Item::Struct(st) => {
                        let tparams: Vec<String> = st.type_params.iter().map(|p| p.name.name.clone()).collect();
                        for tp in &tparams {
                            self.active_type_params.insert(tp.clone());
                        }
                        let fields = st
                            .fields
                            .iter()
                            .map(|field| Ok((field.name.name.clone(), self.lower_type(&field.ty)?)))
                            .collect::<Result<Vec<_>, TypeError>>()?;
                        for tp in &tparams {
                            self.active_type_params.remove(tp);
                        }
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
                        let tparams: Vec<String> = en.type_params.iter().map(|p| p.name.name.clone()).collect();
                        for tp in &tparams {
                            self.active_type_params.insert(tp.clone());
                        }
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
                        for tp in &tparams {
                            self.active_type_params.remove(tp);
                        }
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
                    Item::Fn(_) | Item::Import(_) | Item::Trait(_) | Item::Impl(_) => {}
                }
            }
        }

        for (alias, imported) in &resolved_imports {
            self.type_context = Some(alias.clone());
            for (item, _origin) in graph.exported_items(imported) {
                let Item::Fn(func) = item else {
                    continue;
                };
                let tparams: Vec<String> = func.type_params.iter().map(|p| p.name.name.clone()).collect();
                let bounds = type_param_bound_list(&func.type_params);
                self.fn_type_params
                    .insert(format!("{alias}.{}", func.name.name), tparams.clone());
                self.fn_param_bounds
                    .insert(format!("{alias}.{}", func.name.name), bounds.clone());
                for tp in &tparams {
                    self.active_type_params.insert(tp.clone());
                }
                let ty = self.lower_fn_type(func)?;
                for tp in &tparams {
                    self.active_type_params.remove(tp);
                }
                if graph.unqualified_import_is_unambiguous(module, &func.name.name)
                    && !self.functions.contains_key(&func.name.name)
                {
                    self.fn_type_params.insert(func.name.name.clone(), tparams);
                    self.fn_param_bounds
                        .insert(func.name.name.clone(), bounds);
                    self.functions.insert(func.name.name.clone(), ty.clone());
                }
                module_members
                    .entry(alias.clone())
                    .or_default()
                    .push((func.name.name.clone(), ty));
            }
        }
        self.type_context = None;

        for (_alias, imported) in &resolved_imports {
            for item in &imported.program.items {
                match item {
                    Item::Trait(tr) => {
                        let trait_def = self.lower_trait_decl(tr)?;
                        self.traits.insert(tr.name.name.clone(), trait_def);
                    }
                    Item::Impl(imp) => {
                        self.register_impl(imp)?;
                    }
                    _ => {}
                }
            }
        }

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
            if let Item::Trait(tr) = item {
                if self.traits.contains_key(&tr.name.name)
                    && !matches!(tr.name.name.as_str(), "Eq" | "Ord" | "Hash")
                {
                    return Err(TypeError::new(
                        tr.name.span,
                        format!("duplicate trait `{}`", tr.name.name),
                    ));
                }
                let trait_def = self.lower_trait_decl(tr)?;
                self.traits.insert(tr.name.name.clone(), trait_def);
            }
        }
        for item in &program.items {
            match item {
                Item::Type(alias) => {
                    self.check_type_param_bounds(&alias.type_params)?;
                    let tparams: Vec<String> = alias.type_params.iter().map(|p| p.name.name.clone()).collect();
                    self.alias_type_params.insert(alias.name.name.clone(), tparams.clone());
                    self.alias_param_bounds
                        .insert(alias.name.name.clone(), type_param_bound_list(&alias.type_params));
                    for tp in &alias.type_params {
                        self.active_type_params.insert(tp.name.name.clone());
                        self.active_param_bounds.insert(
                            tp.name.name.clone(),
                            tp.bounds.iter().map(|b| b.name.clone()).collect(),
                        );
                    }
                    let ty = self.lower_type(&alias.ty)?;
                    for tp in &alias.type_params {
                        self.active_type_params.remove(&tp.name.name);
                        self.active_param_bounds.remove(&tp.name.name);
                    }
                    self.aliases.insert(alias.name.name.clone(), ty);
                }
                Item::Struct(st) => {
                    self.check_type_param_bounds(&st.type_params)?;
                    let tparams: Vec<String> = st.type_params.iter().map(|p| p.name.name.clone()).collect();
                    self.struct_type_params.insert(st.name.name.clone(), tparams.clone());
                    self.struct_param_bounds
                        .insert(st.name.name.clone(), type_param_bound_list(&st.type_params));
                    for tp in &st.type_params {
                        self.active_type_params.insert(tp.name.name.clone());
                        self.active_param_bounds.insert(
                            tp.name.name.clone(),
                            tp.bounds.iter().map(|b| b.name.clone()).collect(),
                        );
                    }
                    let mut fields = Vec::new();
                    for field in &st.fields {
                        fields.push((field.name.name.clone(), self.lower_type(&field.ty)?));
                    }
                    for tp in &st.type_params {
                        self.active_type_params.remove(&tp.name.name);
                        self.active_param_bounds.remove(&tp.name.name);
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
                    self.check_type_param_bounds(&en.type_params)?;
                    let tparams: Vec<String> = en.type_params.iter().map(|p| p.name.name.clone()).collect();
                    self.enum_type_params.insert(en.name.name.clone(), tparams.clone());
                    self.enum_param_bounds
                        .insert(en.name.name.clone(), type_param_bound_list(&en.type_params));
                    for tp in &en.type_params {
                        self.active_type_params.insert(tp.name.name.clone());
                        self.active_param_bounds.insert(
                            tp.name.name.clone(),
                            tp.bounds.iter().map(|b| b.name.clone()).collect(),
                        );
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
                    for tp in &en.type_params {
                        self.active_type_params.remove(&tp.name.name);
                        self.active_param_bounds.remove(&tp.name.name);
                    }
                    let ty = Type::Enum {
                        name: en.name.name.clone(),
                        variants,
                    };
                    self.enums.insert(en.name.name.clone(), ty);
                }
                Item::Fn(_) | Item::Import(_) | Item::Trait(_) | Item::Impl(_) => {}
            }
        }

        for item in &program.items {
            if let Item::Impl(imp) = item {
                self.register_impl(imp)?;
            }
        }

        for item in &program.items {
            if let Item::Fn(func) = item {
                self.check_type_param_bounds(&func.type_params)?;
                let tparams: Vec<String> = func.type_params.iter().map(|p| p.name.name.clone()).collect();
                self.fn_type_params.insert(func.name.name.clone(), tparams.clone());
                self.fn_param_bounds
                    .insert(func.name.name.clone(), type_param_bound_list(&func.type_params));
                for tp in &func.type_params {
                    self.active_type_params.insert(tp.name.name.clone());
                    self.active_param_bounds.insert(
                        tp.name.name.clone(),
                        tp.bounds.iter().map(|b| b.name.clone()).collect(),
                    );
                }
                let ty = self.lower_fn_type(func)?;
                for tp in &func.type_params {
                    self.active_type_params.remove(&tp.name.name);
                    self.active_param_bounds.remove(&tp.name.name);
                }
                self.overloaded_builtins.remove(&func.name.name);
                self.functions.insert(func.name.name.clone(), ty);
            }
        }

        for item in &program.items {
            match item {
                Item::Fn(func) => self.check_fn(func)?,
                Item::Impl(imp) => self.check_impl_methods(imp)?,
                Item::Import(_)
                | Item::Struct(_)
                | Item::Enum(_)
                | Item::Type(_)
                | Item::Trait(_) => {}
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
                let enum_ty = self.instantiate_generic(&enum_ty);
                self.unify(ty, &enum_ty, *span)?;
                let enum_ty_res = self.resolve(&enum_ty);
                let Type::Enum { name, variants } = enum_ty_res.without_ownership() else {
                    return Err(TypeError::with_help(
                        *span,
                        "match variant on a non-enum value",
                        "use `Enum.Variant` patterns only when matching an enum",
                    ));
                };
                let Some((_, field_types)) = variants.iter().find(|(n, _)| n == &variant.name)
                else {
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
                    if args.is_empty() && self.active_type_params.contains(other) {
                        return Ok(Type::Param(other.to_string()));
                    }
                    let contextual = self.type_context.as_ref().map(|alias| format!("{alias}.{other}"));
                    let (target_key, tparams, base_ty) = if let Some(ref q) = contextual {
                        if let Some(alias) = self.aliases.get(q).cloned() {
                            (
                                q.clone(),
                                self.alias_type_params.get(q).cloned(),
                                alias,
                            )
                        } else if let Some(st) = self.structs.get(q).cloned() {
                            (
                                q.clone(),
                                self.struct_type_params.get(q).cloned(),
                                st,
                            )
                        } else if let Some(en) = self.enums.get(q).cloned() {
                            (
                                q.clone(),
                                self.enum_type_params.get(q).cloned(),
                                en,
                            )
                        } else if let Some(alias) = self.aliases.get(other).cloned() {
                            (
                                other.to_string(),
                                self.alias_type_params.get(other).cloned(),
                                alias,
                            )
                        } else if let Some(st) = self.structs.get(other).cloned() {
                            (
                                other.to_string(),
                                self.struct_type_params.get(other).cloned(),
                                st,
                            )
                        } else if let Some(en) = self.enums.get(other).cloned() {
                            (
                                other.to_string(),
                                self.enum_type_params.get(other).cloned(),
                                en,
                            )
                        } else if let Some(error) = self.ambiguous_import_error(other, *span) {
                            return Err(error);
                        } else {
                            (
                                other.to_string(),
                                None,
                                Type::Struct {
                                    name: other.to_string(),
                                    fields: Vec::new(),
                                },
                            )
                        }
                    } else if let Some(alias) = self.aliases.get(other).cloned() {
                        (
                            other.to_string(),
                            self.alias_type_params.get(other).cloned(),
                            alias,
                        )
                    } else if let Some(st) = self.structs.get(other).cloned() {
                        (
                            other.to_string(),
                            self.struct_type_params.get(other).cloned(),
                            st,
                        )
                    } else if let Some(en) = self.enums.get(other).cloned() {
                        (
                            other.to_string(),
                            self.enum_type_params.get(other).cloned(),
                            en,
                        )
                    } else if let Some(error) = self.ambiguous_import_error(other, *span) {
                        return Err(error);
                    } else {
                        (
                            other.to_string(),
                            None,
                            Type::Struct {
                                name: other.to_string(),
                                fields: Vec::new(),
                            },
                        )
                    };

                    if let Some(tparams) = tparams {
                        if !args.is_empty() {
                            if args.len() != tparams.len() {
                                return Err(TypeError::new(
                                    *span,
                                    format!(
                                        "type `{target_key}` expects {} type argument(s), got {}",
                                        tparams.len(),
                                        args.len()
                                    ),
                                ));
                            }
                            let mut lowered_args = Vec::new();
                            for arg in args {
                                lowered_args.push(self.lower_type(arg)?);
                            }
                            let mapping: HashMap<String, Type> =
                                tparams.into_iter().zip(lowered_args).collect();
                            let bounds = self.bounds_for_named(&target_key);
                            self.check_applied_bounds(&mapping, &bounds, *span)?;
                            return Ok(substitute_type(&base_ty, &mapping));
                        } else if !tparams.is_empty() {
                            let mapping: HashMap<String, Type> =
                                tparams.into_iter().map(|p| (p, Type::Dyn)).collect();
                            return Ok(substitute_type(&base_ty, &mapping));
                        }
                    }
                    base_ty
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
        for tp in &func.type_params {
            self.active_type_params.insert(tp.name.name.clone());
            self.active_param_bounds.insert(
                tp.name.name.clone(),
                tp.bounds.iter().map(|b| b.name.clone()).collect(),
            );
        }
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
        for tp in &func.type_params {
            self.active_type_params.remove(&tp.name.name);
            self.active_param_bounds.remove(&tp.name.name);
        }
        Ok(())
    }

    fn check_type_param_bounds(&self, params: &[TypeParam]) -> Result<(), TypeError> {
        for param in params {
            let mut seen = HashSet::new();
            for bound in &param.bounds {
                if !self.is_known_trait(&bound.name) {
                    return Err(TypeError::with_help(
                        bound.span,
                        format!("unknown trait `{}`", bound.name),
                        "declare the trait or use a builtin bound (`Eq`, `Ord`, `Hash`)",
                    ));
                }
                if !seen.insert(bound.name.as_str()) {
                    return Err(TypeError::new(
                        bound.span,
                        format!("duplicate bound `{}` on `{}`", bound.name, param.name.name),
                    ));
                }
            }
        }
        Ok(())
    }

    fn lower_trait_decl(&mut self, tr: &TraitDecl) -> Result<TraitDef, TypeError> {
        let mut methods = Vec::new();
        let mut seen_methods = HashSet::new();
        for m in &tr.methods {
            if !seen_methods.insert(m.name.name.as_str()) {
                return Err(TypeError::new(
                    m.name.span,
                    format!("duplicate method `{}` in trait `{}`", m.name.name, tr.name.name),
                ));
            }
            self.check_type_param_bounds(&m.type_params)?;
            let tparams: Vec<String> = m.type_params.iter().map(|p| p.name.name.clone()).collect();
            let param_bounds = type_param_bound_list(&m.type_params);
            for tp in &tparams {
                self.active_type_params.insert(tp.clone());
                if let Some((_, b)) = param_bounds.iter().find(|(n, _)| n == tp) {
                    self.active_param_bounds.insert(tp.clone(), b.clone());
                }
            }
            self.active_type_params.insert("Self".to_string());

            let mut params = Vec::new();
            if m.params.is_empty() {
                return Err(TypeError::new(
                    m.span,
                    format!("trait method `{}` must have `self` as first parameter", m.name.name),
                ));
            }
            for (i, p) in m.params.iter().enumerate() {
                if i == 0 {
                    if p.name.name != "self" {
                        return Err(TypeError::new(
                            p.span,
                            format!("first parameter of trait method `{}` must be `self`", m.name.name),
                        ));
                    }
                    if let Some(ty_expr) = &p.ty {
                        params.push(self.lower_type(ty_expr)?);
                    } else {
                        params.push(Type::Param("Self".to_string()));
                    }
                } else {
                    if p.name.name == "self" {
                        return Err(TypeError::new(
                            p.span,
                            format!("cannot have multiple `self` parameters in method `{}`", m.name.name),
                        ));
                    }
                    if let Some(ty_expr) = &p.ty {
                        params.push(self.lower_type(ty_expr)?);
                    } else {
                        return Err(TypeError::new(
                            p.span,
                            format!("parameter `{}` in trait method `{}` must have a type annotation", p.name.name, m.name.name),
                        ));
                    }
                }
            }
            let ret = if let Some(r) = &m.return_type {
                self.lower_type(r)?
            } else {
                Type::Nil
            };

            self.active_type_params.remove("Self");
            for tp in &tparams {
                self.active_type_params.remove(tp);
                self.active_param_bounds.remove(tp);
            }

            methods.push(TraitMethodDef {
                name: m.name.name.clone(),
                type_params: tparams,
                param_bounds,
                params,
                ret,
                effects: m.effects.clone(),
                span: m.span,
            });
        }
        Ok(TraitDef {
            name: tr.name.name.clone(),
            methods,
            span: tr.span,
        })
    }

    fn is_known_trait(&self, name: &str) -> bool {
        self.traits.contains_key(name)
    }

    fn register_impl(&mut self, imp: &ImplDecl) -> Result<(), TypeError> {
        self.check_type_param_bounds(&imp.type_params)?;
        if !self.is_known_trait(&imp.trait_name.name) {
            return Err(TypeError::with_help(
                imp.trait_name.span,
                format!("unknown trait `{}`", imp.trait_name.name),
                "declare the trait before implementing it",
            ));
        }
        for tp in &imp.type_params {
            self.active_type_params.insert(tp.name.name.clone());
            self.active_param_bounds.insert(
                tp.name.name.clone(),
                tp.bounds.iter().map(|b| b.name.clone()).collect(),
            );
        }
        let target_type = self.lower_type(&imp.ty)?;
        for tp in &imp.type_params {
            self.active_type_params.remove(&tp.name.name);
            self.active_param_bounds.remove(&tp.name.name);
        }
        let (type_ctor, _) = impl_target(&imp.ty)?;
        let key = (imp.trait_name.name.clone(), type_ctor.clone());
        if self
            .impls
            .iter()
            .any(|existing| existing.trait_name == key.0 && existing.type_ctor == key.1)
        {
            return Err(TypeError::new(
                imp.span,
                format!("duplicate implementation of `{}` for `{}`", key.0, key.1),
            ));
        }
        let trait_def = self.traits.get(&imp.trait_name.name).cloned().unwrap();
        if trait_def.methods.is_empty() {
            if !imp.methods.is_empty() {
                return Err(TypeError::new(
                    imp.methods[0].span,
                    format!("trait `{}` has no methods", trait_def.name),
                ));
            }
        } else {
            for required in &trait_def.methods {
                if !imp.methods.iter().any(|m| m.name.name == required.name) {
                    return Err(TypeError::with_help(
                        imp.span,
                        format!("missing method `{}` for trait `{}`", required.name, trait_def.name),
                        format!("implement `fn {}(...)` in the impl block", required.name),
                    ));
                }
            }
            let mut seen_impl_methods = HashSet::new();
            for m_impl in &imp.methods {
                if !seen_impl_methods.insert(m_impl.name.name.as_str()) {
                    return Err(TypeError::new(
                        m_impl.name.span,
                        format!("duplicate method `{}` in impl", m_impl.name.name),
                    ));
                }
                let Some(m_def) = trait_def.methods.iter().find(|m| m.name == m_impl.name.name) else {
                    return Err(TypeError::new(
                        m_impl.name.span,
                        format!("method `{}` is not a member of trait `{}`", m_impl.name.name, trait_def.name),
                    ));
                };
                if m_impl.params.len() != m_def.params.len() {
                    return Err(TypeError::new(
                        m_impl.span,
                        format!(
                            "method `{}` has {} parameter(s), expected {}",
                            m_impl.name.name,
                            m_impl.params.len(),
                            m_def.params.len()
                        ),
                    ));
                }
                if m_impl.params.is_empty() || m_impl.params[0].name.name != "self" {
                    let sp = m_impl.params.first().map_or(m_impl.span, |p| p.span);
                    return Err(TypeError::new(
                        sp,
                        format!("first parameter of method `{}` must be `self`", m_impl.name.name),
                    ));
                }
            }
        }

        let mut methods_map = HashMap::new();
        for m in &imp.methods {
            methods_map.insert(m.name.name.clone(), m.clone());
        }
        self.impls.push(TraitImpl {
            trait_name: imp.trait_name.name.clone(),
            type_ctor,
            target_type,
            param_bounds: type_param_bound_list(&imp.type_params),
            methods: methods_map,
        });
        Ok(())
    }

    fn check_impl_methods(&mut self, imp: &ImplDecl) -> Result<(), TypeError> {
        for tp in &imp.type_params {
            self.active_type_params.insert(tp.name.name.clone());
            self.active_param_bounds.insert(
                tp.name.name.clone(),
                tp.bounds.iter().map(|b| b.name.clone()).collect(),
            );
        }
        let target_type = self.lower_type(&imp.ty)?;
        let trait_def = self.traits.get(&imp.trait_name.name).cloned();
        if let Some(trait_def) = trait_def {
            for func in &imp.methods {
                let Some(m_def) = trait_def.methods.iter().find(|m| m.name == func.name.name) else {
                    continue;
                };
                let mut mapping = HashMap::new();
                mapping.insert("Self".to_string(), target_type.clone());
                let expected_params: Vec<Type> = m_def
                    .params
                    .iter()
                    .map(|p| substitute_type(p, &mapping))
                    .collect();
                let expected_ret = substitute_type(&m_def.ret, &mapping);

                for tp in &func.type_params {
                    self.active_type_params.insert(tp.name.name.clone());
                    self.active_param_bounds.insert(
                        tp.name.name.clone(),
                        tp.bounds.iter().map(|b| b.name.clone()).collect(),
                    );
                }
                self.push_scope();
                for (i, param) in func.params.iter().enumerate() {
                    let ty = if let Some(pt) = expected_params.get(i) {
                        pt.clone()
                    } else if let Some(te) = &param.ty {
                        self.lower_type(te)?
                    } else {
                        Type::Dyn
                    };
                    self.define(param.name.name.clone(), ty);
                }
                self.current_returns.push(expected_ret.clone());
                let body_ty = self.check_block(&func.body)?;
                if contains_task(&self.resolve(&body_ty).without_ownership()) {
                    return Err(TypeError::with_help(
                        func.body.span,
                        "task handle cannot escape its spawning function",
                        "await the task inside this function and return its result instead",
                    ));
                }
                if !block_definitely_returns(&func.body) {
                    self.unify(&body_ty, &expected_ret, func.body.span)?;
                }
                self.current_returns.pop();
                self.pop_scope();
                for tp in &func.type_params {
                    self.active_type_params.remove(&tp.name.name);
                    self.active_param_bounds.remove(&tp.name.name);
                }
            }
        }
        for tp in &imp.type_params {
            self.active_type_params.remove(&tp.name.name);
            self.active_param_bounds.remove(&tp.name.name);
        }
        Ok(())
    }

    fn lookup_trait_method_type(
        &mut self,
        target_ty: &Type,
        method_name: &str,
        method_span: Span,
    ) -> Result<Type, TypeError> {
        let resolved = self.resolve(target_ty).without_ownership();
        match &resolved {
            Type::Dyn | Type::Var(_) => Ok(Type::Dyn),
            Type::Param(p_name) => {
                let bounds = self.active_param_bounds.get(p_name).cloned().unwrap_or_default();
                let mut found_method = None;
                for tr_name in &bounds {
                    if let Some(tr_def) = self.traits.get(tr_name) {
                        if let Some(m_def) = tr_def.methods.iter().find(|m| m.name == method_name) {
                            found_method = Some(m_def.clone());
                            break;
                        }
                    }
                }
                if let Some(m_def) = found_method {
                    let mut mapping = HashMap::new();
                    mapping.insert("Self".to_string(), target_ty.clone());
                    let params: Vec<Type> = m_def
                        .params
                        .iter()
                        .skip(1)
                        .map(|p| substitute_type(p, &mapping))
                        .collect();
                    let ret = substitute_type(&m_def.ret, &mapping);
                    let effects = EffectSet::from_names(m_def.effects.names(), m_def.effects.specified);
                    Ok(Type::function(params, ret, effects))
                } else {
                    let defining_traits: Vec<String> = self
                        .traits
                        .values()
                        .filter(|tr| tr.methods.iter().any(|m| m.name == method_name))
                        .map(|tr| tr.name.clone())
                        .collect();
                    if !defining_traits.is_empty() {
                        Err(TypeError::with_help(
                            method_span,
                            format!("no method `{method_name}` on type parameter `{p_name}`"),
                            format!("add a trait bound `{p_name}: {}`", defining_traits.join(" + ")),
                        ))
                    } else {
                        Err(TypeError::new(
                            method_span,
                            format!("no method `{method_name}` on type `{p_name}`"),
                        ))
                    }
                }
            }
            concrete => {
                let mut found_method = None;
                for tr_def in self.traits.clone().values() {
                    if let Some(m_def) = tr_def.methods.iter().find(|m| m.name == method_name) {
                        if self.type_implements(concrete, &tr_def.name) {
                            found_method = Some(m_def.clone());
                            break;
                        }
                    }
                }
                if let Some(m_def) = found_method {
                    let mut mapping = HashMap::new();
                    mapping.insert("Self".to_string(), target_ty.clone());
                    let params: Vec<Type> = m_def
                        .params
                        .iter()
                        .skip(1)
                        .map(|p| substitute_type(p, &mapping))
                        .collect();
                    let ret = substitute_type(&m_def.ret, &mapping);
                    let effects = EffectSet::from_names(m_def.effects.names(), m_def.effects.specified);
                    Ok(Type::function(params, ret, effects))
                } else {
                    let defining_traits: Vec<String> = self
                        .traits
                        .values()
                        .filter(|tr| tr.methods.iter().any(|m| m.name == method_name))
                        .map(|tr| tr.name.clone())
                        .collect();
                    if !defining_traits.is_empty() {
                        let tr_name = &defining_traits[0];
                        Err(TypeError::with_help(
                            method_span,
                            format!("type `{concrete}` does not implement `{tr_name}` (required for method `{method_name}`)"),
                            format!("write `impl {tr_name} for {concrete} {{ ... }}`"),
                        ))
                    } else {
                        Err(TypeError::new(
                            method_span,
                            format!("no method or field `{method_name}` on type `{concrete}`"),
                        ))
                    }
                }
            }
        }
    }

    fn param_has_bound(&self, param: &str, trait_name: &str) -> bool {
        let Some(bounds) = self.active_param_bounds.get(param) else {
            return false;
        };
        if bounds.iter().any(|b| b == trait_name) {
            return true;
        }
        trait_name == "Eq" && bounds.iter().any(|b| b == "Ord")
    }

    fn instantiate_generic_mapped(&mut self, ty: &Type) -> (Type, HashMap<String, Type>) {
        let mut params = HashSet::new();
        collect_type_params(ty, &mut params);
        if params.is_empty() {
            return (ty.clone(), HashMap::new());
        }
        let mut mapping = HashMap::new();
        for p in params {
            mapping.insert(p, self.fresh());
        }
        (substitute_type(ty, &mapping), mapping)
    }

    fn check_applied_bounds(
        &mut self,
        mapping: &HashMap<String, Type>,
        bounds: &[(String, Vec<String>)],
        span: Span,
    ) -> Result<(), TypeError> {
        for (param, traits) in bounds {
            let Some(ty) = mapping.get(param) else {
                continue;
            };
            let ty = self.resolve(ty);
            for tr in traits {
                self.ensure_bound(&ty, tr, span)?;
            }
        }
        Ok(())
    }

    fn bounds_for_named(&self, key: &str) -> Vec<(String, Vec<String>)> {
        self.struct_param_bounds
            .get(key)
            .cloned()
            .or_else(|| self.enum_param_bounds.get(key).cloned())
            .or_else(|| self.alias_param_bounds.get(key).cloned())
            .unwrap_or_default()
    }

    fn callee_bound_key(callee: &Expr) -> Option<String> {
        match callee {
            Expr::Ident(id) => Some(id.name.clone()),
            Expr::Field { target, field, .. } => {
                let Expr::Ident(module) = target.as_ref() else {
                    return Some(field.name.clone());
                };
                Some(format!("{}.{}", module.name, field.name))
            }
            _ => None,
        }
    }

    fn ensure_bound(&mut self, ty: &Type, trait_name: &str, span: Span) -> Result<(), TypeError> {
        let ty = self.resolve(ty).without_ownership();
        if self.type_implements(&ty, trait_name) {
            return Ok(());
        }
        let help = match &ty {
            Type::Param(name) => {
                format!("add a `{trait_name}` bound: `{name}: {trait_name}`")
            }
            other => format!("write `impl {trait_name} for {other} {{}}`"),
        };
        Err(TypeError::with_help(
            span,
            format!("type `{ty}` does not implement `{trait_name}`"),
            help,
        ))
    }

    fn type_implements(&mut self, ty: &Type, trait_name: &str) -> bool {
        let ty = self.resolve(ty).without_ownership();
        match ty {
            Type::Dyn | Type::Var(_) => true,
            Type::Param(name) => self.param_has_bound(&name, trait_name),
            Type::Int | Type::Float | Type::String => {
                matches!(trait_name, "Eq" | "Ord" | "Hash")
                    || self.has_named_impl(trait_name, primitive_ctor(&ty))
            }
            Type::Bool => {
                matches!(trait_name, "Eq" | "Hash")
                    || self.has_named_impl(trait_name, "Bool")
            }
            Type::Nil => trait_name == "Eq" || self.has_named_impl(trait_name, "Nil"),
            Type::List(elem) => {
                if matches!(trait_name, "Eq" | "Ord" | "Hash") {
                    self.type_implements(&elem, trait_name)
                } else {
                    self.has_named_impl(trait_name, "List")
                }
            }
            Type::Struct { name, fields } => {
                if self.has_named_impl(trait_name, &name)
                    && self.generic_impl_args_ok(trait_name, &name, &Type::Struct { name: name.clone(), fields })
                {
                    return true;
                }
                false
            }
            Type::Enum { name, variants } => {
                if self.has_named_impl(trait_name, &name)
                    && self.generic_impl_args_ok(
                        trait_name,
                        &name,
                        &Type::Enum {
                            name: name.clone(),
                            variants,
                        },
                    )
                {
                    return true;
                }
                false
            }
            _ => self.has_named_impl(trait_name, &ty.name()),
        }
    }

    fn has_named_impl(&self, trait_name: &str, type_ctor: &str) -> bool {
        let ctor = type_ctor.rsplit('.').next().unwrap_or(type_ctor);
        self.impls
            .iter()
            .any(|imp| imp.trait_name == trait_name && (imp.type_ctor == type_ctor || imp.type_ctor == ctor))
    }

    fn generic_impl_args_ok(&mut self, trait_name: &str, type_ctor: &str, inst: &Type) -> bool {
        let ctor = type_ctor.rsplit('.').next().unwrap_or(type_ctor);
        let Some(imp) = self.impls.iter().find(|imp| {
            imp.trait_name == trait_name && (imp.type_ctor == type_ctor || imp.type_ctor == ctor)
        }) else {
            return false;
        };
        if imp.param_bounds.is_empty() {
            return true;
        }
        let original = self
            .structs
            .get(type_ctor)
            .cloned()
            .or_else(|| self.structs.get(ctor).cloned())
            .or_else(|| self.enums.get(type_ctor).cloned())
            .or_else(|| self.enums.get(ctor).cloned());
        let Some(original) = original else {
            return true;
        };
        let mapping = recover_param_mapping(&original, inst);
        let bounds = imp.param_bounds.clone();
        for (param, traits) in bounds {
            let Some(arg) = mapping.get(&param) else {
                continue;
            };
            for tr in traits {
                if !self.type_implements(arg, &tr) {
                    return false;
                }
            }
        }
        true
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
                let (fty, mapping) = self.instantiate_generic_mapped(&fty);
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
                        if let Some(name) = Self::callee_bound_key(callee) {
                            if let Some(bounds) = self.fn_param_bounds.get(&name).cloned() {
                                self.check_applied_bounds(&mapping, &bounds, *span)?;
                            }
                        }
                        match self.resolve(&ret).without_ownership() {
                            Type::Enum { name, .. } => {
                                if let Some(bounds) = self.enum_param_bounds.get(&name).cloned() {
                                    self.check_applied_bounds(&mapping, &bounds, *span)?;
                                }
                            }
                            Type::Struct { name, .. } => {
                                if let Some(bounds) = self.struct_param_bounds.get(&name).cloned() {
                                    self.check_applied_bounds(&mapping, &bounds, *span)?;
                                }
                            }
                            _ => {}
                        }
                        Ok(self.resolve(&ret))
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
                let Expr::Call {
                    callee: _, args, ..
                } = call.as_ref()
                else {
                    return Err(TypeError::with_help(
                        *span,
                        "`spawn` expects a function call",
                        "pass the work and its arguments as `spawn work(args...)`",
                    ));
                };
                for arg in args {
                    if matches!(
                        arg,
                        Expr::Unary {
                            op: UnOp::Ref | UnOp::RefMut,
                            ..
                        }
                    ) {
                        return Err(TypeError::with_help(
                            arg.span(),
                            "cannot capture reference across task boundary into `spawn`",
                            "spawned tasks must capture owned values; references cannot outlive their stack frame",
                        ));
                    }
                    let arg_ty = self.check_expr(arg)?;
                    if self.resolve(&arg_ty).contains_ref() {
                        let err_msg = match arg {
                            Expr::Ident(id) => {
                                format!(
                                    "cannot capture reference `{}` across task boundary into `spawn`",
                                    id.name
                                )
                            }
                            _ => "cannot capture reference across task boundary into `spawn`"
                                .to_string(),
                        };
                        return Err(TypeError::with_help(
                            arg.span(),
                            err_msg,
                            "spawned tasks must capture owned values; references cannot outlive their stack frame",
                        ));
                    }
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
                let ret_resolved = self.resolve(&return_ty).without_ownership();
                let Type::Enum { name: _ret_name, variants: ret_variants } = &ret_resolved else {
                    return Err(TypeError::with_help(
                        *span,
                        format!("cannot propagate `{name}.Err` from this function"),
                        format!("change the function return type to `{name}` or handle the error with `match`"),
                    ));
                };
                let (Some((ret_ok, _)), Some((ret_err, ret_err_fields))) =
                    (ret_variants.first(), ret_variants.get(1))
                else {
                    return Err(TypeError::with_help(
                        *span,
                        format!("cannot propagate `{name}.Err` from this function"),
                        format!("change the function return type to `{name}` or handle the error with `match`"),
                    ));
                };
                if ret_variants.len() != 2
                    || ret_ok != "Ok"
                    || ret_err != "Err"
                    || ret_err_fields.len() != 1
                {
                    return Err(TypeError::with_help(
                        *span,
                        format!("cannot propagate `{name}.Err` from this function"),
                        format!("change the function return type to `{name}` or handle the error with `match`"),
                    ));
                }
                self.unify(&err_fields[0], &ret_err_fields[0], *span).map_err(|_| {
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
                        } else if fields.is_empty()
                            && self.structs.get(&name).is_some_and(|s| {
                                if let Type::Struct { fields, .. } = s {
                                    fields.iter().any(|(n, _)| n == &field.name)
                                } else {
                                    false
                                }
                            })
                        {
                            let ty = self
                                .structs
                                .get(&name)
                                .and_then(|s| {
                                    if let Type::Struct { fields, .. } = s {
                                        fields
                                            .iter()
                                            .find(|(n, _)| n == &field.name)
                                            .map(|(_, ty)| ty.clone())
                                    } else {
                                        None
                                    }
                                })
                                .unwrap();
                            Ok(ty)
                        } else if let Ok(method_ty) =
                            self.lookup_trait_method_type(&t, &field.name, field.span)
                        {
                            Ok(method_ty)
                        } else {
                            self.lookup_trait_method_type(&t, &field.name, field.span)
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
                        let instantiated = self.instantiate_generic(&Type::Enum { name, variants });
                        let Type::Enum { name, variants } = instantiated else { unreachable!() };
                        let Some((_, fields)) = variants.iter().find(|(n, _)| n == &field.name)
                        else {
                            if let Ok(method_ty) =
                                self.lookup_trait_method_type(&t, &field.name, field.span)
                            {
                                return Ok(method_ty);
                            }
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
                    _other => self.lookup_trait_method_type(&t, &field.name, field.span),
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
                let (st, mapping) = self.instantiate_generic_mapped(&st);
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
                if let Some(bounds) = self.struct_param_bounds.get(&name.name).cloned() {
                    self.check_applied_bounds(&mapping, &bounds, *span)?;
                }
                Ok(self.resolve(&Type::Struct {
                    name: n,
                    fields: decl,
                }))
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
                    Type::Dyn | Type::Var(_) => {
                        Ok(Type::List(Box::new(Type::List(Box::new(Type::Dyn)))))
                    }
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
            "is_completed" => {
                self.check_arity_range(name, args.len(), 1, Some(1), span)?;
                let task_ty = self.check_expr(&args[0])?;
                match self.resolve(&task_ty).without_ownership() {
                    Type::Task(_) | Type::Dyn | Type::Var(_) => Ok(Type::Bool),
                    other => Err(TypeError::with_help(
                        args[0].span(),
                        format!("is_completed() expected Task, found {other}"),
                        "pass a Task handle to is_completed()",
                    )),
                }
            }
            "task_status" => {
                self.check_arity_range(name, args.len(), 1, Some(1), span)?;
                let task_ty = self.check_expr(&args[0])?;
                match self.resolve(&task_ty).without_ownership() {
                    Type::Task(_) | Type::Dyn | Type::Var(_) => Ok(Type::String),
                    other => Err(TypeError::with_help(
                        args[0].span(),
                        format!("task_status() expected Task, found {other}"),
                        "pass a Task handle to task_status()",
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
                let resolved = self.resolve(&l).without_ownership();
                if matches!(resolved, Type::Param(_)) {
                    self.ensure_bound(&resolved, "Eq", span)?;
                }
                Ok(Type::Bool)
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let l_ty = self.resolve(&l).without_ownership();
                let r_ty = self.resolve(&r).without_ownership();
                match (&l_ty, &r_ty) {
                    (Type::Param(_), _) | (_, Type::Param(_)) => {
                        self.unify(&l, &r, span)?;
                        self.ensure_bound(&l_ty, "Ord", span)?;
                        Ok(Type::Bool)
                    }
                    (Type::String, Type::String)
                    | (Type::String, Type::Dyn)
                    | (Type::Dyn, Type::String)
                    | (Type::String, Type::Var(_))
                    | (Type::Var(_), Type::String) => {
                        self.unify(&l, &r, span)?;
                        Ok(Type::Bool)
                    }
                    _ => {
                        let _ = numeric_result(self, &l, &r, span)?;
                        Ok(Type::Bool)
                    }
                }
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
            Item::Fn(_) | Item::Import(_) | Item::Trait(_) | Item::Impl(_) => None,
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
            Item::Trait(tr) => {
                let mut types = Vec::new();
                for m in &tr.methods {
                    types.extend(m.params.iter().filter_map(|p| p.ty.as_ref()));
                    types.extend(m.return_type.as_ref());
                }
                (format!("trait `{}`", tr.name.name), types)
            }
            Item::Import(_) | Item::Impl(_) => continue,
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
        Type::Struct { fields, .. } => fields.iter().any(|(_, t)| occurs(id, t)),
        Type::Enum { variants, .. } => variants
            .iter()
            .any(|(_, ts)| ts.iter().any(|t| occurs(id, t))),
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
        | Type::Param(_)
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
