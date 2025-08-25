//! Convenience variant with explicit options for processing SAFE inputs

use std::path::Path;
use crate::core::processing::save::{
    save_processed_image, save_processed_multiband_image_sequential,
};
use crate::error::{Error, Result};
use crate::io::sentinel1::SafeReader;
use crate::types::{
    AutoscaleStrategy, BitDepth, OutputFormat, Polarization, PolarizationOperation,
    ProcessingOperation, SyntheticRgbMode,
};

/// Convenience variant with explicit options (typed)
pub fn process_safe_with_options(
    input: &Path,
    output: &Path,
    format: OutputFormat,
    bit_depth: BitDepth,
    polarization: Polarization,
    autoscale: AutoscaleStrategy,
    size: Option<usize>,
    pad: bool,
) -> Result<()> {
    let reader = SafeReader::open_with_options(
        input,
        polarization,
        None,
        None,
        size,
    )?;

    match polarization {
        Polarization::Vv | Polarization::Vh | Polarization::Hh | Polarization::Hv => {
            let processed = match polarization {
                Polarization::Vv => reader.vv_data()?,
                Polarization::Vh => reader.vh_data()?,
                Polarization::Hh => reader.hh_data()?,
                Polarization::Hv => reader.hv_data()?,
                _ => unreachable!(),
            };

            save_processed_image(
                processed,
                output,
                format,
                bit_depth,
                size,
                Some(reader.metadata()),
                pad,
                autoscale,
                ProcessingOperation::SingleBand,
            )
            .map_err(|e| Error::external(e))
        }
        Polarization::Multiband => {
            if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                let vv = reader.vv_data()?;
                let vh = reader.vh_data()?;
                save_processed_multiband_image_sequential(
                    vv,
                    vh,
                    output,
                    format,
                    bit_depth,
                    size,
                    Some(reader.metadata()),
                    pad,
                    autoscale,
                    ProcessingOperation::MultibandVvVh,
                    SyntheticRgbMode::Default,
                )
                .map_err(|e| Error::external(e))
            } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                let hh = reader.hh_data()?;
                let hv = reader.hv_data()?;
                save_processed_multiband_image_sequential(
                    hh,
                    hv,
                    output,
                    format,
                    bit_depth,
                    size,
                    Some(reader.metadata()),
                    pad,
                    autoscale,
                    ProcessingOperation::MultibandHhHv,
                    SyntheticRgbMode::Default,
                )
                .map_err(|e| Error::external(e))
            } else {
                Err(Error::Processing(format!(
                    "Multiband requires VV+VH or HH+HV; available: {}",
                    reader.get_available_polarizations()
                )))
            }
        }
        Polarization::OP(op) => {
            let processed: ndarray::Array2<f32> =
                if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                    match op {
                        PolarizationOperation::Sum => reader.sum_data()?,
                        PolarizationOperation::Diff => reader.difference_data()?,
                        PolarizationOperation::Ratio => reader.ratio_data()?,
                        PolarizationOperation::NDiff => reader.normalized_diff_data()?,
                        PolarizationOperation::LogRatio => reader.log_ratio_data()?,
                    }
                } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                    match op {
                        PolarizationOperation::Sum => reader.sum_hh_hv_data()?,
                        PolarizationOperation::Diff => reader.difference_hh_hv_data()?,
                        PolarizationOperation::Ratio => reader.ratio_hh_hv_data()?,
                        PolarizationOperation::NDiff => reader.normalized_diff_hh_hv_data()?,
                        PolarizationOperation::LogRatio => reader.log_ratio_hh_hv_data()?,
                    }
                } else {
                    return Err(Error::Processing(format!(
                        "Operation {} requires VV+VH or HH+HV; available: {}",
                        operation_to_str(op),
                        reader.get_available_polarizations()
                    )));
                };

            save_processed_image(
                &processed,
                output,
                format,
                bit_depth,
                size,
                Some(reader.metadata()),
                pad,
                autoscale,
                ProcessingOperation::PolarOp(op),
            )
            .map_err(|e| Error::external(e))
        }
    }
}

fn operation_to_str(op: PolarizationOperation) -> &'static str {
    match op {
        PolarizationOperation::Sum => "sum",
        PolarizationOperation::Diff => "difference",
        PolarizationOperation::Ratio => "ratio",
        PolarizationOperation::NDiff => "normalized_diff",
        PolarizationOperation::LogRatio => "log_ratio",
    }
}
