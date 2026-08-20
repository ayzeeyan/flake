//! Stack-based bytecode interpreter.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

use flake_ast::Span;

use crate::error::VmError;
use crate::opcode::Op;
use crate::value::{Function, Native, Value};

struct Frame {
    func: Rc<Function>,
    ip: usize,
    slots: usize,
}

pub struct Vm<'io> {
    stack: Vec<Value>,
    frames: Vec<Frame>,
    globals: HashMap<String, Value>,
    stdout: &'io mut dyn Write,
}

impl<'io> Vm<'io> {
    pub fn new(stdout: &'io mut dyn Write) -> Self {
        let mut globals = HashMap::new();
        globals.insert("print".into(), Value::Native(Native::Print));
        Self {
            stack: Vec::new(),
            frames: Vec::new(),
            globals,
            stdout,
        }
    }

    pub fn define_function(&mut self, func: Function) {
        let name = func.name.clone();
        self.globals
            .insert(name, Value::Function(Rc::new(func)));
    }

    pub fn run_main(&mut self) -> Result<Value, VmError> {
        let Some(Value::Function(main)) = self.globals.get("main").cloned() else {
            return Err(VmError::new(Span::DUMMY, "program has no `main` function"));
        };
        if main.arity != 0 {
            return Err(VmError::new(Span::DUMMY, "`main` cannot take parameters"));
        }
        self.stack.push(Value::Function(main.clone()));
        self.call(main, 0, Span::DUMMY)?;
        self.run()
    }

    fn run(&mut self) -> Result<Value, VmError> {
        while let Some(frame_index) = self.frames.len().checked_sub(1) {
            let ip = self.frames[frame_index].ip;
            let op = self.frames[frame_index].func.chunk.ops.get(ip).cloned();
            self.frames[frame_index].ip += 1;
            let Some(op) = op else {
                return Err(VmError::new(Span::DUMMY, "unexpected end of bytecode"));
            };
            match op {
                Op::Constant(i) => {
                    let v = self.frames[frame_index].func.chunk.constants[i as usize].clone();
                    self.stack.push(v);
                }
                Op::Nil => self.stack.push(Value::Nil),
                Op::True => self.stack.push(Value::Bool(true)),
                Op::False => self.stack.push(Value::Bool(false)),
                Op::Pop => {
                    self.stack.pop();
                }
                Op::Add => self.bin_add()?,
                Op::Sub => self.bin_num(|a, b| a.checked_sub(b), |a, b| a - b)?,
                Op::Mul => self.bin_num(|a, b| a.checked_mul(b), |a, b| a * b)?,
                Op::Div => self.bin_div()?,
                Op::Rem => self.bin_rem()?,
                Op::Eq => {
                    let b = self.pop();
                    let a = self.pop();
                    self.stack.push(Value::Bool(a.equals(&b)));
                }
                Op::Ne => {
                    let b = self.pop();
                    let a = self.pop();
                    self.stack.push(Value::Bool(!a.equals(&b)));
                }
                Op::Lt => self.bin_cmp(|o| o.is_lt())?,
                Op::Le => self.bin_cmp(|o| o.is_le())?,
                Op::Gt => self.bin_cmp(|o| o.is_gt())?,
                Op::Ge => self.bin_cmp(|o| o.is_ge())?,
                Op::Not => {
                    let v = self.pop();
                    self.stack.push(Value::Bool(!v.as_bool().map_err(|m| {
                        VmError::new(Span::DUMMY, m)
                    })?));
                }
                Op::Neg => match self.pop() {
                    Value::Int(n) => self.stack.push(Value::Int(
                        n.checked_neg()
                            .ok_or_else(|| VmError::new(Span::DUMMY, "integer overflow"))?,
                    )),
                    Value::Float(n) => self.stack.push(Value::Float(-n)),
                    other => {
                        return Err(VmError::new(
                            Span::DUMMY,
                            format!("cannot negate {}", other.type_name()),
                        ));
                    }
                },
                Op::Jump(target) => {
                    self.frames[frame_index].ip = target as usize;
                }
                Op::JumpIfFalse(target) => {
                    let cond = self.peek();
                    if !cond.as_bool().map_err(|m| VmError::new(Span::DUMMY, m))? {
                        self.frames[frame_index].ip = target as usize;
                    }
                }
                Op::GetLocal(slot) => {
                    let idx = self.frames[frame_index].slots + slot as usize;
                    while self.stack.len() <= idx {
                        self.stack.push(Value::Nil);
                    }
                    let v = self.stack[idx].clone();
                    self.stack.push(v);
                }
                Op::SetLocal(slot) => {
                    let idx = self.frames[frame_index].slots + slot as usize;
                    while self.stack.len() <= idx {
                        self.stack.push(Value::Nil);
                    }
                    let v = self.peek().clone();
                    self.stack[idx] = v;
                }
                Op::GetGlobal(i) => {
                    let name = match &self.frames[frame_index].func.chunk.constants[i as usize] {
                        Value::String(s) => s.to_string(),
                        _ => return Err(VmError::new(Span::DUMMY, "invalid global name")),
                    };
                    let v = self.globals.get(&name).cloned().ok_or_else(|| {
                        VmError::new(Span::DUMMY, format!("undefined variable `{name}`"))
                    })?;
                    self.stack.push(v);
                }
                Op::DefineGlobal(i) => {
                    let name = match &self.frames[frame_index].func.chunk.constants[i as usize] {
                        Value::String(s) => s.to_string(),
                        _ => return Err(VmError::new(Span::DUMMY, "invalid global name")),
                    };
                    let v = self.peek().clone();
                    self.globals.insert(name, v);
                }
                Op::Call(argc) => {
                    let argc = argc as usize;
                    let callee_idx = self.stack.len() - argc - 1;
                    let callee = self.stack[callee_idx].clone();
                    match callee {
                        Value::Function(f) => self.call(f, argc, Span::DUMMY)?,
                        Value::Native(Native::Print) => {
                            let mut parts = Vec::new();
                            for _ in 0..argc {
                                parts.push(self.pop());
                            }
                            parts.reverse();
                            let _ = self.pop(); // callee
                            let text: Vec<_> = parts.iter().map(Value::display_value).collect();
                            writeln!(self.stdout, "{}", text.join(" ")).map_err(|e| {
                                VmError::new(Span::DUMMY, format!("write failed: {e}"))
                            })?;
                            self.stack.push(Value::Nil);
                        }
                        other => {
                            return Err(VmError::new(
                                Span::DUMMY,
                                format!("cannot call {}", other.type_name()),
                            ));
                        }
                    }
                }
                Op::Return => {
                    let result = self.pop();
                    let frame = self.frames.pop().unwrap();
                    let callee_idx = frame.slots.saturating_sub(1);
                    self.stack.truncate(callee_idx);
                    self.stack.push(result);
                    if self.frames.is_empty() {
                        return Ok(self.pop());
                    }
                }
                Op::Print => {
                    let v = self.pop();
                    writeln!(self.stdout, "{}", v.display_value())
                        .map_err(|e| VmError::new(Span::DUMMY, format!("write failed: {e}")))?;
                    self.stack.push(Value::Nil);
                }
                Op::BuildList(n) => {
                    let mut items = Vec::with_capacity(n as usize);
                    for _ in 0..n {
                        items.push(self.pop());
                    }
                    items.reverse();
                    self.stack
                        .push(Value::List(Rc::new(RefCell::new(items))));
                }
                Op::GetIndex => {
                    let index = self.pop();
                    let target = self.pop();
                    let value = index_get(&target, &index)?;
                    self.stack.push(value);
                }
                Op::SetIndex => {
                    let index = self.pop();
                    let target = self.pop();
                    let value = self.peek().clone();
                    index_set(&target, &index, value)?;
                }
                Op::Concat(n) => {
                    let mut parts = Vec::new();
                    for _ in 0..n {
                        parts.push(self.pop());
                    }
                    parts.reverse();
                    let s: String = parts.iter().map(Value::display_value).collect();
                    self.stack.push(Value::from_string(s));
                }
            }
        }
        Ok(self.stack.pop().unwrap_or(Value::Nil))
    }

    fn call(&mut self, func: Rc<Function>, argc: usize, span: Span) -> Result<(), VmError> {
        if argc != func.arity as usize {
            return Err(VmError::new(
                span,
                format!(
                    "function `{}` expected {} argument(s), got {argc}",
                    func.name, func.arity
                ),
            ));
        }
        let callee_idx = self.stack.len() - argc - 1;
        let slots = callee_idx + 1;
        let needed = slots + func.locals as usize;
        while self.stack.len() < needed {
            self.stack.push(Value::Nil);
        }
        self.frames.push(Frame {
            func,
            ip: 0,
            slots,
        });
        Ok(())
    }

    fn pop(&mut self) -> Value {
        self.stack.pop().unwrap_or(Value::Nil)
    }

    fn peek(&self) -> &Value {
        self.stack.last().unwrap_or(&Value::Nil)
    }

    fn bin_add(&mut self) -> Result<(), VmError> {
        let b = self.pop();
        let a = self.pop();
        let v = match (a, b) {
            (Value::Int(x), Value::Int(y)) => Value::Int(
                x.checked_add(y)
                    .ok_or_else(|| VmError::new(Span::DUMMY, "integer overflow"))?,
            ),
            (Value::Float(x), Value::Float(y)) => Value::Float(x + y),
            (Value::Int(x), Value::Float(y)) => Value::Float(x as f64 + y),
            (Value::Float(x), Value::Int(y)) => Value::Float(x + y as f64),
            (Value::String(x), y) => Value::from_string(format!("{x}{}", y.display_value())),
            (x, Value::String(y)) => Value::from_string(format!("{}{y}", x.display_value())),
            (l, r) => {
                return Err(VmError::new(
                    Span::DUMMY,
                    format!("cannot add {} and {}", l.type_name(), r.type_name()),
                ));
            }
        };
        self.stack.push(v);
        Ok(())
    }

    fn bin_num(
        &mut self,
        ints: fn(i64, i64) -> Option<i64>,
        floats: fn(f64, f64) -> f64,
    ) -> Result<(), VmError> {
        let b = self.pop();
        let a = self.pop();
        let v = match (a, b) {
            (Value::Int(x), Value::Int(y)) => Value::Int(
                ints(x, y).ok_or_else(|| VmError::new(Span::DUMMY, "integer overflow"))?,
            ),
            (Value::Float(x), Value::Float(y)) => Value::Float(floats(x, y)),
            (Value::Int(x), Value::Float(y)) => Value::Float(floats(x as f64, y)),
            (Value::Float(x), Value::Int(y)) => Value::Float(floats(x, y as f64)),
            (l, r) => {
                return Err(VmError::new(
                    Span::DUMMY,
                    format!("cannot apply arithmetic to {} and {}", l.type_name(), r.type_name()),
                ));
            }
        };
        self.stack.push(v);
        Ok(())
    }

    fn bin_div(&mut self) -> Result<(), VmError> {
        let b = self.pop();
        let a = self.pop();
        let v = match (a, b) {
            (Value::Int(_), Value::Int(0)) => {
                return Err(VmError::new(Span::DUMMY, "division by zero"));
            }
            (Value::Int(x), Value::Int(y)) => Value::Int(
                x.checked_div(y)
                    .ok_or_else(|| VmError::new(Span::DUMMY, "integer overflow"))?,
            ),
            (Value::Float(x), Value::Float(y)) => Value::Float(x / y),
            (Value::Int(x), Value::Float(y)) => Value::Float(x as f64 / y),
            (Value::Float(x), Value::Int(y)) => Value::Float(x / y as f64),
            (l, r) => {
                return Err(VmError::new(
                    Span::DUMMY,
                    format!("cannot divide {} by {}", l.type_name(), r.type_name()),
                ));
            }
        };
        self.stack.push(v);
        Ok(())
    }

    fn bin_rem(&mut self) -> Result<(), VmError> {
        let b = self.pop();
        let a = self.pop();
        let v = match (a, b) {
            (Value::Int(_), Value::Int(0)) => {
                return Err(VmError::new(Span::DUMMY, "division by zero"));
            }
            (Value::Int(x), Value::Int(y)) => Value::Int(
                x.checked_rem(y)
                    .ok_or_else(|| VmError::new(Span::DUMMY, "integer overflow"))?,
            ),
            (l, r) => {
                return Err(VmError::new(
                    Span::DUMMY,
                    format!("cannot compute remainder of {} and {}", l.type_name(), r.type_name()),
                ));
            }
        };
        self.stack.push(v);
        Ok(())
    }

    fn bin_cmp(&mut self, pred: fn(std::cmp::Ordering) -> bool) -> Result<(), VmError> {
        let b = self.pop();
        let a = self.pop();
        let ord = match (&a, &b) {
            (Value::Int(x), Value::Int(y)) => x.cmp(y),
            (Value::Float(x), Value::Float(y)) => x.partial_cmp(y).ok_or_else(|| {
                VmError::new(Span::DUMMY, "cannot compare NaN")
            })?,
            (Value::String(x), Value::String(y)) => x.cmp(y),
            _ => {
                return Err(VmError::new(
                    Span::DUMMY,
                    format!("cannot compare {} and {}", a.type_name(), b.type_name()),
                ));
            }
        };
        self.stack.push(Value::Bool(pred(ord)));
        Ok(())
    }
}

fn index_get(target: &Value, index: &Value) -> Result<Value, VmError> {
    match target {
        Value::List(items) => {
            let i = match index {
                Value::Int(n) => *n,
                _ => {
                    return Err(VmError::new(Span::DUMMY, "list index must be Int"));
                }
            };
            let items = items.borrow();
            let idx = if i < 0 { items.len() as i64 + i } else { i };
            items.get(idx as usize).cloned().ok_or_else(|| {
                VmError::new(Span::DUMMY, format!("index {i} out of bounds"))
            })
        }
        other => Err(VmError::new(
            Span::DUMMY,
            format!("cannot index {}", other.type_name()),
        )),
    }
}

fn index_set(target: &Value, index: &Value, value: Value) -> Result<(), VmError> {
    match target {
        Value::List(items) => {
            let i = match index {
                Value::Int(n) => *n,
                _ => {
                    return Err(VmError::new(Span::DUMMY, "list index must be Int"));
                }
            };
            let mut items = items.borrow_mut();
            let idx = if i < 0 { items.len() as i64 + i } else { i };
            let idx = idx as usize;
            if idx >= items.len() {
                return Err(VmError::new(Span::DUMMY, format!("index {i} out of bounds")));
            }
            items[idx] = value;
            Ok(())
        }
        other => Err(VmError::new(
            Span::DUMMY,
            format!("cannot index-assign {}", other.type_name()),
        )),
    }
}
