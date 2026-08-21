//! Code generation errors.

use thiserror::Error;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct CodegenError(pub String);

impl CodegenError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}
