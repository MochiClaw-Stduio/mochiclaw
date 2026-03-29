//! Configuration error types

/// Error type for configuration operations
#[derive(Debug)]
pub enum ConfigError {
    Io(String),
    Parse(String),
    Serialization(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(s) => write!(f, "config IO error: {}", s),
            ConfigError::Parse(s) => write!(f, "config parse error: {}", s),
            ConfigError::Serialization(s) => write!(f, "config serialization error: {}", s),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e.to_string())
    }
}
