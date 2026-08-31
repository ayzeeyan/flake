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
    types: HashMap<String, String>,
    methods: HashMap<(String, String), Value>,
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
                types: HashMap::new(),
                methods: HashMap::new(),
            })),
        }
    }

    #[must_use]
    pub fn child(&self) -> Self {
        Self {
            inner: Rc::new(RefCell::new(EnvInner {
                parent: Some(self.clone()),
                bindings: HashMap::new(),
                types: HashMap::new(),
                methods: HashMap::new(),
            })),
        }
    }

    pub fn define(&self, name: impl Into<String>, value: Value, mutable: bool) {
        self.inner
            .borrow_mut()
            .bindings
            .insert(name.into(), Binding { value, mutable });
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        let inner = self.inner.borrow();
        if let Some(b) = inner.bindings.get(name) {
            return Some(b.value.clone());
        }
        inner.parent.as_ref().and_then(|p| p.get(name))
    }

    pub fn define_method(
        &self,
        type_name: impl Into<String>,
        method_name: impl Into<String>,
        value: Value,
    ) {
        self.inner
            .borrow_mut()
            .methods
            .insert((type_name.into(), method_name.into()), value);
    }

    pub fn get_method(&self, type_name: &str, method_name: &str) -> Option<Value> {
        let inner = self.inner.borrow();
        if let Some(v) = inner
            .methods
            .get(&(type_name.to_string(), method_name.to_string()))
        {
            return Some(v.clone());
        }
        let short = type_name.rsplit('.').next().unwrap_or(type_name);
        if let Some(v) = inner
            .methods
            .get(&(short.to_string(), method_name.to_string()))
        {
            return Some(v.clone());
        }
        inner
            .parent
            .as_ref()
            .and_then(|p| p.get_method(type_name, method_name))
    }

    pub fn define_type(&self, name: impl Into<String>, canonical: impl Into<String>) {
        self.inner
            .borrow_mut()
            .types
            .insert(name.into(), canonical.into());
    }

    pub fn resolve_type(&self, name: &str) -> Option<String> {
        let inner = self.inner.borrow();
        if let Some(canonical) = inner.types.get(name) {
            return Some(canonical.clone());
        }
        inner
            .parent
            .as_ref()
            .and_then(|parent| parent.resolve_type(name))
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
