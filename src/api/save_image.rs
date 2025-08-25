//! Typed save helper for single-band arrays

use std::path::Path;
use ndarray::Array2;
use crate::core::processing::save::save_processed_image;
use crate::error::{Error, Result};
use crate::io::sentinel1::SafeMetadata;
use crate::types::{AutoscaleStrategy, BitDepth, OutputFormat, ProcessingOperation};

/// Typed save helper for single-band arrays
pub fn save_image(
    processed: &Array2<f32>,
    output: &Path,
    format: OutputFormat,
    bit_depth: BitDepth,
    target_size: Option<usize>,
    metadata: Option<&SafeMetadata>,
    pad: bool,
    autoscale: AutoscaleStrategy,
    operation: ProcessingOperation,
) -> Result<()> {
    save_processed_image(
        processed,
        output,
        format,
        bit_depth,
        target_size,
        metadata,
        pad,
        autoscale,
        operation,
    )
    .map_err(|e| Error::external(e))
}
