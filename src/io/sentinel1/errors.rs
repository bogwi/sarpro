use thiserror::Error;

/// Errors encountered when reading SAFE archives
#[derive(Debug, Error)]
pub enum SafeError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("XML parse error: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("TIFF error: {0}")]
    Tiff(#[from] tiff::TiffError),
    #[error("Missing field `{0}` in SAFE metadata")]
    MissingField(&'static str),
    #[error("Unsupported SAFE product type: {0}")]
    UnsupportedProduct(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Provided SLC measurement file is not a TIFF file: {0}")]
    NotTiff(String),
}
