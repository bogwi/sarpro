//! Typed save helper for multiband arrays

use std::path::Path;
use ndarray::Array2;
use crate::core::processing::save::save_processed_multiband_image_sequential;
use crate::error::{Error, Result};
use crate::io::sentinel1::SafeMetadata;
use crate::types::{AutoscaleStrategy, BitDepth, OutputFormat, ProcessingOperation, SyntheticRgbMode};

/// Typed save helper for multiband arrays (VV/VH or HH/HV)
pub fn save_multiband_image(
    processed1: &Array2<f32>,
    processed2: &Array2<f32>,
    output: &Path,
    format: OutputFormat,
    bit_depth: BitDepth,
    target_size: Option<usize>,
    metadata: Option<&SafeMetadata>,
    pad: bool,
    autoscale: AutoscaleStrategy,
    operation: ProcessingOperation,
) -> Result<()> {
    // We omit the specific multiband label for now; metadata will still include polarizations
    save_processed_multiband_image_sequential(
        processed1,
        processed2,
        output,
        format,
        bit_depth,
        target_size,
        metadata,
        pad,
        autoscale,
        operation,
        SyntheticRgbMode::Default,
    )
    .map_err(|e| Error::external(e))
}
