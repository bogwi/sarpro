//! Crate-level error type and `Result` alias for stable, structured error handling.
//! Converts underlying I/O, SAFE, and GDAL errors, and provides semantic variants
//! for argument validation and processing failures.
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SAFE reader error: {0}")]
    Safe(#[from] crate::io::SafeError),

    #[error("GDAL error: {0}")]
    Gdal(#[from] crate::io::GdalError),

    #[error("Invalid argument: {arg}={value}")]
    InvalidArgument { arg: &'static str, value: String },

    #[error("Size must be greater than 0, got: {size}")]
    ZeroSize { size: usize },

    #[error("Missing required argument: {arg}")]
    MissingArgument { arg: String },

    #[error("No complete polarization data available for operation: {operation}. Available: {available}")]
    IncompleteDataPair { operation: String, available: String },

    #[error("Processing error: {0}")]
    Processing(String),

    #[error("External error: {0}")]
    External(String),
}

impl Error {
    pub fn external<E: std::fmt::Display>(e: E) -> Self {
        Error::External(e.to_string())
    }
}


