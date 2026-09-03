//! Compile-time evaluation of the CTFE-lite fragment.

use std::collections::HashMap;

use crate::ast::{BinOp, Block, Expr, FnDecl, InterpPart, Item, Literal, Program, UnOp};
use crate::span::Span;

/// A value produced by const folding.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

/// Failure to fold a const expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstError {
    pub span: Span,
    pub message: String,
}

impl ConstError {
    fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

const CTFE_FUEL: u32 = 10_000;
const MAX_CALL_DEPTH: u32 = 256;

struct CtfeContext<'a> {
    fns: &'a HashMap<String, &'a FnDecl>,
    fuel: u32,
    depth: u32,
}

/// Evaluate every `const` item in declaration order.
pub fn collect_const_values(program: &Program) -> Result<HashMap<String, ConstValue>, ConstError> {
    let mut fns = HashMap::new();
    for item in &program.items {
        if let Item::Fn(func) = item {
            if func.is_const {
                fns.insert(func.name.name.clone(), func);
            }
        }
    }
    let mut env = HashMap::new();
    let mut ctx = CtfeContext {
        fns: &fns,
        fuel: CTFE_FUEL,
        depth: 0,
    };
    for item in &program.items {
        if let Item::Const(decl) = item {
            let value = eval_const_expr_inner(&decl.value, &env, &mut ctx)?;
            env.insert(decl.name.name.clone(), value);
        }
    }
    Ok(env)
}

/// Fold a const-eligible expression.
pub fn eval_const_expr(
    expr: &Expr,
    env: &HashMap<String, ConstValue>,
) -> Result<ConstValue, ConstError> {
    let fns = HashMap::new();
    let mut ctx = CtfeContext {
        fns: &fns,
        fuel: CTFE_FUEL,
        depth: 0,
    };
    eval_const_expr_inner(expr, env, &mut ctx)
}

/// Fold a const-eligible expression with available const functions.
pub fn eval_const_expr_with_fns(
    expr: &Expr,
    env: &HashMap<String, ConstValue>,
    fns: &HashMap<String, &FnDecl>,
) -> Result<ConstValue, ConstError> {
    let mut ctx = CtfeContext {
        fns,
        fuel: CTFE_FUEL,
        depth: 0,
    };
    eval_const_expr_inner(expr, env, &mut ctx)
}

fn eval_const_expr_inner(
    expr: &Expr,
    env: &HashMap<String, ConstValue>,
    ctx: &mut CtfeContext<'_>,
) -> Result<ConstValue, ConstError> {
    match expr {
        Expr::Literal { value, .. } => Ok(literal_value(value)),
        Expr::Ident(id) => env.get(&id.name).cloned().ok_or_else(|| {
            ConstError::new(
                id.span,
                format!("`{}` is not a constant in this const expression", id.name),
            )
        }),
        Expr::Unary { op, expr, span } => {
            let v = eval_const_expr_inner(expr, env, ctx)?;
            eval_unary(*op, v, *span)
        }
        Expr::Binary {
            op,
            left,
            right,
            span,
        } => {
            if matches!(*op, BinOp::And | BinOp::Or) {
                return eval_logic(*op, left, right, env, ctx, *span);
            }
            let l = eval_const_expr_inner(left, env, ctx)?;
            let r = eval_const_expr_inner(right, env, ctx)?;
            eval_binary(*op, l, r, *span)
        }
        Expr::If {
            cond,
            then_block,
            else_block,
            span,
        } => {
            let c = eval_const_expr_inner(cond, env, ctx)?;
            let ConstValue::Bool(flag) = c else {
                return Err(ConstError::new(*span, "const `if` condition must be Bool"));
            };
            if flag {
                eval_const_block(then_block, env, ctx)
            } else {
                let else_expr = else_block.as_deref().ok_or_else(|| {
                    ConstError::new(*span, "const `if` requires an `else` branch")
                })?;
                eval_const_expr_inner(else_expr, env, ctx)
            }
        }
        Expr::Block(block) => eval_const_block(block, env, ctx),
        Expr::Interpolated { parts, span } => {
            let mut out = String::new();
            for part in parts {
                match part {
                    InterpPart::Text(t) => out.push_str(t),
                    InterpPart::Expr(e) => {
                        out.push_str(&display_const(&eval_const_expr_inner(e, env, ctx)?))
                    }
                }
            }
            let _ = span;
            Ok(ConstValue::String(out))
        }
        Expr::Call {
            callee,
            args,
            span,
        } => eval_const_call(callee, args, env, ctx, *span),
        other => Err(ConstError::new(
            other.span(),
            "expression is not a constant",
        )),
    }
}

fn eval_const_call(
    callee: &Expr,
    args: &[Expr],
    env: &HashMap<String, ConstValue>,
    ctx: &mut CtfeContext<'_>,
    span: Span,
) -> Result<ConstValue, ConstError> {
    let Expr::Ident(id) = callee else {
        return Err(ConstError::new(
            span,
            "cannot call a function in a const expression",
        ));
    };
    let Some(func) = ctx.fns.get(&id.name) else {
        return Err(ConstError::new(
            span,
            format!("cannot call non-const function `{}` from a const expression", id.name),
        ));
    };
    if ctx.fuel == 0 || ctx.depth >= MAX_CALL_DEPTH {
        return Err(ConstError::new(
            span,
            "const evaluation exceeded recursion limit",
        ));
    }
    ctx.fuel -= 1;
    ctx.depth += 1;
    if args.len() != func.params.len() {
        return Err(ConstError::new(
            span,
            format!(
                "const function `{}` expected {} argument(s), got {}",
                id.name,
                func.params.len(),
                args.len()
            ),
        ));
    }
    let mut local = env.clone();
    for (param, arg) in func.params.iter().zip(args) {
        let value = eval_const_expr_inner(arg, env, ctx)?;
        local.insert(param.name.name.clone(), value);
    }
    let res = eval_const_block(&func.body, &local, ctx);
    ctx.depth -= 1;
    res
}

fn eval_const_block(
    block: &Block,
    env: &HashMap<String, ConstValue>,
    ctx: &mut CtfeContext<'_>,
) -> Result<ConstValue, ConstError> {
    if !block.stmts.is_empty() {
        return Err(ConstError::new(
            block.span,
            "const block cannot contain statements",
        ));
    }
    match &block.tail {
        Some(expr) => eval_const_expr_inner(expr, env, ctx),
        None => Ok(ConstValue::Nil),
    }
}

fn literal_value(value: &Literal) -> ConstValue {
    match value {
        Literal::Nil => ConstValue::Nil,
        Literal::Bool(b) => ConstValue::Bool(*b),
        Literal::Int(n) => ConstValue::Int(*n),
        Literal::Float(n) => ConstValue::Float(*n),
        Literal::String(s) => ConstValue::String(s.clone()),
    }
}

fn eval_unary(op: UnOp, value: ConstValue, span: Span) -> Result<ConstValue, ConstError> {
    match (op, value) {
        (UnOp::Neg, ConstValue::Int(n)) => n
            .checked_neg()
            .map(ConstValue::Int)
            .ok_or_else(|| ConstError::new(span, "integer overflow in const expression")),
        (UnOp::Neg, ConstValue::Float(n)) => Ok(ConstValue::Float(-n)),
        (UnOp::Not, ConstValue::Bool(b)) => Ok(ConstValue::Bool(!b)),
        _ => Err(ConstError::new(
            span,
            "invalid operand for unary operator in const expression",
        )),
    }
}

fn eval_logic(
    op: BinOp,
    left: &Expr,
    right: &Expr,
    env: &HashMap<String, ConstValue>,
    ctx: &mut CtfeContext<'_>,
    span: Span,
) -> Result<ConstValue, ConstError> {
    let l = eval_const_expr_inner(left, env, ctx)?;
    let ConstValue::Bool(lv) = l else {
        return Err(ConstError::new(
            span,
            "logical operator requires Bool operands",
        ));
    };
    match op {
        BinOp::And if !lv => Ok(ConstValue::Bool(false)),
        BinOp::Or if lv => Ok(ConstValue::Bool(true)),
        BinOp::And | BinOp::Or => {
            let r = eval_const_expr_inner(right, env, ctx)?;
            let ConstValue::Bool(rv) = r else {
                return Err(ConstError::new(
                    span,
                    "logical operator requires Bool operands",
                ));
            };
            Ok(ConstValue::Bool(if matches!(op, BinOp::And) {
                lv && rv
            } else {
                lv || rv
            }))
        }
        _ => unreachable!(),
    }
}

fn eval_binary(
    op: BinOp,
    left: ConstValue,
    right: ConstValue,
    span: Span,
) -> Result<ConstValue, ConstError> {
    match (op, left, right) {
        (BinOp::Add, ConstValue::Int(a), ConstValue::Int(b)) => a
            .checked_add(b)
            .map(ConstValue::Int)
            .ok_or_else(|| ConstError::new(span, "integer overflow in const expression")),
        (BinOp::Sub, ConstValue::Int(a), ConstValue::Int(b)) => a
            .checked_sub(b)
            .map(ConstValue::Int)
            .ok_or_else(|| ConstError::new(span, "integer overflow in const expression")),
        (BinOp::Mul, ConstValue::Int(a), ConstValue::Int(b)) => a
            .checked_mul(b)
            .map(ConstValue::Int)
            .ok_or_else(|| ConstError::new(span, "integer overflow in const expression")),
        (BinOp::Div, ConstValue::Int(a), ConstValue::Int(b)) => {
            if b == 0 {
                return Err(ConstError::new(
                    span,
                    "division by zero in const expression",
                ));
            }
            a.checked_div(b)
                .map(ConstValue::Int)
                .ok_or_else(|| ConstError::new(span, "integer overflow in const expression"))
        }
        (BinOp::Rem, ConstValue::Int(a), ConstValue::Int(b)) => {
            if b == 0 {
                return Err(ConstError::new(
                    span,
                    "division by zero in const expression",
                ));
            }
            a.checked_rem(b)
                .map(ConstValue::Int)
                .ok_or_else(|| ConstError::new(span, "integer overflow in const expression"))
        }
        (BinOp::Add, ConstValue::Float(a), ConstValue::Float(b)) => Ok(ConstValue::Float(a + b)),
        (BinOp::Sub, ConstValue::Float(a), ConstValue::Float(b)) => Ok(ConstValue::Float(a - b)),
        (BinOp::Mul, ConstValue::Float(a), ConstValue::Float(b)) => Ok(ConstValue::Float(a * b)),
        (BinOp::Div, ConstValue::Float(a), ConstValue::Float(b)) => Ok(ConstValue::Float(a / b)),
        (BinOp::Add, ConstValue::String(a), ConstValue::String(b)) => {
            Ok(ConstValue::String(format!("{a}{b}")))
        }
        (BinOp::Eq, a, b) => Ok(ConstValue::Bool(const_eq(&a, &b))),
        (BinOp::Ne, a, b) => Ok(ConstValue::Bool(!const_eq(&a, &b))),
        (BinOp::Lt, ConstValue::Int(a), ConstValue::Int(b)) => Ok(ConstValue::Bool(a < b)),
        (BinOp::Le, ConstValue::Int(a), ConstValue::Int(b)) => Ok(ConstValue::Bool(a <= b)),
        (BinOp::Gt, ConstValue::Int(a), ConstValue::Int(b)) => Ok(ConstValue::Bool(a > b)),
        (BinOp::Ge, ConstValue::Int(a), ConstValue::Int(b)) => Ok(ConstValue::Bool(a >= b)),
        (BinOp::Lt, ConstValue::String(a), ConstValue::String(b)) => Ok(ConstValue::Bool(a < b)),
        (BinOp::Le, ConstValue::String(a), ConstValue::String(b)) => Ok(ConstValue::Bool(a <= b)),
        (BinOp::Gt, ConstValue::String(a), ConstValue::String(b)) => Ok(ConstValue::Bool(a > b)),
        (BinOp::Ge, ConstValue::String(a), ConstValue::String(b)) => Ok(ConstValue::Bool(a >= b)),
        _ => Err(ConstError::new(
            span,
            "invalid operands for operator in const expression",
        )),
    }
}

fn const_eq(a: &ConstValue, b: &ConstValue) -> bool {
    match (a, b) {
        (ConstValue::Nil, ConstValue::Nil) => true,
        (ConstValue::Bool(x), ConstValue::Bool(y)) => x == y,
        (ConstValue::Int(x), ConstValue::Int(y)) => x == y,
        (ConstValue::Float(x), ConstValue::Float(y)) => x == y,
        (ConstValue::String(x), ConstValue::String(y)) => x == y,
        _ => false,
    }
}

fn display_const(value: &ConstValue) -> String {
    match value {
        ConstValue::Nil => "nil".into(),
        ConstValue::Bool(b) => b.to_string(),
        ConstValue::Int(n) => n.to_string(),
        ConstValue::Float(n) => n.to_string(),
        ConstValue::String(s) => s.clone(),
    }
}
