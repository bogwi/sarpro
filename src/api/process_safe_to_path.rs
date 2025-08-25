//! Process a SAFE input to an output path using ProcessingParams

use std::path::Path;
use crate::core::params::ProcessingParams;
use crate::core::processing::save::{
    save_processed_image, save_processed_multiband_image_sequential,
};
use crate::error::{Error, Result};
use crate::io::sentinel1::{SafeReader, TargetCrsArg};
use crate::types::{
    BitDepth, BitDepthArg, Polarization, PolarizationOperation,
    ProcessingOperation,
};

fn bitdepth_arg_to_bitdepth(arg: BitDepthArg) -> BitDepth {
    match arg {
        BitDepthArg::U8 => BitDepth::U8,
        BitDepthArg::U16 => BitDepth::U16,
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

/// Process a SAFE input to an output path using ProcessingParams
pub fn process_safe_to_path(input: &Path, output: &Path, params: &ProcessingParams) -> Result<()> {
    let bit_depth = bitdepth_arg_to_bitdepth(params.bit_depth);

    // Open reader according to polarization
    // Map target CRS into internal enum; resolution happens inside reader
    let target_arg: Option<TargetCrsArg> = match params.target_crs.as_deref() {
        Some(t) if t.eq_ignore_ascii_case("none") => Some(TargetCrsArg::None),
        Some(t) if t.eq_ignore_ascii_case("auto") => Some(TargetCrsArg::Auto),
        Some(t) => Some(TargetCrsArg::Custom(t.to_string())),
        None => None,
    };

    let resample_alg = match params.resample_alg.as_deref() {
        Some("nearest") => Some(gdal::raster::ResampleAlg::NearestNeighbour),
        Some("bilinear") => Some(gdal::raster::ResampleAlg::Bilinear),
        Some("cubic") => Some(gdal::raster::ResampleAlg::Cubic),
        Some("lanczos") | None => Some(gdal::raster::ResampleAlg::Lanczos),
        Some(_) => Some(gdal::raster::ResampleAlg::Lanczos),
    };

    let reader = SafeReader::open_with_options(
        input,
        params.polarization,
        target_arg,
        resample_alg,
        params.size,
    )?;

    match params.polarization {
        Polarization::Vv | Polarization::Vh | Polarization::Hh | Polarization::Hv => {
            let processed = match params.polarization {
                Polarization::Vv => reader.vv_data()?,
                Polarization::Vh => reader.vh_data()?,
                Polarization::Hh => reader.hh_data()?,
                Polarization::Hv => reader.hv_data()?,
                _ => unreachable!(),
            };

            save_processed_image(
                processed,
                output,
                params.format,
                bit_depth,
                params.size,
                Some(reader.metadata()),
                params.pad,
                params.autoscale,
                ProcessingOperation::SingleBand,
            )
            .map_err(|e| Error::external(e))
        }
        Polarization::Multiband => {
            // Prefer VV/VH if present, otherwise HH/HV
            if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                let vv = reader.vv_data()?;
                let vh = reader.vh_data()?;
                save_processed_multiband_image_sequential(
                    vv,
                    vh,
                    output,
                    params.format,
                    bit_depth,
                    params.size,
                    Some(reader.metadata()),
                    params.pad,
                    params.autoscale,
                    ProcessingOperation::MultibandVvVh,
                    params.synrgb_mode,
                )
                .map_err(|e| Error::external(e))
            } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                let hh = reader.hh_data()?;
                let hv = reader.hv_data()?;
                save_processed_multiband_image_sequential(
                    hh,
                    hv,
                    output,
                    params.format,
                    bit_depth,
                    params.size,
                    Some(reader.metadata()),
                    params.pad,
                    params.autoscale,
                    ProcessingOperation::MultibandHhHv,
                    params.synrgb_mode,
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
            // Choose pair (VV/VH preferred)
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
                params.format,
                bit_depth,
                params.size,
                Some(reader.metadata()),
                params.pad,
                params.autoscale,
                ProcessingOperation::PolarOp(op),
            )
            .map_err(|e| Error::external(e))
        }
    }
}

/// Internal helper to mirror `process_safe_to_path` but accept `ProcessingOptions`.
/// For Phase 1 this simply maps to the same behavior as `ProcessingParams`.
pub(crate) fn process_safe_to_path_with_options(
    input: &Path,
    output: &Path,
    opts: &crate::core::params::ProcessingOptions,
) -> Result<()> {
    // Build a temporary ProcessingParams to reuse the legacy path without behavior change
    let params = ProcessingParams {
        format: opts.format,
        input_format: opts.input_format,
        bit_depth: match opts.bit_depth {
            crate::types::BitDepthArg::U8 => crate::types::BitDepthArg::U8,
            crate::types::BitDepthArg::U16 => crate::types::BitDepthArg::U16,
        },
        polarization: opts.polarization,
        autoscale: opts.autoscale,
        synrgb_mode: opts.synrgb_mode,
        size: opts.size,
        pad: opts.pad,
        target_crs: opts.target_crs.clone(),
        resample_alg: opts.resample_alg.clone(),
    };
    process_safe_to_path(input, output, &params)
}
