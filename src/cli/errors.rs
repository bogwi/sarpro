use thiserror::Error;

/// Application-specific errors for the CLI
#[derive(Debug, Error)]
pub enum AppError {
    #[error("Invalid size parameter: {size}. Must be a positive integer or 'original'")]
    InvalidSize { size: String },

    #[error("Size must be greater than 0, got: {size}")]
    ZeroSize { size: usize },

    #[error(
        "No complete polarization data available for operation: {operation}. Available: {available}"
    )]
    IncompleDataPair {
        operation: String,
        available: String,
    },

    // #[error("Unsupported product type: {product}")]
    // UnsupportedProduct { product: String },
    #[error("Missing required argument: {arg}")]
    MissingArgument { arg: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SAFE reader error: {0}")]
    Safe(#[from] sarpro::io::SafeError),
}
