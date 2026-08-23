//! Gradual ownership: enforced only in `strict` / `owned` contexts.
//!
//! Ordinary Flake is not move-checked. In a `strict` or `owned` function:
//! - `owned` values are moved on use in move positions
//! - `ref` values may be used many times but cannot be assigned or moved
//! - `&x` / `&mut x` borrow without moving
//! - a value cannot be moved while borrowed, and `&mut` is exclusive

use std::collections::{HashMap, HashSet};

use flake_ast::{Block, Expr, FnDecl, InterpPart, Item, Program, Span, Stmt, TypeExpr, UnOp};

use crate::error::TypeError;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Owned,
    Ref,
    Mut,
    Copy,
}

#[derive(Clone, Copy)]
enum State {
    Available,
    Moved(Span),
    Borrowed {
        mutable: bool,
        at: Span,
        depth: usize,
        /// Statement-local borrow (e.g. a call argument `&x`). Ends after the statement.
        temp: bool,
    },
}

#[derive(Clone)]
struct Binding {
    kind: Kind,
    state: State,
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
        let depth = self.scopes.len().saturating_sub(1);
        self.scopes.pop();
        // Scope-based borrows: a borrow ends when its block ends.
        for scope in &mut self.scopes {
            for binding in scope.values_mut() {
                if let State::Borrowed { depth: d, .. } = binding.state {
                    if d >= depth {
                        binding.state = State::Available;
                    }
                }
            }
        }
    }

    fn depth(&self) -> usize {
        self.scopes.len().saturating_sub(1)
    }

    fn snapshot(&self) -> Vec<(String, State)> {
        let mut out = Vec::new();
        for scope in &self.scopes {
            for (name, binding) in scope {
                out.push((name.clone(), binding.state));
            }
        }
        out
    }

    fn restore_states(&mut self, snap: &[(String, State)]) {
        for (name, state) in snap {
            if let Some(binding) = self.lookup_mut(name) {
                binding.state = *state;
            }
        }
    }

    fn merge_after_branches(&mut self, a: &[(String, State)], b: &[(String, State)]) {
        let names: HashSet<_> = a
            .iter()
            .map(|(n, _)| n.clone())
            .chain(b.iter().map(|(n, _)| n.clone()))
            .collect();
        for name in names {
            let sa = a.iter().find(|(n, _)| n == &name).map(|(_, s)| *s);
            let sb = b.iter().find(|(n, _)| n == &name).map(|(_, s)| *s);
            if let Some(binding) = self.lookup_mut(&name) {
                binding.state = merge_state(sa, sb);
            }
        }
    }

    fn define(&mut self, name: String, kind: Kind) {
        self.scopes.last_mut().unwrap().insert(
            name,
            Binding {
                kind,
                state: State::Available,
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

    fn pin_temps(&mut self) {
        for scope in &mut self.scopes {
            for binding in scope.values_mut() {
                if let State::Borrowed { temp, .. } = &mut binding.state {
                    *temp = false;
                }
            }
        }
    }

    fn clear_temps(&mut self) {
        for scope in &mut self.scopes {
            for binding in scope.values_mut() {
                if let State::Borrowed { temp: true, .. } = binding.state {
                    binding.state = State::Available;
                }
            }
        }
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
            matches!(
                name.name.as_str(),
                "Int" | "Float" | "Bool" | "Nil" | "Unit"
            )
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
            cx.pin_temps();
            Ok(())
        }
        Stmt::Return { value: Some(v), .. } => {
            check_expr(cx, v, true)?;
            cx.clear_temps();
            Ok(())
        }
        Stmt::Return { .. } | Stmt::Break { .. } | Stmt::Continue { .. } => Ok(()),
        Stmt::While { cond, body, .. } => {
            check_expr(cx, cond, false)?;
            check_loop_body(cx, body)
        }
        Stmt::For {
            iter, body, name, ..
        } => {
            check_expr(cx, iter, false)?;
            cx.push();
            cx.define(name.name.clone(), Kind::Copy);
            check_loop_body(cx, body)?;
            cx.pop();
            Ok(())
        }
        Stmt::Loop { body, .. } => check_loop_body(cx, body),
        Stmt::Expr(e) => {
            check_expr(cx, e, false)?;
            cx.clear_temps();
            Ok(())
        }
    }
}

fn check_expr(cx: &mut OwnCx, expr: &Expr, move_ok: bool) -> Result<(), TypeError> {
    match expr {
        Expr::Ident(id) => use_binding(cx, &id.name, id.span, move_ok),
        Expr::Literal { .. } => Ok(()),
        Expr::Interpolated { parts, .. } => {
            for part in parts {
                if let InterpPart::Expr(e) = part {
                    // Interpolation borrows; it does not consume.
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
        Expr::Unary { op, expr, span, .. } => match op {
            UnOp::Ref => borrow(cx, expr, false, *span),
            UnOp::RefMut => borrow(cx, expr, true, *span),
            UnOp::Neg | UnOp::Not => check_expr(cx, expr, false),
        },
        Expr::Binary { left, right, .. } => {
            check_expr(cx, left, false)?;
            check_expr(cx, right, false)
        }
        Expr::Assign { target, value, .. } => {
            if let Expr::Ident(id) = target.as_ref() {
                if let Some(binding) = cx.lookup_mut(&id.name) {
                    match binding.kind {
                        Kind::Ref => {
                            return Err(TypeError::new(
                                id.span,
                                format!("cannot assign to `ref` binding `{}`", id.name),
                            ));
                        }
                        Kind::Copy | Kind::Owned | Kind::Mut => {
                            if let State::Borrowed { .. } = binding.state {
                                return Err(TypeError::new(
                                    id.span,
                                    format!(
                                        "cannot assign to `{}` while it is borrowed\nhelp: the borrow must end before the value is assigned",
                                        id.name
                                    ),
                                ));
                            }
                            binding.state = State::Available;
                        }
                    }
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
        Expr::Spawn { call, .. } => check_expr(cx, call, true),
        Expr::Await { task, .. } => check_expr(cx, task, true),
        Expr::Try { expr, .. } => check_expr(cx, expr, true),
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
            let before = cx.snapshot();
            check_block(cx, then_block)?;
            let after_then = cx.snapshot();
            cx.restore_states(&before);
            if let Some(els) = else_block {
                check_expr(cx, els, move_ok)?;
                let after_else = cx.snapshot();
                cx.merge_after_branches(&after_then, &after_else);
            } else {
                cx.restore_states(&before);
            }
            Ok(())
        }
        Expr::Block(b) | Expr::Nursery { body: b, .. } => check_block(cx, b),
        Expr::StructInit { fields, .. } => {
            for (_, v) in fields {
                check_expr(cx, v, true)?;
            }
            Ok(())
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            check_expr(cx, scrutinee, false)?;
            if arms.is_empty() {
                return Ok(());
            }
            let before = cx.snapshot();
            let mut arm_snapshots = Vec::new();
            for arm in arms {
                cx.restore_states(&before);
                check_expr(cx, &arm.body, move_ok)?;
                arm_snapshots.push(cx.snapshot());
            }
            if let Some(first) = arm_snapshots.first() {
                let mut merged = first.clone();
                for other in &arm_snapshots[1..] {
                    cx.restore_states(&merged);
                    cx.merge_after_branches(&merged, other);
                    merged = cx.snapshot();
                }
                cx.restore_states(&merged);
            }
            Ok(())
        }
    }
}

fn root_variable_info(expr: &Expr) -> Option<(&str, Span)> {
    match expr {
        Expr::Ident(id) => Some((id.name.as_str(), id.span)),
        Expr::Field { target, .. } | Expr::Index { target, .. } => root_variable_info(target),
        _ => None,
    }
}

fn borrow(cx: &mut OwnCx, expr: &Expr, mutable: bool, span: Span) -> Result<(), TypeError> {
    if let Some((root_name, _root_span)) = root_variable_info(expr) {
        let depth = cx.depth();
        let Some(binding) = cx.lookup_mut(root_name) else {
            return Ok(());
        };
        match binding.state {
            State::Moved(_) => {
                return Err(TypeError::new(
                    span,
                    format!("cannot borrow `{root_name}` because it was already moved"),
                ));
            }
            State::Borrowed { mutable: true, .. } => {
                return Err(TypeError::new(
                    span,
                    format!(
                        "cannot borrow `{root_name}` because it is already mutably borrowed"
                    ),
                ));
            }
            State::Borrowed { mutable: false, .. } if mutable => {
                return Err(TypeError::new(
                    span,
                    format!(
                        "cannot mutably borrow `{root_name}` because it is already borrowed"
                    ),
                ));
            }
            State::Available | State::Borrowed { mutable: false, .. } => {
                if binding.kind == Kind::Ref && mutable {
                    return Err(TypeError::new(
                        span,
                        format!("cannot mutably borrow `ref` binding `{root_name}`"),
                    ));
                }
                binding.state = State::Borrowed {
                    mutable,
                    at: span,
                    depth,
                    temp: true,
                };
            }
        }
        Ok(())
    } else {
        check_expr(cx, expr, false)
    }
}

fn use_binding(cx: &mut OwnCx, name: &str, span: Span, move_ok: bool) -> Result<(), TypeError> {
    let Some(binding) = cx.lookup_mut(name) else {
        return Ok(());
    };
    match binding.state {
        State::Moved(_) => {
            return Err(TypeError::new(
                span,
                format!(
                    "use of moved value `{name}`\nhelp: in `strict` functions, `owned` values are moved on use; take `ref T` to reuse"
                ),
            ));
        }
        State::Borrowed { .. } if move_ok && binding.kind == Kind::Owned => {
            return Err(TypeError::new(
                span,
                format!(
                    "cannot move `{name}` while it is borrowed\nhelp: the borrow lasts until the end of the current block"
                ),
            ));
        }
        _ => {}
    }
    if move_ok && binding.kind == Kind::Owned {
        binding.state = State::Moved(span);
    }
    Ok(())
}

fn check_loop_body(cx: &mut OwnCx, body: &Block) -> Result<(), TypeError> {
    let before = cx.snapshot();
    check_block(cx, body)?;
    let after = cx.snapshot();
    for (name, state) in &after {
        if matches!(state, State::Moved(_)) {
            let was_moved = before
                .iter()
                .find(|(n, _)| n == name)
                .is_some_and(|(_, s)| matches!(s, State::Moved(_)));
            if !was_moved {
                return Err(TypeError::new(
                    body.span,
                    format!(
                        "cannot move `{name}` inside a loop\nhelp: `owned` values can be moved only once"
                    ),
                ));
            }
        }
    }
    cx.restore_states(&before);
    Ok(())
}

fn merge_state(a: Option<State>, b: Option<State>) -> State {
    match (a, b) {
        (Some(State::Moved(s)), Some(State::Moved(_))) => State::Moved(s),
        (
            Some(State::Borrowed {
                mutable: m1,
                at,
                depth,
                temp,
            }),
            Some(State::Borrowed { mutable: m2, .. }),
        ) => State::Borrowed {
            mutable: m1 || m2,
            at,
            depth,
            temp,
        },
        (
            Some(State::Borrowed {
                mutable,
                at,
                depth,
                temp,
            }),
            _,
        )
        | (
            _,
            Some(State::Borrowed {
                mutable,
                at,
                depth,
                temp,
            }),
        ) => State::Borrowed {
            mutable,
            at,
            depth,
            temp,
        },
        (Some(State::Moved(_)), _) | (_, Some(State::Moved(_))) => {
            // Only one branch moved: the value is not definitely moved.
            State::Available
        }
        (Some(s), _) => s,
        (_, Some(s)) => s,
        _ => State::Available,
    }
}
