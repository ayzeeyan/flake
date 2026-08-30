//! Pretty-printer for Flake ASTs. Output is valid Flake source.

use crate::ast::{
    AssignOp, BinOp, Block, EffectSet, Expr, FnDecl, InterpPart, Item, LetStmt, Literal, Program,
    Stmt, TypeExpr, UnOp,
};

/// Pretty-print a complete program.
#[must_use]
pub fn print_program(program: &Program) -> String {
    let mut out = String::new();
    for (i, item) in program.items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        print_item(item, &mut out);
        out.push('\n');
    }
    out
}

fn print_type_params(params: &[crate::ast::TypeParam], out: &mut String) {
    if params.is_empty() {
        return;
    }
    out.push('[');
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&p.name.name);
        if !p.bounds.is_empty() {
            out.push_str(": ");
            for (j, bound) in p.bounds.iter().enumerate() {
                if j > 0 {
                    out.push_str(" + ");
                }
                out.push_str(&bound.name);
            }
        }
    }
    out.push(']');
}

fn print_item(item: &Item, out: &mut String) {
    match item {
        Item::Fn(f) => print_fn(f, out),
        Item::Struct(s) => {
            if s.is_pub {
                out.push_str("pub ");
            }
            out.push_str("struct ");
            out.push_str(&s.name.name);
            print_type_params(&s.type_params, out);
            out.push_str(" {\n");
            for field in &s.fields {
                out.push_str("    ");
                out.push_str(&field.name.name);
                out.push_str(": ");
                print_type(&field.ty, out);
                out.push('\n');
            }
            out.push('}');
        }
        Item::Enum(e) => {
            if e.is_pub {
                out.push_str("pub ");
            }
            out.push_str("enum ");
            out.push_str(&e.name.name);
            print_type_params(&e.type_params, out);
            out.push_str(" {\n");
            for v in &e.variants {
                out.push_str("    ");
                out.push_str(&v.name.name);
                if !v.fields.is_empty() {
                    out.push('(');
                    for (i, ty) in v.fields.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        print_type(ty, out);
                    }
                    out.push(')');
                }
                out.push('\n');
            }
            out.push('}');
        }
        Item::Type(t) => {
            if t.is_pub {
                out.push_str("pub ");
            }
            out.push_str("type ");
            out.push_str(&t.name.name);
            print_type_params(&t.type_params, out);
            out.push_str(" = ");
            print_type(&t.ty, out);
        }
        Item::Import(i) => {
            out.push_str("import ");
            out.push_str(&i.path.name);
            if let Some(alias) = &i.alias {
                out.push_str(" as ");
                out.push_str(&alias.name);
            }
        }
        Item::Trait(t) => {
            if t.is_pub {
                out.push_str("pub ");
            }
            out.push_str("trait ");
            out.push_str(&t.name.name);
            out.push_str(" {}");
        }
        Item::Impl(i) => {
            out.push_str("impl");
            print_type_params(&i.type_params, out);
            out.push(' ');
            out.push_str(&i.trait_name.name);
            out.push_str(" for ");
            print_type(&i.ty, out);
            out.push_str(" {}");
        }
    }
}

fn print_fn(f: &FnDecl, out: &mut String) {
    if f.is_pub {
        out.push_str("pub ");
    }
    if f.strict {
        out.push_str("strict ");
    }
    if f.owned {
        out.push_str("owned ");
    }
    out.push_str("fn ");
    out.push_str(&f.name.name);
    print_type_params(&f.type_params, out);
    out.push('(');
    for (i, p) in f.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&p.name.name);
        if let Some(ty) = &p.ty {
            out.push_str(": ");
            print_type(ty, out);
        }
    }
    out.push(')');
    if let Some(ret) = &f.return_type {
        out.push_str(" -> ");
        print_type(ret, out);
    }
    print_effects(&f.effects, out);
    out.push(' ');
    print_block(&f.body, 0, out);
}

fn print_effects(effects: &EffectSet, out: &mut String) {
    if !effects.specified {
        return;
    }
    out.push_str(" / ");
    if effects.effects.is_empty() {
        out.push_str("pure");
        return;
    }
    for (i, e) in effects.effects.iter().enumerate() {
        if i > 0 {
            out.push_str(" + ");
        }
        out.push_str(&e.name);
    }
}

fn print_block(block: &Block, indent: usize, out: &mut String) {
    out.push('{');
    if block.is_empty() {
        out.push('}');
        return;
    }
    out.push('\n');
    for stmt in &block.stmts {
        pad(indent + 1, out);
        print_stmt(stmt, indent + 1, out);
        out.push('\n');
    }
    if let Some(tail) = &block.tail {
        pad(indent + 1, out);
        print_expr(tail, indent + 1, out);
        out.push('\n');
    }
    pad(indent, out);
    out.push('}');
}

fn print_stmt(stmt: &Stmt, indent: usize, out: &mut String) {
    match stmt {
        Stmt::Let(s) => print_let("let", s, indent, out),
        Stmt::Var(s) => print_let("var", s, indent, out),
        Stmt::Return { value, .. } => {
            out.push_str("return");
            if let Some(v) = value {
                out.push(' ');
                print_expr(v, indent, out);
            }
        }
        Stmt::Break { .. } => out.push_str("break"),
        Stmt::Continue { .. } => out.push_str("continue"),
        Stmt::While { cond, body, .. } => {
            out.push_str("while ");
            print_expr(cond, indent, out);
            out.push(' ');
            print_block(body, indent, out);
        }
        Stmt::For {
            name, iter, body, ..
        } => {
            out.push_str("for ");
            out.push_str(&name.name);
            out.push_str(" in ");
            print_expr(iter, indent, out);
            out.push(' ');
            print_block(body, indent, out);
        }
        Stmt::Loop { body, .. } => {
            out.push_str("loop ");
            print_block(body, indent, out);
        }
        Stmt::Expr(e) => print_expr(e, indent, out),
    }
}

fn print_let(kw: &str, s: &LetStmt, indent: usize, out: &mut String) {
    out.push_str(kw);
    out.push(' ');
    out.push_str(&s.name.name);
    if let Some(ty) = &s.ty {
        out.push_str(": ");
        print_type(ty, out);
    }
    out.push_str(" = ");
    print_expr(&s.value, indent, out);
}

fn print_expr(expr: &Expr, indent: usize, out: &mut String) {
    match expr {
        Expr::Literal { value, .. } => print_literal(value, out),
        Expr::Ident(id) => out.push_str(&id.name),
        Expr::Interpolated { parts, .. } => {
            out.push('"');
            for part in parts {
                match part {
                    InterpPart::Text(t) => out.push_str(&escape_string(t)),
                    InterpPart::Expr(e) => {
                        out.push('{');
                        print_expr(e, indent, out);
                        out.push('}');
                    }
                }
            }
            out.push('"');
        }
        Expr::List { elements, .. } => {
            out.push('[');
            for (i, e) in elements.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_expr(e, indent, out);
            }
            out.push(']');
        }
        Expr::Map { entries, .. } => {
            out.push('{');
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_expr(k, indent, out);
                out.push_str(": ");
                print_expr(v, indent, out);
            }
            out.push('}');
        }
        Expr::Unary { op, expr, .. } => {
            out.push_str(op.as_str());
            if needs_paren_unary(expr) {
                out.push('(');
                print_expr(expr, indent, out);
                out.push(')');
            } else {
                print_expr(expr, indent, out);
            }
        }
        Expr::Binary {
            op, left, right, ..
        } => {
            print_expr(left, indent, out);
            out.push(' ');
            out.push_str(op.as_str());
            out.push(' ');
            print_expr(right, indent, out);
        }
        Expr::Assign {
            op, target, value, ..
        } => {
            print_expr(target, indent, out);
            out.push(' ');
            out.push_str(op.as_str());
            out.push(' ');
            print_expr(value, indent, out);
        }
        Expr::Call { callee, args, .. } => {
            print_expr(callee, indent, out);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_expr(a, indent, out);
            }
            out.push(')');
        }
        Expr::Spawn { call, .. } => {
            out.push_str("spawn ");
            print_expr(call, indent, out);
        }
        Expr::Await { task, .. } => {
            out.push_str("await ");
            print_expr(task, indent, out);
        }
        Expr::Try { expr, .. } => {
            if needs_paren_postfix(expr) {
                out.push('(');
                print_expr(expr, indent, out);
                out.push(')');
            } else {
                print_expr(expr, indent, out);
            }
            out.push('?');
        }
        Expr::Index { target, index, .. } => {
            print_expr(target, indent, out);
            out.push('[');
            print_expr(index, indent, out);
            out.push(']');
        }
        Expr::Field { target, field, .. } => {
            print_expr(target, indent, out);
            out.push('.');
            out.push_str(&field.name);
        }
        Expr::Range { start, end, .. } => {
            print_expr(start, indent, out);
            out.push_str("..");
            print_expr(end, indent, out);
        }
        Expr::If {
            cond,
            then_block,
            else_block,
            ..
        } => {
            out.push_str("if ");
            print_expr(cond, indent, out);
            out.push(' ');
            print_block(then_block, indent, out);
            if let Some(els) = else_block {
                out.push_str(" else ");
                match els.as_ref() {
                    Expr::Block(b) => print_block(b, indent, out),
                    other => print_expr(other, indent, out),
                }
            }
        }
        Expr::Block(b) => print_block(b, indent, out),
        Expr::Nursery { body, .. } => {
            out.push_str("nursery ");
            print_block(body, indent, out);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            out.push_str("match ");
            print_expr(scrutinee, indent, out);
            out.push_str(" {\n");
            for arm in arms {
                for _ in 0..(indent + 1) {
                    out.push_str("    ");
                }
                print_pattern(&arm.pattern, out);
                out.push_str(" => ");
                print_expr(&arm.body, indent + 1, out);
                out.push('\n');
            }
            for _ in 0..indent {
                out.push_str("    ");
            }
            out.push('}');
        }
        Expr::StructInit { name, fields, .. } => {
            out.push_str(&name.name);
            out.push_str(" { ");
            for (i, (field, value)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&field.name);
                out.push_str(": ");
                print_expr(value, indent, out);
            }
            out.push_str(" }");
        }
    }
}

fn print_literal(lit: &Literal, out: &mut String) {
    match lit {
        Literal::Nil => out.push_str("nil"),
        Literal::Bool(true) => out.push_str("true"),
        Literal::Bool(false) => out.push_str("false"),
        Literal::Int(n) => out.push_str(&n.to_string()),
        Literal::Float(n) => {
            let s = n.to_string();
            out.push_str(&s);
            if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                out.push_str(".0");
            }
        }
        Literal::String(s) => {
            out.push('"');
            out.push_str(&escape_string(s));
            out.push('"');
        }
    }
}

fn print_type(ty: &TypeExpr, out: &mut String) {
    match ty {
        TypeExpr::Dyn { .. } => out.push_str("dyn"),
        TypeExpr::Named { name, args, .. } => {
            out.push_str(&name.name);
            if !args.is_empty() {
                out.push('[');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    print_type(a, out);
                }
                out.push(']');
            }
        }
        TypeExpr::List { element, .. } => {
            out.push('[');
            print_type(element, out);
            out.push(']');
        }
        TypeExpr::Optional { inner, .. } => {
            print_type(inner, out);
            out.push('?');
        }
        TypeExpr::Owned { inner, .. } => {
            out.push_str("owned ");
            print_type(inner, out);
        }
        TypeExpr::Ref { mutable, inner, .. } => {
            out.push('&');
            if *mutable {
                out.push_str("mut ");
            }
            print_type(inner, out);
        }
        TypeExpr::Mut { inner, .. } => {
            out.push_str("mut ");
            print_type(inner, out);
        }
        TypeExpr::Fn {
            params,
            ret,
            effects,
            ..
        } => {
            out.push_str("fn(");
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_type(p, out);
            }
            out.push(')');
            if let Some(r) = ret {
                out.push_str(" -> ");
                print_type(r, out);
            }
            print_effects(effects, out);
        }
    }
}

fn escape_string(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            other => out.push(other),
        }
    }
    out
}

fn needs_paren_unary(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Binary { .. } | Expr::Assign { .. } | Expr::Range { .. }
    )
}

fn needs_paren_postfix(expr: &Expr) -> bool {
    !matches!(
        expr,
        Expr::Literal { .. }
            | Expr::Ident(_)
            | Expr::Interpolated { .. }
            | Expr::List { .. }
            | Expr::Map { .. }
            | Expr::Call { .. }
            | Expr::Try { .. }
            | Expr::Index { .. }
            | Expr::Field { .. }
            | Expr::StructInit { .. }
    )
}

fn pad(indent: usize, out: &mut String) {
    for _ in 0..indent {
        out.push_str("    ");
    }
}

impl std::fmt::Display for BinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::fmt::Display for UnOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::fmt::Display for AssignOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub fn print_pattern(pattern: &crate::ast::Pattern, out: &mut String) {
    match pattern {
        crate::ast::Pattern::Wildcard { .. } => out.push('_'),
        crate::ast::Pattern::Literal { value, .. } => print_literal(value, out),
        crate::ast::Pattern::Ident(id) => out.push_str(&id.name),
        crate::ast::Pattern::List { patterns, .. } => {
            out.push('[');
            for (i, p) in patterns.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_pattern(p, out);
            }
            out.push(']');
        }
        crate::ast::Pattern::Variant {
            ty,
            variant,
            fields,
            ..
        } => {
            if let Some(t) = ty {
                out.push_str(&t.name);
                out.push('.');
            }
            out.push_str(&variant.name);
            if !fields.is_empty() {
                out.push('(');
                for (i, f) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    print_pattern(f, out);
                }
                out.push(')');
            }
        }
    }
}
