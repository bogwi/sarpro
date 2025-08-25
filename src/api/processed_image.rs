//! ProcessedImage struct definition and related types

use crate::io::sentinel1::SafeMetadata;
use crate::types::{BitDepth, OutputFormat};

/// Result of in-memory processing
#[derive(Debug, Clone)]
pub struct ProcessedImage {
    pub width: usize,
    pub height: usize,
    pub bit_depth: BitDepth,
    pub format: OutputFormat,
    pub gray: Option<Vec<u8>>,          // single-band U8
    pub gray16: Option<Vec<u16>>,       // single-band U16
    pub rgb: Option<Vec<u8>>,           // interleaved RGB (for JPEG synthetic RGB)
    pub gray_band2: Option<Vec<u8>>,    // multiband second band U8
    pub gray16_band2: Option<Vec<u16>>, // multiband second band U16
    pub metadata: SafeMetadata,
}
