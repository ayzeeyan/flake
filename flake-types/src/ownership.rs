//! Gradual ownership: enforced only in `strict` / `owned` contexts.
//!
//! Ordinary Flake code does not need annotations and is not move-checked.
//! In a `strict` or `owned` function, `owned` values are moved on use,
//! `ref` values may be used many times, and using a binding after a move
//! is an error. Copy types (`Int`, `Float`, `Bool`, `Nil`) never move.

use std::collections::HashMap;

use flake_ast::{Block, Expr, FnDecl, InterpPart, Item, Program, Span, Stmt, TypeExpr};

use crate::error::TypeError;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Owned,
    Ref,
    Mut,
    Copy,
}

#[derive(Clone)]
struct Binding {
    kind: Kind,
    moved_at: Option<Span>,
}

struct OwnCx {
    scopes: Vec<HashMap<String, Binding>>,
}

impl OwnCx {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, name: String, kind: Kind) {
        self.scopes.last_mut().unwrap().insert(
            name,
            Binding {
                kind,
                moved_at: None,
            },
        );
    }

    fn lookup_mut(&mut self, name: &str) -> Option<&mut Binding> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                return scope.get_mut(name);
            }
        }
        None
    }
}

/// Check ownership rules on `strict` / `owned` functions. Other items are ignored.
pub fn check_ownership(program: &Program) -> Result<(), TypeError> {
    for item in &program.items {
        if let Item::Fn(func) = item {
            if func.strict || func.owned {
                check_fn(func)?;
            }
        }
    }
    Ok(())
}

fn check_fn(func: &FnDecl) -> Result<(), TypeError> {
    let mut cx = OwnCx::new();
    for param in &func.params {
        let kind = kind_from_type(param.ty.as_ref(), func.owned || func.strict);
        cx.define(param.name.name.clone(), kind);
    }
    check_block(&mut cx, &func.body)?;
    Ok(())
}

fn kind_from_type(ty: Option<&TypeExpr>, default_owned: bool) -> Kind {
    match ty {
        Some(t) if is_copy_type(t) => Kind::Copy,
        Some(TypeExpr::Owned { inner, .. }) if is_copy_type(inner) => Kind::Copy,
        Some(TypeExpr::Owned { .. }) => Kind::Owned,
        Some(TypeExpr::Ref { mutable: true, .. }) | Some(TypeExpr::Mut { .. }) => Kind::Mut,
        Some(TypeExpr::Ref { mutable: false, .. }) => Kind::Ref,
        Some(_) if default_owned => Kind::Owned,
        Some(_) => Kind::Copy,
        None if default_owned => Kind::Owned,
        None => Kind::Copy,
    }
}

fn is_copy_type(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Named { name, .. } => {
            matches!(name.name.as_str(), "Int" | "Float" | "Bool" | "Nil" | "Unit")
        }
        TypeExpr::Dyn { .. } => false,
        TypeExpr::Owned { inner, .. }
        | TypeExpr::Mut { inner, .. }
        | TypeExpr::Ref { inner, .. }
        | TypeExpr::Optional { inner, .. } => is_copy_type(inner),
        _ => false,
    }
}

fn check_block(cx: &mut OwnCx, block: &Block) -> Result<(), TypeError> {
    cx.push();
    for stmt in &block.stmts {
        check_stmt(cx, stmt)?;
    }
    if let Some(tail) = &block.tail {
        check_expr(cx, tail, true)?;
    }
    cx.pop();
    Ok(())
}

fn check_stmt(cx: &mut OwnCx, stmt: &Stmt) -> Result<(), TypeError> {
    match stmt {
        Stmt::Let(s) | Stmt::Var(s) => {
            check_expr(cx, &s.value, true)?;
            let kind = kind_from_type(s.ty.as_ref(), true);
            cx.define(s.name.name.clone(), kind);
            Ok(())
        }
        Stmt::Return { value: Some(v), .. } => check_expr(cx, v, true),
        Stmt::Return { .. } | Stmt::Break { .. } | Stmt::Continue { .. } => Ok(()),
        Stmt::While { cond, body, .. } => {
            check_expr(cx, cond, false)?;
            check_block(cx, body)
        }
        Stmt::For { iter, body, name, .. } => {
            check_expr(cx, iter, false)?;
            cx.push();
            cx.define(name.name.clone(), Kind::Copy);
            check_block(cx, body)?;
            cx.pop();
            Ok(())
        }
        Stmt::Loop { body, .. } => check_block(cx, body),
        Stmt::Expr(e) => check_expr(cx, e, false),
    }
}

/// `move_ok` is true when this expression is in a move position (return, argument, binding).
fn check_expr(cx: &mut OwnCx, expr: &Expr, move_ok: bool) -> Result<(), TypeError> {
    match expr {
        Expr::Ident(id) => use_binding(cx, &id.name, id.span, move_ok),
        Expr::Literal { .. } => Ok(()),
        Expr::Interpolated { parts, .. } => {
            for part in parts {
                if let InterpPart::Expr(e) = part {
                    check_expr(cx, e, false)?;
                }
            }
            Ok(())
        }
        Expr::List { elements, .. } => {
            for e in elements {
                check_expr(cx, e, true)?;
            }
            Ok(())
        }
        Expr::Map { entries, .. } => {
            for (k, v) in entries {
                check_expr(cx, k, true)?;
                check_expr(cx, v, true)?;
            }
            Ok(())
        }
        Expr::Unary { op, expr, .. } => {
            // `&x` / `&mut x` borrow; they do not move.
            let moving = !matches!(op, flake_ast::UnOp::Ref | flake_ast::UnOp::RefMut);
            check_expr(cx, expr, moving && move_ok)
        }
        Expr::Binary { left, right, .. } => {
            check_expr(cx, left, false)?;
            check_expr(cx, right, false)
        }
        Expr::Assign { target, value, .. } => {
            if let Expr::Ident(id) = target.as_ref() {
                if let Some(binding) = cx.lookup_mut(&id.name) {
                    if binding.kind == Kind::Ref {
                        return Err(TypeError::new(
                            id.span,
                            format!("cannot assign to `ref` binding `{}`", id.name),
                        ));
                    }
                    binding.moved_at = None;
                }
            } else {
                check_expr(cx, target, false)?;
            }
            check_expr(cx, value, true)
        }
        Expr::Call { callee, args, .. } => {
            check_expr(cx, callee, false)?;
            for arg in args {
                check_expr(cx, arg, true)?;
            }
            Ok(())
        }
        Expr::Index { target, index, .. } => {
            check_expr(cx, target, false)?;
            check_expr(cx, index, false)
        }
        Expr::Field { target, .. } => check_expr(cx, target, false),
        Expr::Range { start, end, .. } => {
            check_expr(cx, start, false)?;
            check_expr(cx, end, false)
        }
        Expr::If {
            cond,
            then_block,
            else_block,
            ..
        } => {
            check_expr(cx, cond, false)?;
            check_block(cx, then_block)?;
            if let Some(els) = else_block {
                check_expr(cx, els, move_ok)?;
            }
            Ok(())
        }
        Expr::Block(b) => check_block(cx, b),
        Expr::StructInit { fields, .. } => {
            for (_, v) in fields {
                check_expr(cx, v, true)?;
            }
            Ok(())
        }
    }
}

fn use_binding(cx: &mut OwnCx, name: &str, span: Span, move_ok: bool) -> Result<(), TypeError> {
    let Some(binding) = cx.lookup_mut(name) else {
        return Ok(());
    };
    if let Some(moved_at) = binding.moved_at {
        return Err(TypeError::new(
            span,
            format!("use of moved value `{name}` (moved at byte {})", moved_at.start),
        ));
    }
    if move_ok && binding.kind == Kind::Owned {
        binding.moved_at = Some(span);
    }
    Ok(())
}
