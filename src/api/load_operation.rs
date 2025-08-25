//! Compute operations over polarizations and return intensity array with metadata

use std::path::Path;
use ndarray::Array2;
use crate::error::{Error, Result};
use crate::io::sentinel1::{SafeMetadata, SafeReader};
use crate::types::PolarizationOperation;

fn operation_to_str(op: PolarizationOperation) -> &'static str {
    match op {
        PolarizationOperation::Sum => "sum",
        PolarizationOperation::Diff => "difference",
        PolarizationOperation::Ratio => "ratio",
        PolarizationOperation::NDiff => "normalized_diff",
        PolarizationOperation::LogRatio => "log_ratio",
    }
}

/// Compute an operation (sum/diff/ratio/...) over an available pair and return intensity array + metadata
pub fn load_operation(
    input: &Path,
    op: PolarizationOperation,
) -> Result<(Array2<f32>, SafeMetadata)> {
    let reader = SafeReader::open_with_options(input, crate::types::Polarization::Multiband, None, None, None)?;

    // Prefer VV/VH if both available, otherwise HH/HV
    if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
        let data = match op {
            PolarizationOperation::Sum => reader.sum_data()?,
            PolarizationOperation::Diff => reader.difference_data()?,
            PolarizationOperation::Ratio => reader.ratio_data()?,
            PolarizationOperation::NDiff => reader.normalized_diff_data()?,
            PolarizationOperation::LogRatio => reader.log_ratio_data()?,
        };
        Ok((data, reader.metadata.clone()))
    } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
        let data = match op {
            PolarizationOperation::Sum => reader.sum_hh_hv_data()?,
            PolarizationOperation::Diff => reader.difference_hh_hv_data()?,
            PolarizationOperation::Ratio => reader.ratio_hh_hv_data()?,
            PolarizationOperation::NDiff => reader.normalized_diff_hh_hv_data()?,
            PolarizationOperation::LogRatio => reader.log_ratio_hh_hv_data()?,
        };
        Ok((data, reader.metadata.clone()))
    } else {
        Err(Error::Processing(format!(
            "Operation {} requires VV+VH or HH+HV; available: {}",
            operation_to_str(op),
            reader.get_available_polarizations()
        )))
    }
}
