//! Error types for the Z language compiler and VM.

use std::fmt;

use crate::value::Value;

/// A runtime error raised by Z code via `callerror`, or by a failed operation
/// (e.g. a bad type). Catchable by `except`.
#[derive(Debug, Clone)]
pub struct RaisedError {
    pub class_name: String,
    pub message: String,
    /// Instance of a user-defined error class, bound by `except ... as e`.
    pub instance: Option<Value>,
}

impl fmt::Display for RaisedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.message.is_empty() {
            write!(f, "{}", self.class_name)
        } else {
            write!(f, "{}: {}", self.class_name, self.message)
        }
    }
}

#[derive(Debug, Clone)]
pub enum ZError {
    Lexer(String),
    Parse(String),
    Compile(String),
    ImportError(String),
    /// Internal VM failure that surfaces as an uncaught error.
    Runtime(String),
    /// A catchable error raised by Z code or a failing built-in.
    Raised(RaisedError),
}

impl ZError {
    pub fn raised(class_name: impl Into<String>, message: impl Into<String>) -> Self {
        ZError::Raised(RaisedError {
            class_name: class_name.into(),
            message: message.into(),
            instance: None,
        })
    }

    /// Raise an error carrying a user-defined error instance.
    pub fn raise_instance(
        instance: Value,
        class_name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        ZError::Raised(RaisedError {
            class_name: class_name.into(),
            message: message.into(),
            instance: Some(instance),
        })
    }

    /// A built-in error type, e.g. `ZError::builtin("TypeError", "...")`.
    pub fn builtin(class_name: &str, message: impl Into<String>) -> Self {
        ZError::raised(class_name, message)
    }

    /// Message shown for uncaught errors.
    pub fn report(&self) -> String {
        match self {
            ZError::Lexer(s) => format!("LexError: {}", s),
            ZError::Parse(s) => format!("SyntaxError: {}", s),
            ZError::Compile(s) => format!("CompileError: {}", s),
            ZError::ImportError(s) => format!("ImportError: {}", s),
            ZError::Runtime(s) => format!("RuntimeError: {}", s),
            ZError::Raised(r) => r.to_string(),
        }
    }

    pub fn error_class(&self) -> &str {
        match self {
            ZError::Raised(r) => &r.class_name,
            ZError::Lexer(_) => "LexError",
            ZError::Parse(_) => "SyntaxError",
            ZError::Compile(_) => "CompileError",
            ZError::ImportError(_) => "ImportError",
            ZError::Runtime(_) => "RuntimeError",
        }
    }
}

impl fmt::Display for ZError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.report())
    }
}

impl std::error::Error for ZError {}

pub type ZResult<T> = Result<T, ZError>;