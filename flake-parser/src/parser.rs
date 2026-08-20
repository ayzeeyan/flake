//! Recursive-descent parser with Pratt expressions.

use std::mem::discriminant;

use flake_ast::{
    AssignOp, BinOp, Block, EffectSet, Expr, FnDecl, Ident, ImportDecl, InterpPart, Item, LetStmt,
    Literal, Param, Program, Source, Span, Stmt, StructDecl, StructField, TypeAlias, TypeExpr, UnOp,
};
use flake_lexer::{Token, TokenKind, tokenize};

use crate::error::ParseError;

pub fn parse(source: &Source) -> Result<Program, ParseError> {
    let tokens = tokenize(source)?;
    Parser::new(source, tokens).parse_program()
}

pub fn parse_str(text: &str) -> Result<Program, ParseError> {
    parse(&Source::new("<input>", text))
}

struct Parser<'src> {
    source: &'src Source,
    tokens: Vec<Token>,
    pos: usize,
}

impl<'src> Parser<'src> {
    fn new(source: &'src Source, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            pos: 0,
        }
    }

    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let start = self.current().span;
        let mut items = Vec::new();
        self.skip_nl();
        while !self.at_eof() {
            items.push(self.parse_item()?);
            self.skip_nl();
        }
        let end = self.current().span;
        Ok(Program {
            items,
            span: start.merge(end),
        })
    }

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        let start = self.current().span;
        let is_pub = self.eat(&TokenKind::Pub);
        let strict = self.eat(&TokenKind::Strict);
        let owned = self.eat(&TokenKind::Owned);

        if matches!(self.kind(), TokenKind::Fn) {
            return Ok(Item::Fn(self.parse_fn(start, is_pub, strict, owned)?));
        }
        if strict || owned {
            return Err(self.error("expected `fn` after `strict` / `owned`"));
        }
        if matches!(self.kind(), TokenKind::Struct) {
            return Ok(Item::Struct(self.parse_struct(start, is_pub)?));
        }
        if matches!(self.kind(), TokenKind::Type) {
            return Ok(Item::Type(self.parse_type_alias(start, is_pub)?));
        }
        if is_pub {
            return Err(self.error("expected `fn`, `struct`, or `type` after `pub`"));
        }
        if matches!(self.kind(), TokenKind::Import) {
            return Ok(Item::Import(self.parse_import()?));
        }
        Err(self.unexpected("top-level item (`fn`, `struct`, `type`, or `import`)"))
    }

    fn parse_fn(
        &mut self,
        start: Span,
        is_pub: bool,
        strict: bool,
        owned: bool,
    ) -> Result<FnDecl, ParseError> {
        self.expect(&TokenKind::Fn, "`fn`")?;
        let name = self.parse_ident()?;
        self.expect(&TokenKind::LParen, "`(`")?;
        let params = self.parse_params()?;
        self.expect(&TokenKind::RParen, "`)`")?;
        self.skip_nl();

        let return_type = if self.eat(&TokenKind::Arrow) {
            self.skip_nl();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.skip_nl();
        let effects = self.parse_effect_clause()?;
        self.skip_nl();
        let body = self.parse_block()?;
        let span = start.merge(body.span);
        Ok(FnDecl {
            is_pub,
            strict,
            owned,
            name,
            params,
            return_type,
            effects,
            body,
            span,
        })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        self.skip_nl();
        if matches!(self.kind(), TokenKind::RParen) {
            return Ok(params);
        }
        loop {
            self.skip_nl();
            let name = self.parse_ident()?;
            let mut span = name.span;
            self.skip_nl();
            let ty = if self.eat(&TokenKind::Colon) {
                self.skip_nl();
                let ty = self.parse_type()?;
                span = span.merge(ty.span());
                Some(ty)
            } else {
                None
            };
            params.push(Param { name, ty, span });
            self.skip_nl();
            if self.eat(&TokenKind::Comma) {
                self.skip_nl();
                if matches!(self.kind(), TokenKind::RParen) {
                    break;
                }
                continue;
            }
            break;
        }
        Ok(params)
    }

    fn parse_effect_clause(&mut self) -> Result<EffectSet, ParseError> {
        if !self.eat(&TokenKind::Slash) {
            return Ok(EffectSet::unspecified());
        }
        let start = self.prev().span;
        self.skip_nl();
        let mut effects = Vec::new();
        let first = self.parse_ident()?;
        let mut span = start.merge(first.span);
        effects.push(first);
        self.skip_nl();
        while self.eat(&TokenKind::Plus) {
            self.skip_nl();
            let e = self.parse_ident()?;
            span = span.merge(e.span);
            effects.push(e);
            self.skip_nl();
        }
        Ok(EffectSet {
            effects,
            specified: true,
            span,
        })
    }

    fn parse_struct(&mut self, start: Span, is_pub: bool) -> Result<StructDecl, ParseError> {
        self.expect(&TokenKind::Struct, "`struct`")?;
        let name = self.parse_ident()?;
        self.skip_nl();
        self.expect(&TokenKind::LBrace, "`{`")?;
        let mut fields = Vec::new();
        loop {
            self.skip_nl();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            let field_name = self.parse_ident()?;
            self.skip_nl();
            self.expect(&TokenKind::Colon, "`:`")?;
            self.skip_nl();
            let ty = self.parse_type()?;
            let span = field_name.span.merge(ty.span());
            fields.push(StructField {
                name: field_name,
                ty,
                span,
            });
            self.skip_nl();
            self.eat(&TokenKind::Comma);
            self.skip_nl();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
        }
        let span = start.merge(self.prev().span);
        Ok(StructDecl {
            is_pub,
            name,
            fields,
            span,
        })
    }

    fn parse_type_alias(&mut self, start: Span, is_pub: bool) -> Result<TypeAlias, ParseError> {
        self.expect(&TokenKind::Type, "`type`")?;
        let name = self.parse_ident()?;
        self.skip_nl();
        self.expect(&TokenKind::Eq, "`=`")?;
        self.skip_nl();
        let ty = self.parse_type()?;
        Ok(TypeAlias {
            is_pub,
            name,
            span: start.merge(ty.span()),
            ty,
        })
    }

    fn parse_import(&mut self) -> Result<ImportDecl, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::Import, "`import`")?;
        let path = self.parse_ident()?;
        let mut span = start.merge(path.span);
        let alias = if self.eat(&TokenKind::As) {
            let alias = self.parse_ident()?;
            span = span.merge(alias.span);
            Some(alias)
        } else {
            None
        };
        Ok(ImportDecl { path, alias, span })
    }

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        let mut last_semi = true;
        loop {
            self.skip_nl();
            if matches!(self.kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            let stmt = self.parse_stmt()?;
            last_semi = self.eat(&TokenKind::Semicolon);
            stmts.push(stmt);
        }
        self.expect(&TokenKind::RBrace, "`}`")?;
        let span = start.merge(self.prev().span);

        let tail = if !last_semi {
            if let Some(Stmt::Expr(_)) = stmts.last() {
                match stmts.pop() {
                    Some(Stmt::Expr(e)) => Some(Box::new(e)),
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        };

        Ok(Block { stmts, tail, span })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.skip_nl();
        match self.kind() {
            TokenKind::Let => Ok(Stmt::Let(self.parse_let()?)),
            TokenKind::Var => Ok(Stmt::Var(self.parse_let()?)),
            TokenKind::Return => {
                let start = self.bump().span;
                let value = if self.stmt_end() {
                    None
                } else {
                    Some(self.parse_expr(0)?)
                };
                let span = match &value {
                    Some(v) => start.merge(v.span()),
                    None => start,
                };
                Ok(Stmt::Return { value, span })
            }
            TokenKind::Break => {
                let span = self.bump().span;
                Ok(Stmt::Break { span })
            }
            TokenKind::Continue => {
                let span = self.bump().span;
                Ok(Stmt::Continue { span })
            }
            TokenKind::While => {
                let start = self.bump().span;
                let cond = self.parse_expr(0)?;
                self.skip_nl();
                let body = self.parse_block()?;
                let span = start.merge(body.span);
                Ok(Stmt::While { cond, body, span })
            }
            TokenKind::For => {
                let start = self.bump().span;
                let name = self.parse_ident()?;
                self.skip_nl();
                self.expect(&TokenKind::In, "`in`")?;
                self.skip_nl();
                let iter = self.parse_expr(0)?;
                self.skip_nl();
                let body = self.parse_block()?;
                let span = start.merge(body.span);
                Ok(Stmt::For {
                    name,
                    iter,
                    body,
                    span,
                })
            }
            TokenKind::Loop => {
                let start = self.bump().span;
                self.skip_nl();
                let body = self.parse_block()?;
                let span = start.merge(body.span);
                Ok(Stmt::Loop { body, span })
            }
            _ => Ok(Stmt::Expr(self.parse_expr(0)?)),
        }
    }

    fn parse_let(&mut self) -> Result<LetStmt, ParseError> {
        let start = self.bump().span;
        let name = self.parse_ident()?;
        self.skip_nl();
        let ty = if self.eat(&TokenKind::Colon) {
            self.skip_nl();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.skip_nl();
        self.expect(&TokenKind::Eq, "`=`")?;
        self.skip_nl();
        let value = self.parse_expr(0)?;
        let span = start.merge(value.span());
        Ok(LetStmt {
            name,
            ty,
            value,
            span,
        })
    }

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix()?;
        loop {
            if min_bp <= 24 {
                if matches!(self.kind(), TokenKind::LParen) {
                    lhs = self.finish_call(lhs)?;
                    continue;
                }
                if matches!(self.kind(), TokenKind::LBracket) {
                    lhs = self.finish_index(lhs)?;
                    continue;
                }
                if matches!(self.kind(), TokenKind::Dot) {
                    lhs = self.finish_field(lhs)?;
                    continue;
                }
            }

            let Some(op_idx) = self.peek_infix_index() else {
                break;
            };
            let kind = &self.tokens[op_idx].kind;
            let Some((lbp, rbp, infix)) = infix_binding(kind) else {
                break;
            };
            if lbp < min_bp {
                break;
            }
            self.pos = op_idx;
            let op_tok = self.bump();
            self.skip_nl();
            match infix {
                Infix::Assign(op) => {
                    if !is_assign_target(&lhs) {
                        return Err(ParseError::new(
                            lhs.span(),
                            "invalid assignment target",
                        ));
                    }
                    let value = self.parse_expr(rbp)?;
                    let span = lhs.span().merge(value.span());
                    lhs = Expr::Assign {
                        op,
                        target: Box::new(lhs),
                        value: Box::new(value),
                        span,
                    };
                }
                Infix::Range => {
                    let end = self.parse_expr(rbp)?;
                    let span = lhs.span().merge(end.span());
                    lhs = Expr::Range {
                        start: Box::new(lhs),
                        end: Box::new(end),
                        span,
                    };
                }
                Infix::Bin(op) => {
                    let right = self.parse_expr(rbp)?;
                    let span = lhs.span().merge(right.span());
                    lhs = Expr::Binary {
                        op,
                        left: Box::new(lhs),
                        right: Box::new(right),
                        span,
                    };
                }
            }
            let _ = op_tok;
        }
        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        self.skip_nl();
        let tok = self.current().clone();
        match tok.kind {
            TokenKind::Int(n) => {
                self.bump();
                Ok(Expr::Literal {
                    value: Literal::Int(n),
                    span: tok.span,
                })
            }
            TokenKind::Float(n) => {
                self.bump();
                Ok(Expr::Literal {
                    value: Literal::Float(n),
                    span: tok.span,
                })
            }
            TokenKind::True => {
                self.bump();
                Ok(Expr::Literal {
                    value: Literal::Bool(true),
                    span: tok.span,
                })
            }
            TokenKind::False => {
                self.bump();
                Ok(Expr::Literal {
                    value: Literal::Bool(false),
                    span: tok.span,
                })
            }
            TokenKind::Nil => {
                self.bump();
                Ok(Expr::Literal {
                    value: Literal::Nil,
                    span: tok.span,
                })
            }
            TokenKind::Ident => {
                let name = self.parse_ident()?;
                if matches!(self.kind(), TokenKind::LBrace) && self.looks_like_struct_init() {
                    self.parse_struct_init(name)
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            TokenKind::StringStart => self.parse_string(),
            TokenKind::Minus => {
                self.bump();
                let expr = self.parse_expr(18)?;
                let span = tok.span.merge(expr.span());
                Ok(Expr::Unary {
                    op: UnOp::Neg,
                    expr: Box::new(expr),
                    span,
                })
            }
            TokenKind::Bang => {
                self.bump();
                let expr = self.parse_expr(18)?;
                let span = tok.span.merge(expr.span());
                Ok(Expr::Unary {
                    op: UnOp::Not,
                    expr: Box::new(expr),
                    span,
                })
            }
            TokenKind::Amp => {
                self.bump();
                self.skip_nl();
                let (op, expr) = if self.eat(&TokenKind::Mut) {
                    self.skip_nl();
                    (UnOp::RefMut, self.parse_expr(18)?)
                } else {
                    (UnOp::Ref, self.parse_expr(18)?)
                };
                let span = tok.span.merge(expr.span());
                Ok(Expr::Unary {
                    op,
                    expr: Box::new(expr),
                    span,
                })
            }
            TokenKind::LParen => {
                self.bump();
                self.skip_nl();
                if self.eat(&TokenKind::RParen) {
                    return Ok(Expr::Literal {
                        value: Literal::Nil,
                        span: tok.span.merge(self.prev().span),
                    });
                }
                let expr = self.parse_expr(0)?;
                self.skip_nl();
                self.expect(&TokenKind::RParen, "`)`")?;
                Ok(expr)
            }
            TokenKind::LBracket => self.parse_list(),
            TokenKind::LBrace => {
                if self.looks_like_map() {
                    self.parse_map()
                } else {
                    Ok(Expr::Block(self.parse_block()?))
                }
            }
            TokenKind::If => self.parse_if(),
            _ => Err(self.unexpected("expression")),
        }
    }

    fn parse_if(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::If, "`if`")?;
        let cond = self.parse_expr(0)?;
        self.skip_nl();
        let then_block = self.parse_block()?;
        self.skip_nl();
        let else_block = if self.eat(&TokenKind::Else) {
            self.skip_nl();
            if matches!(self.kind(), TokenKind::If) {
                Some(Box::new(self.parse_if()?))
            } else {
                Some(Box::new(Expr::Block(self.parse_block()?)))
            }
        } else {
            None
        };
        let end = else_block
            .as_ref()
            .map(|e| e.span())
            .unwrap_or(then_block.span);
        Ok(Expr::If {
            cond: Box::new(cond),
            then_block,
            else_block,
            span: start.merge(end),
        })
    }

    fn parse_string(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::StringStart, "string")?;
        let mut parts = Vec::new();
        loop {
            match self.kind().clone() {
                TokenKind::StringText(text) => {
                    self.bump();
                    parts.push(InterpPart::Text(text));
                }
                TokenKind::InterpOpen => {
                    self.bump();
                    self.skip_nl();
                    let expr = self.parse_expr(0)?;
                    self.skip_nl();
                    self.expect(&TokenKind::InterpClose, "`}` to close interpolation")?;
                    parts.push(InterpPart::Expr(expr));
                }
                TokenKind::StringEnd => {
                    let end = self.bump().span;
                    let span = start.merge(end);
                    return Ok(fold_string(parts, span));
                }
                _ => return Err(self.unexpected("string content or closing `\"`")),
            }
        }
    }

    fn parse_list(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::LBracket, "`[`")?;
        let mut elements = Vec::new();
        loop {
            self.skip_nl();
            if self.eat(&TokenKind::RBracket) {
                break;
            }
            elements.push(self.parse_expr(0)?);
            self.skip_nl();
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            self.skip_nl();
            self.expect(&TokenKind::RBracket, "`]`")?;
            break;
        }
        Ok(Expr::List {
            elements,
            span: start.merge(self.prev().span),
        })
    }

    fn parse_map(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span;
        self.expect(&TokenKind::LBrace, "`{`")?;
        let mut entries = Vec::new();
        loop {
            self.skip_nl();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            let key = self.parse_expr(0)?;
            self.skip_nl();
            self.expect(&TokenKind::Colon, "`:`")?;
            self.skip_nl();
            let value = self.parse_expr(0)?;
            entries.push((key, value));
            self.skip_nl();
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            self.skip_nl();
            self.expect(&TokenKind::RBrace, "`}`")?;
            break;
        }
        Ok(Expr::Map {
            entries,
            span: start.merge(self.prev().span),
        })
    }

    fn parse_struct_init(&mut self, name: Ident) -> Result<Expr, ParseError> {
        let start = name.span;
        self.expect(&TokenKind::LBrace, "`{`")?;
        let mut fields = Vec::new();
        loop {
            self.skip_nl();
            if self.eat(&TokenKind::RBrace) {
                break;
            }
            let field = self.parse_ident()?;
            self.skip_nl();
            self.expect(&TokenKind::Colon, "`:`")?;
            self.skip_nl();
            let value = self.parse_expr(0)?;
            fields.push((field, value));
            self.skip_nl();
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            self.skip_nl();
            self.expect(&TokenKind::RBrace, "`}`")?;
            break;
        }
        Ok(Expr::StructInit {
            name,
            fields,
            span: start.merge(self.prev().span),
        })
    }

    fn finish_call(&mut self, callee: Expr) -> Result<Expr, ParseError> {
        let start = callee.span();
        self.expect(&TokenKind::LParen, "`(`")?;
        let mut args = Vec::new();
        loop {
            self.skip_nl();
            if self.eat(&TokenKind::RParen) {
                break;
            }
            args.push(self.parse_expr(0)?);
            self.skip_nl();
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            self.skip_nl();
            self.expect(&TokenKind::RParen, "`)`")?;
            break;
        }
        Ok(Expr::Call {
            callee: Box::new(callee),
            args,
            span: start.merge(self.prev().span),
        })
    }

    fn finish_index(&mut self, target: Expr) -> Result<Expr, ParseError> {
        let start = target.span();
        self.expect(&TokenKind::LBracket, "`[`")?;
        self.skip_nl();
        let index = self.parse_expr(0)?;
        self.skip_nl();
        self.expect(&TokenKind::RBracket, "`]`")?;
        Ok(Expr::Index {
            target: Box::new(target),
            index: Box::new(index),
            span: start.merge(self.prev().span),
        })
    }

    fn finish_field(&mut self, target: Expr) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::Dot, "`.`")?;
        self.skip_nl();
        let field = self.parse_ident()?;
        let span = target.span().merge(field.span);
        Ok(Expr::Field {
            target: Box::new(target),
            field,
            span,
        })
    }

    fn parse_type(&mut self) -> Result<TypeExpr, ParseError> {
        let start = self.current().span;
        if self.eat(&TokenKind::Owned) {
            self.skip_nl();
            let inner = self.parse_type()?;
            return Ok(TypeExpr::Owned {
                span: start.merge(inner.span()),
                inner: Box::new(inner),
            });
        }
        if self.eat(&TokenKind::Ref) {
            self.skip_nl();
            let inner = self.parse_type()?;
            return Ok(TypeExpr::Ref {
                mutable: false,
                span: start.merge(inner.span()),
                inner: Box::new(inner),
            });
        }
        if self.eat(&TokenKind::Mut) {
            self.skip_nl();
            let inner = self.parse_type()?;
            return Ok(TypeExpr::Mut {
                span: start.merge(inner.span()),
                inner: Box::new(inner),
            });
        }
        if self.eat(&TokenKind::Amp) {
            self.skip_nl();
            let mutable = self.eat(&TokenKind::Mut);
            self.skip_nl();
            let inner = self.parse_type()?;
            return Ok(TypeExpr::Ref {
                mutable,
                span: start.merge(inner.span()),
                inner: Box::new(inner),
            });
        }
        let mut ty = self.parse_type_atom()?;
        if self.eat(&TokenKind::Question) {
            ty = TypeExpr::Optional {
                span: ty.span().merge(self.prev().span),
                inner: Box::new(ty),
            };
        }
        Ok(ty)
    }

    fn parse_type_atom(&mut self) -> Result<TypeExpr, ParseError> {
        let start = self.current().span;
        if self.eat(&TokenKind::Dyn) {
            return Ok(TypeExpr::Dyn { span: start });
        }
        if self.eat(&TokenKind::Fn) {
            self.expect(&TokenKind::LParen, "`(`")?;
            let mut params = Vec::new();
            loop {
                self.skip_nl();
                if self.eat(&TokenKind::RParen) {
                    break;
                }
                params.push(self.parse_type()?);
                self.skip_nl();
                if self.eat(&TokenKind::Comma) {
                    continue;
                }
                self.skip_nl();
                self.expect(&TokenKind::RParen, "`)`")?;
                break;
            }
            self.skip_nl();
            let ret = if self.eat(&TokenKind::Arrow) {
                self.skip_nl();
                Some(Box::new(self.parse_type()?))
            } else {
                None
            };
            self.skip_nl();
            let effects = self.parse_effect_clause()?;
            let end = ret
                .as_ref()
                .map(|r| r.span())
                .unwrap_or(self.prev().span)
                .merge(if effects.specified {
                    effects.span
                } else {
                    Span::DUMMY
                });
            return Ok(TypeExpr::Fn {
                params,
                ret,
                effects,
                span: start.merge(end),
            });
        }
        if self.eat(&TokenKind::LBracket) {
            self.skip_nl();
            let element = self.parse_type()?;
            self.skip_nl();
            self.expect(&TokenKind::RBracket, "`]`")?;
            return Ok(TypeExpr::List {
                element: Box::new(element),
                span: start.merge(self.prev().span),
            });
        }
        let name = self.parse_ident()?;
        let mut span = name.span;
        let mut args = Vec::new();
        if self.eat(&TokenKind::LBracket) {
            loop {
                self.skip_nl();
                if self.eat(&TokenKind::RBracket) {
                    break;
                }
                args.push(self.parse_type()?);
                self.skip_nl();
                if self.eat(&TokenKind::Comma) {
                    continue;
                }
                self.skip_nl();
                self.expect(&TokenKind::RBracket, "`]`")?;
                break;
            }
            span = start.merge(self.prev().span);
        }
        Ok(TypeExpr::Named { name, args, span })
    }

    fn parse_ident(&mut self) -> Result<Ident, ParseError> {
        let tok = self.current().clone();
        if matches!(tok.kind, TokenKind::Ident) {
            let name = self.source.slice(tok.span).to_string();
            self.bump();
            Ok(Ident::new(name, tok.span))
        } else {
            Err(self.unexpected("identifier"))
        }
    }

    fn looks_like_map(&self) -> bool {
        // `{ key : value` where current token is `{`.
        let mut i = self.pos + 1;
        while i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::Newline) {
            i += 1;
        }
        if i >= self.tokens.len() || matches!(self.tokens[i].kind, TokenKind::RBrace) {
            return false;
        }
        match &self.tokens[i].kind {
            TokenKind::StringStart => {
                i += 1;
                while i < self.tokens.len()
                    && !matches!(self.tokens[i].kind, TokenKind::StringEnd | TokenKind::Eof)
                {
                    i += 1;
                }
                if i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::StringEnd) {
                    i += 1;
                }
            }
            TokenKind::Ident
            | TokenKind::Int(_)
            | TokenKind::Float(_)
            | TokenKind::True
            | TokenKind::False
            | TokenKind::Nil => {
                i += 1;
            }
            _ => return false,
        }
        while i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::Newline) {
            i += 1;
        }
        i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::Colon)
    }

    fn looks_like_struct_init(&self) -> bool {
        // Current token is `{`. Field inits start with `ident :`.
        let mut i = self.pos + 1;
        while i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::Newline) {
            i += 1;
        }
        if i >= self.tokens.len() {
            return false;
        }
        if matches!(self.tokens[i].kind, TokenKind::RBrace) {
            return true;
        }
        if !matches!(self.tokens[i].kind, TokenKind::Ident) {
            return false;
        }
        let mut j = i + 1;
        while j < self.tokens.len() && matches!(self.tokens[j].kind, TokenKind::Newline) {
            j += 1;
        }
        j < self.tokens.len() && matches!(self.tokens[j].kind, TokenKind::Colon)
    }

    /// Look ahead past newlines for an infix operator. Returns its index.
    fn peek_infix_index(&self) -> Option<usize> {
        let mut i = self.pos;
        let mut skipped_nl = false;
        while i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::Newline) {
            skipped_nl = true;
            i += 1;
        }
        if i >= self.tokens.len() {
            return None;
        }
        let kind = &self.tokens[i].kind;
        if skipped_nl && matches!(kind, TokenKind::LParen | TokenKind::LBracket) {
            return None;
        }
        if infix_binding(kind).is_some() {
            Some(i)
        } else {
            None
        }
    }

    fn stmt_end(&self) -> bool {
        matches!(
            self.kind(),
            TokenKind::Newline | TokenKind::Semicolon | TokenKind::RBrace | TokenKind::Eof
        )
    }

    fn skip_nl(&mut self) {
        while matches!(self.kind(), TokenKind::Newline) {
            self.bump();
        }
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if discriminant(self.kind()) == discriminant(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: &TokenKind, expected: &str) -> Result<Token, ParseError> {
        if discriminant(self.kind()) == discriminant(kind) {
            Ok(self.bump())
        } else {
            Err(self.unexpected(expected))
        }
    }

    fn unexpected(&self, expected: &str) -> ParseError {
        ParseError::new(
            self.current().span,
            format!("expected {expected}, found {}", self.current().kind.as_str()),
        )
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError::new(self.current().span, message)
    }

    fn kind(&self) -> &TokenKind {
        &self.current().kind
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn prev(&self) -> &Token {
        &self.tokens[self.pos.saturating_sub(1)]
    }

    fn at_eof(&self) -> bool {
        matches!(self.kind(), TokenKind::Eof)
    }

    fn bump(&mut self) -> Token {
        let tok = self.current().clone();
        if !matches!(tok.kind, TokenKind::Eof) && self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        } else if !matches!(tok.kind, TokenKind::Eof) {
            self.pos = self.tokens.len() - 1;
        }
        tok
    }
}

enum Infix {
    Bin(BinOp),
    Assign(AssignOp),
    Range,
}

fn infix_binding(kind: &TokenKind) -> Option<(u8, u8, Infix)> {
    Some(match kind {
        TokenKind::Eq => (1, 0, Infix::Assign(AssignOp::Assign)),
        TokenKind::PlusEq => (1, 0, Infix::Assign(AssignOp::AddAssign)),
        TokenKind::MinusEq => (1, 0, Infix::Assign(AssignOp::SubAssign)),
        TokenKind::StarEq => (1, 0, Infix::Assign(AssignOp::MulAssign)),
        TokenKind::SlashEq => (1, 0, Infix::Assign(AssignOp::DivAssign)),
        TokenKind::PercentEq => (1, 0, Infix::Assign(AssignOp::RemAssign)),
        TokenKind::PipePipe => (2, 3, Infix::Bin(BinOp::Or)),
        TokenKind::AmpAmp => (4, 5, Infix::Bin(BinOp::And)),
        TokenKind::EqEq => (6, 7, Infix::Bin(BinOp::Eq)),
        TokenKind::BangEq => (6, 7, Infix::Bin(BinOp::Ne)),
        TokenKind::Lt => (8, 9, Infix::Bin(BinOp::Lt)),
        TokenKind::LtEq => (8, 9, Infix::Bin(BinOp::Le)),
        TokenKind::Gt => (8, 9, Infix::Bin(BinOp::Gt)),
        TokenKind::GtEq => (8, 9, Infix::Bin(BinOp::Ge)),
        TokenKind::DotDot => (10, 11, Infix::Range),
        TokenKind::Plus => (12, 13, Infix::Bin(BinOp::Add)),
        TokenKind::Minus => (12, 13, Infix::Bin(BinOp::Sub)),
        TokenKind::Star => (14, 15, Infix::Bin(BinOp::Mul)),
        TokenKind::Slash => (14, 15, Infix::Bin(BinOp::Div)),
        TokenKind::Percent => (14, 15, Infix::Bin(BinOp::Rem)),
        _ => return None,
    })
}

fn is_assign_target(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Ident(_) | Expr::Index { .. } | Expr::Field { .. }
    )
}

fn fold_string(parts: Vec<InterpPart>, span: Span) -> Expr {
    match parts.as_slice() {
        [] => Expr::Literal {
            value: Literal::String(String::new()),
            span,
        },
        [InterpPart::Text(s)] => Expr::Literal {
            value: Literal::String(s.clone()),
            span,
        },
        _ => Expr::Interpolated { parts, span },
    }
}
