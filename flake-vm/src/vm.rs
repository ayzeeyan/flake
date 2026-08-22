//! Stack-based bytecode interpreter.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

use flake_ast::Span;

use crate::error::VmError;
use crate::natives::call_native;
use crate::opcode::Op;
use crate::value::{Function, Iter, IterKind, MapKey, Native, TaskRef, TaskState, Value};

const MAX_CALL_DEPTH: usize = 10_000;

struct Frame {
    func: Rc<Function>,
    ip: usize,
    slots: usize,
    tasks: Vec<TaskRef>,
}

pub struct Vm<'io> {
    stack: Vec<Value>,
    frames: Vec<Frame>,
    globals: HashMap<String, Value>,
    stdout: &'io mut dyn Write,
    current_span: Span,
}

impl<'io> Vm<'io> {
    pub fn new(stdout: &'io mut dyn Write) -> Self {
        let mut globals = HashMap::new();
        for native in Native::all() {
            globals.insert(native.name().into(), Value::Native(native));
        }
        Self {
            stack: Vec::new(),
            frames: Vec::new(),
            globals,
            stdout,
            current_span: Span::DUMMY,
        }
    }

    pub fn define_function(&mut self, func: Function) {
        let name = func.name.clone();
        self.globals.insert(name, Value::Function(Rc::new(func)));
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
        self.run_inner()
            .map_err(|error| error.with_fallback_span(self.current_span))
    }

    fn run_inner(&mut self) -> Result<Value, VmError> {
        while let Some(frame_index) = self.frames.len().checked_sub(1) {
            let ip = self.frames[frame_index].ip;
            self.current_span = self.frames[frame_index]
                .func
                .chunk
                .spans
                .get(ip)
                .copied()
                .unwrap_or(Span::DUMMY);
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
                Op::Dup => {
                    let v = self.peek().clone();
                    self.stack.push(v);
                }
                Op::Swap => {
                    let len = self.stack.len();
                    if len < 2 {
                        return Err(VmError::new(Span::DUMMY, "stack underflow"));
                    }
                    self.stack.swap(len - 1, len - 2);
                }
                Op::DupTwo => {
                    let len = self.stack.len();
                    if len < 2 {
                        return Err(VmError::new(Span::DUMMY, "stack underflow"));
                    }
                    let a = self.stack[len - 2].clone();
                    let b = self.stack[len - 1].clone();
                    self.stack.push(a);
                    self.stack.push(b);
                }
                Op::Rot3 => {
                    let len = self.stack.len();
                    if len < 3 {
                        return Err(VmError::new(Span::DUMMY, "stack underflow"));
                    }
                    self.stack.swap(len - 1, len - 2);
                    self.stack.swap(len - 2, len - 3);
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
                    self.stack.push(Value::Bool(
                        !v.as_bool().map_err(|m| VmError::new(Span::DUMMY, m))?,
                    ));
                }
                Op::Neg => match self.pop() {
                    Value::Int(n) => self
                        .stack
                        .push(Value::Int(n.checked_neg().ok_or_else(|| {
                            VmError::new(Span::DUMMY, "integer overflow")
                        })?)),
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
                    let name = constant_name(&self.frames[frame_index].func.chunk.constants, i)?;
                    let v = self.globals.get(&name).cloned().ok_or_else(|| {
                        VmError::new(Span::DUMMY, format!("undefined variable `{name}`"))
                    })?;
                    self.stack.push(v);
                }
                Op::DefineGlobal(i) => {
                    let name = constant_name(&self.frames[frame_index].func.chunk.constants, i)?;
                    let v = self.peek().clone();
                    self.globals.insert(name, v);
                }
                Op::Call(argc) => {
                    let argc = argc as usize;
                    let callee_idx = self.stack.len() - argc - 1;
                    let callee = self.stack[callee_idx].clone();
                    match callee {
                        Value::Function(f) => self.call(f, argc, Span::DUMMY)?,
                        Value::Native(native) => {
                            let mut args = Vec::with_capacity(argc);
                            for _ in 0..argc {
                                args.push(self.pop());
                            }
                            args.reverse();
                            let _ = self.pop();
                            let result = call_native(native, &args, self.stdout)?;
                            self.stack.push(result);
                        }
                        other => {
                            return Err(VmError::new(
                                Span::DUMMY,
                                format!("cannot call {}", other.type_name()),
                            ));
                        }
                    }
                }
                Op::Spawn(argc) => {
                    let argc = argc as usize;
                    if self.stack.len() < argc + 1 {
                        return Err(VmError::new(Span::DUMMY, "stack underflow in `spawn`"));
                    }
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(self.pop());
                    }
                    args.reverse();
                    let callee = self.pop();
                    if !matches!(callee, Value::Function(_) | Value::Native(_)) {
                        return Err(VmError::new(
                            Span::DUMMY,
                            format!("cannot spawn {}", callee.type_name()),
                        ));
                    }
                    let task = Rc::new(RefCell::new(TaskState::Pending { callee, args }));
                    self.frames[frame_index].tasks.push(task.clone());
                    self.stack.push(Value::Task(task));
                }
                Op::ReadyTask => {
                    let value = self.pop();
                    let task = Rc::new(RefCell::new(TaskState::Ready(value)));
                    self.frames[frame_index].tasks.push(task.clone());
                    self.stack.push(Value::Task(task));
                }
                Op::Await => {
                    let value = self.pop();
                    let Value::Task(task) = value else {
                        return Err(VmError::new(
                            Span::DUMMY,
                            format!("cannot await {}", value.type_name()),
                        ));
                    };
                    let result = self.join_task(&task)?;
                    self.stack.push(result);
                }
                Op::Return => {
                    let result = self.pop();
                    self.finish_frame_tasks(frame_index)?;
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
                    self.stack.push(Value::List(Rc::new(RefCell::new(items))));
                }
                Op::BuildMap(n) => {
                    let mut map = HashMap::new();
                    let mut pairs = Vec::with_capacity(n as usize);
                    for _ in 0..n {
                        let v = self.pop();
                        let k = self.pop();
                        pairs.push((k, v));
                    }
                    pairs.reverse();
                    for (k, v) in pairs {
                        map.insert(map_key(&k)?, v);
                    }
                    self.stack.push(Value::Map(Rc::new(RefCell::new(map))));
                }
                Op::BuildStruct { name, fields } => {
                    let type_name =
                        constant_name(&self.frames[frame_index].func.chunk.constants, name)?;
                    let mut field_map = HashMap::new();
                    let mut values = Vec::with_capacity(fields.len());
                    for _ in 0..fields.len() {
                        values.push(self.pop());
                    }
                    values.reverse();
                    for (idx, value) in values.into_iter().enumerate() {
                        let fname = constant_name(
                            &self.frames[frame_index].func.chunk.constants,
                            fields[idx],
                        )?;
                        field_map.insert(fname, value);
                    }
                    self.stack.push(Value::Struct {
                        name: Rc::from(type_name),
                        fields: Rc::new(RefCell::new(field_map)),
                    });
                }
                Op::MakeRange => {
                    let end = expect_int(&self.pop())?;
                    let start = expect_int(&self.pop())?;
                    self.stack.push(Value::Range { start, end });
                }
                Op::MakeIter => {
                    let value = self.pop();
                    self.stack
                        .push(Value::Iter(Rc::new(RefCell::new(make_iter(value)?))));
                }
                Op::IterNext(target) => match self.peek().clone() {
                    Value::Iter(iter) => {
                        let next = iter.borrow_mut().next_value();
                        match next {
                            Some(v) => self.stack.push(v),
                            None => self.frames[frame_index].ip = target as usize,
                        }
                    }
                    other => {
                        return Err(VmError::new(
                            Span::DUMMY,
                            format!("cannot iterate over {}", other.type_name()),
                        ));
                    }
                },
                Op::GetIndex => {
                    let index = self.pop();
                    let target = self.pop();
                    self.stack.push(index_get(&target, &index)?);
                }
                Op::SetIndex => {
                    let index = self.pop();
                    let target = self.pop();
                    let value = self.peek().clone();
                    index_set(&target, &index, value)?;
                }
                Op::GetField(i) => {
                    let name = constant_name(&self.frames[frame_index].func.chunk.constants, i)?;
                    let target = self.pop();
                    self.stack.push(field_get(&target, &name)?);
                }
                Op::SetField(i) => {
                    let name = constant_name(&self.frames[frame_index].func.chunk.constants, i)?;
                    let target = self.pop();
                    let value = self.peek().clone();
                    field_set(&target, &name, value)?;
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
        if self.frames.len() >= MAX_CALL_DEPTH {
            return Err(VmError::new(span, "maximum call depth exceeded"));
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
            tasks: Vec::new(),
        });
        Ok(())
    }

    fn join_task(&mut self, task: &TaskRef) -> Result<Value, VmError> {
        enum Work {
            Ready(Value),
            Call(Value, Vec<Value>),
        }

        let work = {
            let mut state = task.borrow_mut();
            match &*state {
                TaskState::Pending { .. } | TaskState::Ready(_) => {}
                TaskState::Running => {
                    return Err(VmError::new(Span::DUMMY, "task is already running"));
                }
                TaskState::Joined => {
                    return Err(VmError::new(Span::DUMMY, "task was already awaited"));
                }
                TaskState::Cancelled => {
                    return Err(VmError::new(Span::DUMMY, "task was cancelled"));
                }
            }
            match std::mem::replace(&mut *state, TaskState::Running) {
                TaskState::Pending { callee, args } => Work::Call(callee, args),
                TaskState::Ready(value) => Work::Ready(value),
                _ => unreachable!("joinable task changed while borrowed"),
            }
        };

        let result = match work {
            Work::Ready(value) => Ok(value),
            Work::Call(Value::Function(function), args) => self.run_task_function(function, args),
            Work::Call(Value::Native(native), args) => call_native(native, &args, self.stdout),
            Work::Call(other, _) => Err(VmError::new(
                Span::DUMMY,
                format!("cannot spawn {}", other.type_name()),
            )),
        };
        *task.borrow_mut() = TaskState::Joined;
        result
    }

    fn finish_frame_tasks(&mut self, frame_index: usize) -> Result<(), VmError> {
        let tasks = self.frames[frame_index].tasks.clone();
        for (index, task) in tasks.iter().enumerate() {
            let joinable = matches!(
                &*task.borrow(),
                TaskState::Pending { .. } | TaskState::Ready(_)
            );
            if joinable {
                if let Err(error) = self.join_task(task) {
                    for remaining in &tasks[index + 1..] {
                        if matches!(
                            &*remaining.borrow(),
                            TaskState::Pending { .. } | TaskState::Ready(_)
                        ) {
                            *remaining.borrow_mut() = TaskState::Cancelled;
                        }
                    }
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    fn run_task_function(
        &mut self,
        function: Rc<Function>,
        args: Vec<Value>,
    ) -> Result<Value, VmError> {
        let saved_stack = std::mem::take(&mut self.stack);
        let saved_frames = std::mem::take(&mut self.frames);

        self.stack.push(Value::Function(function.clone()));
        self.stack.extend(args);
        let setup = self.call(function, self.stack.len().saturating_sub(1), Span::DUMMY);
        let result = match setup {
            Ok(()) => self.run(),
            Err(error) => Err(error),
        };

        self.stack = saved_stack;
        self.frames = saved_frames;
        result
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
            (Value::List(a), Value::List(b)) => {
                let mut out = a.borrow().clone();
                out.extend(b.borrow().iter().cloned());
                Value::List(Rc::new(RefCell::new(out)))
            }
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
            (Value::Int(x), Value::Int(y)) => {
                Value::Int(ints(x, y).ok_or_else(|| VmError::new(Span::DUMMY, "integer overflow"))?)
            }
            (Value::Float(x), Value::Float(y)) => Value::Float(floats(x, y)),
            (Value::Int(x), Value::Float(y)) => Value::Float(floats(x as f64, y)),
            (Value::Float(x), Value::Int(y)) => Value::Float(floats(x, y as f64)),
            (l, r) => {
                return Err(VmError::new(
                    Span::DUMMY,
                    format!(
                        "cannot apply arithmetic to {} and {}",
                        l.type_name(),
                        r.type_name()
                    ),
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
            (Value::Float(x), Value::Float(y)) => Value::Float(x % y),
            (l, r) => {
                return Err(VmError::new(
                    Span::DUMMY,
                    format!(
                        "cannot compute remainder of {} and {}",
                        l.type_name(),
                        r.type_name()
                    ),
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
            (Value::Float(x), Value::Float(y)) => x
                .partial_cmp(y)
                .ok_or_else(|| VmError::new(Span::DUMMY, "cannot compare NaN"))?,
            (Value::Int(x), Value::Float(y)) => (*x as f64)
                .partial_cmp(y)
                .ok_or_else(|| VmError::new(Span::DUMMY, "cannot compare NaN"))?,
            (Value::Float(x), Value::Int(y)) => x
                .partial_cmp(&(*y as f64))
                .ok_or_else(|| VmError::new(Span::DUMMY, "cannot compare NaN"))?,
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

fn constant_name(constants: &[Value], i: u16) -> Result<String, VmError> {
    match constants.get(i as usize) {
        Some(Value::String(s)) => Ok(s.to_string()),
        _ => Err(VmError::new(Span::DUMMY, "invalid name constant")),
    }
}

fn expect_int(value: &Value) -> Result<i64, VmError> {
    match value {
        Value::Int(n) => Ok(*n),
        other => Err(VmError::new(
            Span::DUMMY,
            format!("expected Int, found {}", other.type_name()),
        )),
    }
}

fn map_key(value: &Value) -> Result<MapKey, VmError> {
    MapKey::from_value(value).ok_or_else(|| {
        VmError::new(
            Span::DUMMY,
            format!("cannot use {} as a map key", value.type_name()),
        )
    })
}

fn make_iter(value: Value) -> Result<Iter, VmError> {
    let kind = match value {
        Value::List(items) => IterKind::List { items, idx: 0 },
        Value::Range { start, end } => IterKind::Range {
            next: start,
            end,
            rev: end < start,
        },
        Value::String(s) => IterKind::Chars {
            chars: s.chars().collect(),
            idx: 0,
        },
        Value::Iter(iter) => return Ok(iter.borrow().clone()),
        other => {
            return Err(VmError::new(
                Span::DUMMY,
                format!("cannot iterate over {}", other.type_name()),
            ));
        }
    };
    Ok(Iter { kind })
}

fn index_get(target: &Value, index: &Value) -> Result<Value, VmError> {
    match target {
        Value::List(items) => {
            let i = expect_int(index)?;
            let items = items.borrow();
            let idx = if i < 0 { items.len() as i64 + i } else { i };
            items
                .get(idx as usize)
                .cloned()
                .ok_or_else(|| VmError::new(Span::DUMMY, format!("index {i} out of bounds")))
        }
        Value::String(s) => {
            let i = expect_int(index)?;
            let chars: Vec<char> = s.chars().collect();
            let idx = if i < 0 { chars.len() as i64 + i } else { i };
            chars
                .get(idx as usize)
                .map(|c| Value::from_string(c.to_string()))
                .ok_or_else(|| VmError::new(Span::DUMMY, format!("index {i} out of bounds")))
        }
        Value::Map(map) => {
            let key = map_key(index)?;
            map.borrow()
                .get(&key)
                .cloned()
                .ok_or_else(|| VmError::new(Span::DUMMY, format!("map has no key {}", key.repr())))
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
            let i = expect_int(index)?;
            let mut items = items.borrow_mut();
            let idx = if i < 0 { items.len() as i64 + i } else { i };
            let idx = idx as usize;
            if idx >= items.len() {
                return Err(VmError::new(
                    Span::DUMMY,
                    format!("index {i} out of bounds"),
                ));
            }
            items[idx] = value;
            Ok(())
        }
        Value::Map(map) => {
            let key = map_key(index)?;
            map.borrow_mut().insert(key, value);
            Ok(())
        }
        other => Err(VmError::new(
            Span::DUMMY,
            format!("cannot index-assign {}", other.type_name()),
        )),
    }
}

fn field_get(target: &Value, name: &str) -> Result<Value, VmError> {
    match target {
        Value::Struct { fields, .. } => fields
            .borrow()
            .get(name)
            .cloned()
            .ok_or_else(|| VmError::new(Span::DUMMY, format!("no field `{name}`"))),
        other => Err(VmError::new(
            Span::DUMMY,
            format!("cannot access field `{name}` on {}", other.type_name()),
        )),
    }
}

fn field_set(target: &Value, name: &str, value: Value) -> Result<(), VmError> {
    match target {
        Value::Struct { fields, .. } => {
            fields.borrow_mut().insert(name.to_string(), value);
            Ok(())
        }
        other => Err(VmError::new(
            Span::DUMMY,
            format!("cannot assign field `{name}` on {}", other.type_name()),
        )),
    }
}
