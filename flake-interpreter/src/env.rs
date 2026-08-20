//! Nested lexical environments.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use flake_ast::Span;

use crate::error::RuntimeError;
use crate::value::Value;

#[derive(Clone)]
pub struct Env {
    inner: Rc<RefCell<EnvInner>>,
}

struct EnvInner {
    parent: Option<Env>,
    bindings: HashMap<String, Binding>,
}

struct Binding {
    value: Value,
    mutable: bool,
}

impl Env {
    #[must_use]
    pub fn root() -> Self {
        Self {
            inner: Rc::new(RefCell::new(EnvInner {
                parent: None,
                bindings: HashMap::new(),
            })),
        }
    }

    #[must_use]
    pub fn child(&self) -> Self {
        Self {
            inner: Rc::new(RefCell::new(EnvInner {
                parent: Some(self.clone()),
                bindings: HashMap::new(),
            })),
        }
    }

    pub fn define(&self, name: impl Into<String>, value: Value, mutable: bool) {
        self.inner.borrow_mut().bindings.insert(
            name.into(),
            Binding { value, mutable },
        );
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        let inner = self.inner.borrow();
        if let Some(b) = inner.bindings.get(name) {
            return Some(b.value.clone());
        }
        inner.parent.as_ref().and_then(|p| p.get(name))
    }

    pub fn assign(&self, name: &str, value: Value, span: Span) -> Result<(), RuntimeError> {
        let mut current = self.clone();
        loop {
            {
                let mut inner = current.inner.borrow_mut();
                if let Some(binding) = inner.bindings.get_mut(name) {
                    if !binding.mutable {
                        return Err(RuntimeError::new(
                            span,
                            format!("cannot assign to immutable binding `{name}`"),
                        ));
                    }
                    binding.value = value;
                    return Ok(());
                }
                if inner.parent.is_none() {
                    break;
                }
            }
            let parent = current.inner.borrow().parent.clone();
            match parent {
                Some(p) => current = p,
                None => break,
            }
        }
        Err(RuntimeError::new(
            span,
            format!("undefined variable `{name}`"),
        ))
    }
}
