//! AST → bytecode compiler.

use flake_ast::{
    AssignOp, BinOp, Block, Expr, FnDecl, InterpPart, Item, Literal, Program, Stmt, UnOp,
};

use crate::error::VmError;
use crate::opcode::{Chunk, Op};
use crate::value::{Function, Value};

pub struct Compiled {
    pub functions: Vec<Function>,
}

struct FnCompiler {
    chunk: Chunk,
    locals: Vec<String>,
}

impl FnCompiler {
    fn new(params: Vec<String>) -> Self {
        Self {
            chunk: Chunk::new(),
            locals: params,
        }
    }

    fn finish(self, name: String, arity: u8) -> Function {
        let locals = self.locals.len() as u16;
        Function {
            name,
            arity,
            chunk: self.chunk,
            locals,
        }
    }

    fn add_local(&mut self, name: String) -> u16 {
        let i = self.locals.len() as u16;
        self.locals.push(name);
        i
    }

    fn resolve_local(&self, name: &str) -> Option<u16> {
        self.locals
            .iter()
            .enumerate()
            .rev()
            .find(|(_, n)| n.as_str() == name)
            .map(|(i, _)| i as u16)
    }

    fn compile_block_value(&mut self, block: &Block) -> Result<(), VmError> {
        for stmt in &block.stmts {
            self.compile_stmt(stmt)?;
        }
        if let Some(tail) = &block.tail {
            self.compile_expr(tail)?;
        } else {
            self.chunk.emit(Op::Nil);
        }
        Ok(())
    }

    fn compile_block_as_stmt(&mut self, block: &Block) -> Result<(), VmError> {
        self.compile_block_value(block)?;
        self.chunk.emit(Op::Pop);
        Ok(())
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), VmError> {
        match stmt {
            Stmt::Let(s) | Stmt::Var(s) => {
                self.compile_expr(&s.value)?;
                let slot = self.add_local(s.name.name.clone());
                self.chunk.emit(Op::SetLocal(slot));
                self.chunk.emit(Op::Pop);
                Ok(())
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.compile_expr(v)?;
                } else {
                    self.chunk.emit(Op::Nil);
                }
                self.chunk.emit(Op::Return);
                Ok(())
            }
            Stmt::Expr(e) => {
                self.compile_expr(e)?;
                self.chunk.emit(Op::Pop);
                Ok(())
            }
            Stmt::While { cond, body, .. } => {
                let start = self.chunk.ops.len() as u16;
                self.compile_expr(cond)?;
                let exit = self.chunk.emit(Op::JumpIfFalse(0));
                self.chunk.emit(Op::Pop);
                self.compile_block_as_stmt(body)?;
                self.chunk.emit(Op::Jump(start));
                let end = self.chunk.ops.len();
                self.chunk.patch_jump(exit, end);
                self.chunk.emit(Op::Pop);
                Ok(())
            }
            Stmt::Loop { body, .. } => {
                let start = self.chunk.ops.len() as u16;
                self.compile_block_as_stmt(body)?;
                self.chunk.emit(Op::Jump(start));
                Ok(())
            }
            Stmt::For { span, .. } => Err(VmError::new(
                *span,
                "the bytecode VM does not yet compile `for` loops; use `while` or run without `--vm`",
            )),
            Stmt::Break { span } | Stmt::Continue { span } => Err(VmError::new(
                *span,
                "`break`/`continue` are not yet compiled to bytecode",
            )),
        }
    }

    fn compile_expr(&mut self, expr: &Expr) -> Result<(), VmError> {
        match expr {
            Expr::Literal { value, .. } => {
                match value {
                    Literal::Nil => {
                        self.chunk.emit(Op::Nil);
                    }
                    Literal::Bool(true) => {
                        self.chunk.emit(Op::True);
                    }
                    Literal::Bool(false) => {
                        self.chunk.emit(Op::False);
                    }
                    Literal::Int(n) => {
                        let c = self.chunk.add_constant(Value::Int(*n));
                        self.chunk.emit(Op::Constant(c));
                    }
                    Literal::Float(n) => {
                        let c = self.chunk.add_constant(Value::Float(*n));
                        self.chunk.emit(Op::Constant(c));
                    }
                    Literal::String(s) => {
                        let c = self.chunk.add_constant(Value::from_string(s.clone()));
                        self.chunk.emit(Op::Constant(c));
                    }
                }
                Ok(())
            }
            Expr::Ident(id) => {
                if let Some(slot) = self.resolve_local(&id.name) {
                    self.chunk.emit(Op::GetLocal(slot));
                } else {
                    let c = self.chunk.add_constant(Value::from_string(id.name.clone()));
                    self.chunk.emit(Op::GetGlobal(c));
                }
                Ok(())
            }
            Expr::Interpolated { parts, .. } => {
                let mut n = 0u8;
                for part in parts {
                    match part {
                        InterpPart::Text(t) => {
                            let c = self.chunk.add_constant(Value::from_string(t.clone()));
                            self.chunk.emit(Op::Constant(c));
                            n += 1;
                        }
                        InterpPart::Expr(e) => {
                            self.compile_expr(e)?;
                            n += 1;
                        }
                    }
                }
                self.chunk.emit(Op::Concat(n));
                Ok(())
            }
            Expr::List { elements, .. } => {
                for e in elements {
                    self.compile_expr(e)?;
                }
                self.chunk.emit(Op::BuildList(elements.len() as u16));
                Ok(())
            }
            Expr::Unary { op, expr, .. } => {
                self.compile_expr(expr)?;
                match op {
                    UnOp::Neg => {
                        self.chunk.emit(Op::Neg);
                    }
                    UnOp::Not => {
                        self.chunk.emit(Op::Not);
                    }
                    UnOp::Ref | UnOp::RefMut => {}
                }
                Ok(())
            }
            Expr::Binary {
                op, left, right, ..
            } => {
                if *op == BinOp::And {
                    self.compile_expr(left)?;
                    let jump = self.chunk.emit(Op::JumpIfFalse(0));
                    self.chunk.emit(Op::Pop);
                    self.compile_expr(right)?;
                    let end = self.chunk.ops.len();
                    self.chunk.patch_jump(jump, end);
                    return Ok(());
                }
                if *op == BinOp::Or {
                    self.compile_expr(left)?;
                    let jump = self.chunk.emit(Op::JumpIfFalse(0));
                    let skip = self.chunk.emit(Op::Jump(0));
                    let false_target = self.chunk.ops.len();
                    self.chunk.patch_jump(jump, false_target);
                    self.chunk.emit(Op::Pop);
                    self.compile_expr(right)?;
                    let end = self.chunk.ops.len();
                    self.chunk.patch_jump(skip, end);
                    return Ok(());
                }
                self.compile_expr(left)?;
                self.compile_expr(right)?;
                let op = match op {
                    BinOp::Add => Op::Add,
                    BinOp::Sub => Op::Sub,
                    BinOp::Mul => Op::Mul,
                    BinOp::Div => Op::Div,
                    BinOp::Rem => Op::Rem,
                    BinOp::Eq => Op::Eq,
                    BinOp::Ne => Op::Ne,
                    BinOp::Lt => Op::Lt,
                    BinOp::Le => Op::Le,
                    BinOp::Gt => Op::Gt,
                    BinOp::Ge => Op::Ge,
                    BinOp::And | BinOp::Or => unreachable!(),
                };
                self.chunk.emit(op);
                Ok(())
            }
            Expr::Assign { op, target, value, span } => {
                if *op != AssignOp::Assign {
                    return Err(VmError::new(*span, "compound assignment is not yet compiled"));
                }
                self.compile_expr(value)?;
                match target.as_ref() {
                    Expr::Ident(id) => {
                        if let Some(slot) = self.resolve_local(&id.name) {
                            self.chunk.emit(Op::SetLocal(slot));
                        } else {
                            let c = self.chunk.add_constant(Value::from_string(id.name.clone()));
                            self.chunk.emit(Op::DefineGlobal(c));
                        }
                        Ok(())
                    }
                    Expr::Index { target, index, .. } => {
                        self.compile_expr(target)?;
                        self.compile_expr(index)?;
                        self.chunk.emit(Op::SetIndex);
                        Ok(())
                    }
                    _ => Err(VmError::new(*span, "invalid assignment target")),
                }
            }
            Expr::Call { callee, args, .. } => {
                self.compile_expr(callee)?;
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.chunk.emit(Op::Call(args.len() as u8));
                Ok(())
            }
            Expr::Index { target, index, .. } => {
                self.compile_expr(target)?;
                self.compile_expr(index)?;
                self.chunk.emit(Op::GetIndex);
                Ok(())
            }
            Expr::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.compile_expr(cond)?;
                let else_jump = self.chunk.emit(Op::JumpIfFalse(0));
                self.chunk.emit(Op::Pop);
                self.compile_block_value(then_block)?;
                let end_jump = self.chunk.emit(Op::Jump(0));
                let else_target = self.chunk.ops.len();
                self.chunk.patch_jump(else_jump, else_target);
                self.chunk.emit(Op::Pop);
                if let Some(els) = else_block {
                    self.compile_expr(els)?;
                } else {
                    self.chunk.emit(Op::Nil);
                }
                let end = self.chunk.ops.len();
                self.chunk.patch_jump(end_jump, end);
                Ok(())
            }
            Expr::Block(b) => self.compile_block_value(b),
            Expr::Field { span, .. } => Err(VmError::new(
                *span,
                "field access is not yet compiled to bytecode",
            )),
            Expr::Range { span, .. } => Err(VmError::new(
                *span,
                "ranges are not yet compiled to bytecode",
            )),
            Expr::Map { span, .. } => Err(VmError::new(*span, "maps are not yet compiled to bytecode")),
            Expr::StructInit { span, .. } => Err(VmError::new(
                *span,
                "struct literals are not yet compiled to bytecode",
            )),
        }
    }
}

pub fn compile(program: &Program) -> Result<Compiled, VmError> {
    let mut functions = Vec::new();
    for item in &program.items {
        match item {
            Item::Fn(func) => functions.push(compile_fn(func)?),
            Item::Import(import) => {
                return Err(VmError::new(import.span, "imports are not implemented"));
            }
            Item::Struct(_) | Item::Type(_) => {}
        }
    }
    Ok(Compiled { functions })
}

fn compile_fn(func: &FnDecl) -> Result<Function, VmError> {
    let params: Vec<String> = func.params.iter().map(|p| p.name.name.clone()).collect();
    let arity = params.len() as u8;
    let mut compiler = FnCompiler::new(params);
    compiler.compile_block_value(&func.body)?;
    compiler.chunk.emit(Op::Return);
    Ok(compiler.finish(func.name.name.clone(), arity))
}
