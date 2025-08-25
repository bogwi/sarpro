//! Process SAFE input to in-memory buffers

use std::path::Path;
use ndarray::Array2;

use crate::core::processing::pipeline::process_scalar_data_pipeline;
use crate::core::processing::resize::resize_image_data;
use crate::core::processing::synthetic_rgb::create_synthetic_rgb_by_mode_and_strategy;
use crate::error::{Error, Result};
use crate::io::sentinel1::SafeReader;
use crate::types::{
    AutoscaleStrategy, BitDepth, OutputFormat, Polarization, PolarizationOperation,
    SyntheticRgbMode,
};
use super::processed_image::ProcessedImage;

fn operation_to_str(op: PolarizationOperation) -> &'static str {
    match op {
        PolarizationOperation::Sum => "sum",
        PolarizationOperation::Diff => "difference",
        PolarizationOperation::Ratio => "ratio",
        PolarizationOperation::NDiff => "normalized_diff",
        PolarizationOperation::LogRatio => "log_ratio",
    }
}

/// Process a SAFE input to in-memory buffers (no disk I/O)
pub fn process_safe_to_buffer(
    input: &Path,
    polarization: Polarization,
    autoscale: AutoscaleStrategy,
    bit_depth: BitDepth,
    target_size: Option<usize>,
    pad: bool,
    output_format: OutputFormat,
) -> Result<ProcessedImage> {
    let reader = SafeReader::open_with_options(
        input,
        polarization,
        None,
        None,
        target_size,
    )?;

    match (output_format, polarization) {
        // Single-band TIFF (U8/U16)
        (
            OutputFormat::TIFF,
            Polarization::Vv | Polarization::Vh | Polarization::Hh | Polarization::Hv,
        ) => {
            let processed = match polarization {
                Polarization::Vv => reader.vv_data()?,
                Polarization::Vh => reader.vh_data()?,
                Polarization::Hh => reader.hh_data()?,
                Polarization::Hv => reader.hv_data()?,
                _ => unreachable!(),
            };

            let (db_data, _mask, scaled_u8, scaled_u16) =
                process_scalar_data_pipeline(processed, bit_depth, autoscale);
            let (rows, cols) = db_data.dim();
            let (final_cols, final_rows, final_u8, final_u16) = resize_image_data(
                &scaled_u8,
                scaled_u16.as_deref(),
                cols,
                rows,
                target_size,
                bit_depth,
                pad,
            )
            .map_err(|e| Error::external(e))?;

            Ok(ProcessedImage {
                width: final_cols,
                height: final_rows,
                bit_depth,
                format: OutputFormat::TIFF,
                gray: if matches!(bit_depth, BitDepth::U8) {
                    Some(final_u8)
                } else {
                    None
                },
                gray16: if matches!(bit_depth, BitDepth::U16) {
                    final_u16
                } else {
                    None
                },
                rgb: None,
                gray_band2: None,
                gray16_band2: None,
                metadata: reader.metadata.clone(),
            })
        }

        // Multiband TIFF (two bands U8/U16). Prefer VV/VH, else HH/HV
        (OutputFormat::TIFF, Polarization::Multiband) => {
            let (band1, band2) = if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                (reader.vv_data()?, reader.vh_data()?)
            } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                (reader.hh_data()?, reader.hv_data()?)
            } else {
                return Err(Error::Processing(format!(
                    "Multiband requires VV+VH or HH+HV; available: {}",
                    reader.get_available_polarizations()
                )));
            };

            let (db1, _m1, s1_u8, s1_u16) =
                process_scalar_data_pipeline(band1, bit_depth, autoscale);
            let (rows, cols) = db1.dim();
            let (final_cols, final_rows, final1_u8, final1_u16) = resize_image_data(
                &s1_u8,
                s1_u16.as_deref(),
                cols,
                rows,
                target_size,
                bit_depth,
                pad,
            )
            .map_err(|e| Error::external(e))?;

            let (_db2, _m2, s2_u8, s2_u16) =
                process_scalar_data_pipeline(band2, bit_depth, autoscale);
            let (_c2, _r2, final2_u8, final2_u16) = resize_image_data(
                &s2_u8,
                s2_u16.as_deref(),
                cols,
                rows,
                target_size,
                bit_depth,
                pad,
            )
            .map_err(|e| Error::external(e))?;

            Ok(ProcessedImage {
                width: final_cols,
                height: final_rows,
                bit_depth,
                format: OutputFormat::TIFF,
                gray: if matches!(bit_depth, BitDepth::U8) {
                    Some(final1_u8)
                } else {
                    None
                },
                gray16: if matches!(bit_depth, BitDepth::U16) {
                    final1_u16.clone()
                } else {
                    None
                },
                rgb: None,
                gray_band2: if matches!(bit_depth, BitDepth::U8) {
                    Some(final2_u8)
                } else {
                    None
                },
                gray16_band2: if matches!(bit_depth, BitDepth::U16) {
                    final2_u16
                } else {
                    None
                },
                metadata: reader.metadata.clone(),
            })
        }

        // Synthetic RGB JPEG (two bands => RGB), prefer VV/VH else HH/HV
        (OutputFormat::JPEG, Polarization::Multiband) => {
            let (band1, band2) = if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                (reader.vv_data()?, reader.vh_data()?)
            } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                (reader.hh_data()?, reader.hv_data()?)
            } else {
                return Err(Error::Processing(format!(
                    "Multiband requires VV+VH or HH+HV; available: {}",
                    reader.get_available_polarizations()
                )));
            };

            let (db1, _m1, s1_u8, _s1_u16) =
                process_scalar_data_pipeline(band1, BitDepth::U8, autoscale);
            let (rows, cols) = db1.dim();
            let (final_cols, final_rows, final1_u8, _) =
                resize_image_data(&s1_u8, None, cols, rows, target_size, BitDepth::U8, pad)
                    .map_err(|e| Error::external(e))?;

            let (_db2, _m2, s2_u8, _s2_u16) =
                process_scalar_data_pipeline(band2, BitDepth::U8, autoscale);
            let (_c2, _r2, final2_u8, _) =
                resize_image_data(&s2_u8, None, cols, rows, target_size, BitDepth::U8, pad)
                    .map_err(|e| Error::external(e))?;

            let rgb = create_synthetic_rgb_by_mode_and_strategy(
                SyntheticRgbMode::Default,
                autoscale,
                &final1_u8,
                &final2_u8,
            );

            Ok(ProcessedImage {
                width: final_cols,
                height: final_rows,
                bit_depth: BitDepth::U8,
                format: OutputFormat::JPEG,
                gray: None,
                gray16: None,
                rgb: Some(rgb),
                gray_band2: None,
                gray16_band2: None,
                metadata: reader.metadata.clone(),
            })
        }

        // Single-band JPEG grayscale (always U8)
        (
            OutputFormat::JPEG,
            Polarization::Vv | Polarization::Vh | Polarization::Hh | Polarization::Hv,
        ) => {
            let processed = match polarization {
                Polarization::Vv => reader.vv_data()?,
                Polarization::Vh => reader.vh_data()?,
                Polarization::Hh => reader.hh_data()?,
                Polarization::Hv => reader.hv_data()?,
                _ => unreachable!(),
            };

            let (db_data, _m, s_u8, _s_u16) =
                process_scalar_data_pipeline(processed, BitDepth::U8, autoscale);
            let (rows, cols) = db_data.dim();
            let (final_cols, final_rows, final_u8, _) =
                resize_image_data(&s_u8, None, cols, rows, target_size, BitDepth::U8, pad)
                    .map_err(|e| Error::external(e))?;

            Ok(ProcessedImage {
                width: final_cols,
                height: final_rows,
                bit_depth: BitDepth::U8,
                format: OutputFormat::JPEG,
                gray: Some(final_u8),
                gray16: None,
                rgb: None,
                gray_band2: None,
                gray16_band2: None,
                metadata: reader.metadata.clone(),
            })
        }

        // Polarization operation -> single band path
        (format, Polarization::OP(op)) => {
            // Resolve to a single processed band first, then reuse single-band paths above
            let combined: Array2<f32> =
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

            match format {
                OutputFormat::TIFF => {
                    let (db_data, _m, s_u8, s_u16) =
                        process_scalar_data_pipeline(&combined, bit_depth, autoscale);
                    let (rows, cols) = db_data.dim();
                    let (final_cols, final_rows, final_u8, final_u16) = resize_image_data(
                        &s_u8,
                        s_u16.as_deref(),
                        cols,
                        rows,
                        target_size,
                        bit_depth,
                        pad,
                    )
                    .map_err(|e| Error::external(e))?;

                    Ok(ProcessedImage {
                        width: final_cols,
                        height: final_rows,
                        bit_depth,
                        format: OutputFormat::TIFF,
                        gray: if matches!(bit_depth, BitDepth::U8) {
                            Some(final_u8)
                        } else {
                            None
                        },
                        gray16: if matches!(bit_depth, BitDepth::U16) {
                            final_u16
                        } else {
                            None
                        },
                        rgb: None,
                        gray_band2: None,
                        gray16_band2: None,
                        metadata: reader.metadata.clone(),
                    })
                }
                OutputFormat::JPEG => {
                    let (db_data, _m, s_u8, _s_u16) =
                        process_scalar_data_pipeline(&combined, BitDepth::U8, autoscale);
                    let (rows, cols) = db_data.dim();
                    let (final_cols, final_rows, final_u8, _) =
                        resize_image_data(&s_u8, None, cols, rows, target_size, BitDepth::U8, pad)
                            .map_err(|e| Error::external(e))?;
                    Ok(ProcessedImage {
                        width: final_cols,
                        height: final_rows,
                        bit_depth: BitDepth::U8,
                        format: OutputFormat::JPEG,
                        gray: Some(final_u8),
                        gray16: None,
                        rgb: None,
                        gray_band2: None,
                        gray16_band2: None,
                        metadata: reader.metadata.clone(),
                    })
                }
            }
        }
    }
}
