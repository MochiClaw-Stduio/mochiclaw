//! Unified error types for Mochiclaw

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}

impl Error {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
        }
    }

    pub fn channel(msg: &str) -> Self {
        Self::new("CHANNEL", msg)
    }

    pub fn provider(msg: &str) -> Self {
        Self::new("PROVIDER", msg)
    }

    pub fn lambda(msg: &str) -> Self {
        Self::new("LAMBDA", msg)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}
