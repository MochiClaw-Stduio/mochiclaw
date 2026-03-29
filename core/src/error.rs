//! Core error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("agent error: {0}")]
    Agent(String),

    #[error("bus error: {0}")]
    Bus(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("http error: {0}")]
    Http(String),

    #[error("lambda error: {0}")]
    Lambda(String),

    #[error("provider error: {0}")]
    Provider(String),

    #[error("session error: {0}")]
    Session(String),
}
