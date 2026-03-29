//! Lambda error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("lambda error: {0}")]
    Lambda(String),

    #[error("manifest error: {0}")]
    Manifest(String),
}

impl From<crate::manifest::ManifestError> for Error {
    fn from(e: crate::manifest::ManifestError) -> Self {
        Error::Manifest(e.to_string())
    }
}
