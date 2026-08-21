//! Built-in functions shared with the tree-walking interpreter.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::io::Write;
use std::rc::Rc;

use flake_ast::Span;

use crate::error::VmError;
use crate::value::{Native, Value};

pub fn call_native(
    native: Native,
    args: &[Value],
    stdout: &mut dyn Write,
) -> Result<Value, VmError> {
    let span = Span::DUMMY;
    match native {
        Native::Print => {
            let text: Vec<_> = args.iter().map(Value::display_value).collect();
            writeln!(stdout, "{}", text.join(" "))
                .map_err(|e| VmError::new(span, format!("write failed: {e}")))?;
            Ok(Value::Nil)
        }
        Native::Len => {
            expect_arity("len", args, 1)?;
            match &args[0] {
                Value::List(v) => Ok(Value::Int(v.borrow().len() as i64)),
                Value::String(s) => Ok(Value::Int(s.chars().count() as i64)),
                Value::Map(m) => Ok(Value::Int(m.borrow().len() as i64)),
                other => Err(VmError::new(
                    span,
                    format!(
                        "len() expected List, String, or Map, found {}",
                        other.type_name()
                    ),
                )),
            }
        }
        Native::Push => {
            expect_arity("push", args, 2)?;
            match &args[0] {
                Value::List(v) => {
                    v.borrow_mut().push(args[1].clone());
                    Ok(Value::Nil)
                }
                other => Err(VmError::new(
                    span,
                    format!("push() expected List, found {}", other.type_name()),
                )),
            }
        }
        Native::Pop => {
            expect_arity("pop", args, 1)?;
            match &args[0] {
                Value::List(v) => Ok(v.borrow_mut().pop().unwrap_or(Value::Nil)),
                other => Err(VmError::new(
                    span,
                    format!("pop() expected List, found {}", other.type_name()),
                )),
            }
        }
        Native::Str => {
            expect_arity("str", args, 1)?;
            Ok(Value::from_string(args[0].display_value()))
        }
        Native::Int => {
            expect_arity("int", args, 1)?;
            to_int(&args[0])
        }
        Native::Float => {
            expect_arity("float", args, 1)?;
            to_float(&args[0])
        }
        Native::TypeOf => {
            expect_arity("type_of", args, 1)?;
            Ok(Value::from_string(args[0].type_name()))
        }
        Native::Assert => {
            if args.is_empty() || args.len() > 2 {
                return Err(VmError::new(span, "assert() expected 1 or 2 arguments"));
            }
            if !args[0].as_bool().map_err(|m| VmError::new(span, m))? {
                let msg = if args.len() == 2 {
                    args[1].display_value()
                } else {
                    "assertion failed".into()
                };
                return Err(VmError::new(span, msg));
            }
            Ok(Value::Nil)
        }
        Native::ReadFile => {
            expect_arity("read_file", args, 1)?;
            let path = match &args[0] {
                Value::String(s) => s.to_string(),
                other => {
                    return Err(VmError::new(
                        span,
                        format!("read_file() expected String, found {}", other.type_name()),
                    ));
                }
            };
            match std::fs::read_to_string(&path) {
                Ok(text) => Ok(Value::from_string(text)),
                Err(e) => Err(VmError::new(span, format!("failed to read `{path}`: {e}"))),
            }
        }
        Native::Abs => {
            expect_arity("abs", args, 1)?;
            match &args[0] {
                Value::Int(n) => n
                    .checked_abs()
                    .map(Value::Int)
                    .ok_or_else(|| VmError::new(span, "integer overflow")),
                Value::Float(n) => Ok(Value::Float(n.abs())),
                other => Err(VmError::new(
                    span,
                    format!("abs() expected Int or Float, found {}", other.type_name()),
                )),
            }
        }
        Native::Min | Native::Max => {
            if args.len() < 2 {
                return Err(VmError::new(
                    span,
                    format!("{}() expected at least 2 arguments", native.name()),
                ));
            }
            let mut acc = args[0].clone();
            for arg in &args[1..] {
                let lt = cmp_lt(&acc, arg)?;
                match native {
                    Native::Min if !lt => acc = arg.clone(),
                    Native::Max if lt => acc = arg.clone(),
                    _ => {}
                }
            }
            Ok(acc)
        }
        Native::Range => {
            let (start, end) = match args.len() {
                1 => (0, expect_int(&args[0])?),
                2 => (expect_int(&args[0])?, expect_int(&args[1])?),
                _ => return Err(VmError::new(span, "range() expected 1 or 2 arguments")),
            };
            Ok(Value::Range { start, end })
        }
        Native::Join => {
            expect_arity("join", args, 2)?;
            let sep = match &args[1] {
                Value::String(s) => s.to_string(),
                other => {
                    return Err(VmError::new(
                        span,
                        format!(
                            "join() separator expected String, found {}",
                            other.type_name()
                        ),
                    ));
                }
            };
            match &args[0] {
                Value::List(items) => {
                    let parts: Vec<_> = items.borrow().iter().map(Value::display_value).collect();
                    Ok(Value::from_string(parts.join(&sep)))
                }
                other => Err(VmError::new(
                    span,
                    format!("join() expected List, found {}", other.type_name()),
                )),
            }
        }
        Native::Split => {
            expect_arity("split", args, 2)?;
            let s = match &args[0] {
                Value::String(s) => s.to_string(),
                other => {
                    return Err(VmError::new(
                        span,
                        format!("split() expected String, found {}", other.type_name()),
                    ));
                }
            };
            let sep = match &args[1] {
                Value::String(s) => s.to_string(),
                other => {
                    return Err(VmError::new(
                        span,
                        format!(
                            "split() separator expected String, found {}",
                            other.type_name()
                        ),
                    ));
                }
            };
            let parts: Vec<_> = if sep.is_empty() {
                s.chars()
                    .map(|c| Value::from_string(c.to_string()))
                    .collect()
            } else {
                s.split(&sep).map(Value::from_string).collect()
            };
            Ok(Value::List(Rc::new(RefCell::new(parts))))
        }
        Native::WriteFile => {
            expect_arity("write_file", args, 2)?;
            let path = match &args[0] {
                Value::String(s) => s.to_string(),
                other => {
                    return Err(VmError::new(
                        span,
                        format!(
                            "write_file() expected String path, found {}",
                            other.type_name()
                        ),
                    ));
                }
            };
            let text = args[1].display_value();
            std::fs::write(&path, text)
                .map_err(|e| VmError::new(span, format!("failed to write `{path}`: {e}")))?;
            Ok(Value::Nil)
        }
        Native::Contains => {
            expect_arity("contains", args, 2)?;
            match &args[0] {
                Value::String(s) => Ok(Value::Bool(s.contains(&args[1].display_value()))),
                Value::List(items) => Ok(Value::Bool(
                    items.borrow().iter().any(|v| v.equals(&args[1])),
                )),
                other => Err(VmError::new(
                    span,
                    format!(
                        "contains() expected String or List, found {}",
                        other.type_name()
                    ),
                )),
            }
        }
        Native::StartsWith => {
            expect_arity("starts_with", args, 2)?;
            match (&args[0], &args[1]) {
                (Value::String(s), Value::String(p)) => Ok(Value::Bool(s.starts_with(p.as_ref()))),
                _ => Err(VmError::new(span, "starts_with() expected two Strings")),
            }
        }
        Native::EndsWith => {
            expect_arity("ends_with", args, 2)?;
            match (&args[0], &args[1]) {
                (Value::String(s), Value::String(p)) => Ok(Value::Bool(s.ends_with(p.as_ref()))),
                _ => Err(VmError::new(span, "ends_with() expected two Strings")),
            }
        }
        Native::First => {
            expect_arity("first", args, 1)?;
            match &args[0] {
                Value::List(v) => Ok(v.borrow().first().cloned().unwrap_or(Value::Nil)),
                Value::String(s) => Ok(s
                    .chars()
                    .next()
                    .map(|c| Value::from_string(c.to_string()))
                    .unwrap_or(Value::Nil)),
                other => Err(VmError::new(
                    span,
                    format!(
                        "first() expected List or String, found {}",
                        other.type_name()
                    ),
                )),
            }
        }
        Native::Last => {
            expect_arity("last", args, 1)?;
            match &args[0] {
                Value::List(v) => Ok(v.borrow().last().cloned().unwrap_or(Value::Nil)),
                Value::String(s) => Ok(s
                    .chars()
                    .last()
                    .map(|c| Value::from_string(c.to_string()))
                    .unwrap_or(Value::Nil)),
                other => Err(VmError::new(
                    span,
                    format!(
                        "last() expected List or String, found {}",
                        other.type_name()
                    ),
                )),
            }
        }
        Native::Trim => {
            expect_arity("trim", args, 1)?;
            match &args[0] {
                Value::String(s) => Ok(Value::from_string(s.trim().to_string())),
                other => Err(VmError::new(
                    span,
                    format!("trim() expected String, found {}", other.type_name()),
                )),
            }
        }
        Native::Upper => {
            expect_arity("upper", args, 1)?;
            match &args[0] {
                Value::String(s) => Ok(Value::from_string(s.to_uppercase())),
                other => Err(VmError::new(
                    span,
                    format!("upper() expected String, found {}", other.type_name()),
                )),
            }
        }
        Native::Lower => {
            expect_arity("lower", args, 1)?;
            match &args[0] {
                Value::String(s) => Ok(Value::from_string(s.to_lowercase())),
                other => Err(VmError::new(
                    span,
                    format!("lower() expected String, found {}", other.type_name()),
                )),
            }
        }
        Native::FileExists => {
            expect_arity("file_exists", args, 1)?;
            let path = match &args[0] {
                Value::String(s) => s.to_string(),
                other => {
                    return Err(VmError::new(
                        span,
                        format!("file_exists() expected String, found {}", other.type_name()),
                    ));
                }
            };
            Ok(Value::Bool(std::path::Path::new(&path).exists()))
        }
        Native::Env => {
            expect_arity("env", args, 1)?;
            let name = match &args[0] {
                Value::String(s) => s.to_string(),
                other => {
                    return Err(VmError::new(
                        span,
                        format!("env() expected String, found {}", other.type_name()),
                    ));
                }
            };
            Ok(Value::from_string(std::env::var(name).unwrap_or_default()))
        }
        Native::Cwd => {
            expect_arity("cwd", args, 0)?;
            let dir = std::env::current_dir()
                .map_err(|e| VmError::new(span, format!("cwd() failed: {e}")))?;
            Ok(Value::from_string(dir.to_string_lossy().replace('\\', "/")))
        }
        Native::RemoveFile => {
            expect_arity("remove_file", args, 1)?;
            let path = match &args[0] {
                Value::String(s) => s.to_string(),
                other => {
                    return Err(VmError::new(
                        span,
                        format!("remove_file() expected String, found {}", other.type_name()),
                    ));
                }
            };
            let _ = std::fs::remove_file(&path);
            Ok(Value::Nil)
        }
    }
}

fn expect_arity(name: &str, args: &[Value], n: usize) -> Result<(), VmError> {
    if args.len() == n {
        Ok(())
    } else {
        Err(VmError::new(
            Span::DUMMY,
            format!("{name}() expected {n} argument(s), got {}", args.len()),
        ))
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

fn to_int(value: &Value) -> Result<Value, VmError> {
    match value {
        Value::Int(n) => Ok(Value::Int(*n)),
        Value::Float(n) => Ok(Value::Int(*n as i64)),
        Value::Bool(true) => Ok(Value::Int(1)),
        Value::Bool(false) => Ok(Value::Int(0)),
        Value::String(s) => s
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| VmError::new(Span::DUMMY, format!("cannot parse `{s}` as Int"))),
        other => Err(VmError::new(
            Span::DUMMY,
            format!("cannot convert {} to Int", other.type_name()),
        )),
    }
}

fn to_float(value: &Value) -> Result<Value, VmError> {
    match value {
        Value::Float(n) => Ok(Value::Float(*n)),
        Value::Int(n) => Ok(Value::Float(*n as f64)),
        Value::String(s) => s
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| VmError::new(Span::DUMMY, format!("cannot parse `{s}` as Float"))),
        other => Err(VmError::new(
            Span::DUMMY,
            format!("cannot convert {} to Float", other.type_name()),
        )),
    }
}

fn cmp_lt(a: &Value, b: &Value) -> Result<bool, VmError> {
    let ord = match (a, b) {
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
    Ok(ord == Ordering::Less)
}
