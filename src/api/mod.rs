//! High-level, ergonomic library API: process SAFE to files or in-memory buffers,
//! batch helpers for directories, and typed save/load helpers. Prefer using these
//! entrypoints over low-level processing modules when integrating SARPRO.
use std::path::Path;

use ndarray::Array2;
use num_complex::Complex;

use crate::core::params::ProcessingParams;
use crate::core::processing::pipeline::process_complex_data_pipeline;
use crate::core::processing::resize::resize_image_data;
use crate::core::processing::save::{
    save_processed_image, save_processed_multiband_image_sequential,
};
use crate::core::processing::synthetic_rgb::create_synthetic_rgb;
use crate::error::{Error, Result};
use crate::io::sentinel1::{SafeMetadata, SafeReader};
use crate::types::{
    AutoscaleStrategy, BitDepth, BitDepthArg, OutputFormat, Polarization, PolarizationOperation,
    ProcessingOperation,
};

fn operation_to_str(op: PolarizationOperation) -> &'static str {
    match op {
        PolarizationOperation::Sum => "sum",
        PolarizationOperation::Diff => "difference",
        PolarizationOperation::Ratio => "ratio",
        PolarizationOperation::NDiff => "normalized_diff",
        PolarizationOperation::LogRatio => "log_ratio",
    }
}

fn bitdepth_arg_to_bitdepth(arg: BitDepthArg) -> BitDepth {
    match arg {
        BitDepthArg::U8 => BitDepth::U8,
        BitDepthArg::U16 => BitDepth::U16,
    }
}

fn pol_to_reader_hint(pol: &Polarization) -> Option<&'static str> {
    match pol {
        Polarization::Vv => Some("vv"),
        Polarization::Vh => Some("vh"),
        Polarization::Hh => Some("hh"),
        Polarization::Hv => Some("hv"),
        Polarization::Multiband | Polarization::OP(_) => Some("all_pairs"),
    }
}

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
    let reader = SafeReader::open(input, pol_to_reader_hint(&polarization))?;

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
                process_complex_data_pipeline(&processed, bit_depth, autoscale);
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
                process_complex_data_pipeline(&band1, bit_depth, autoscale);
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
                process_complex_data_pipeline(&band2, bit_depth, autoscale);
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
                process_complex_data_pipeline(&band1, BitDepth::U8, autoscale);
            let (rows, cols) = db1.dim();
            let (final_cols, final_rows, final1_u8, _) =
                resize_image_data(&s1_u8, None, cols, rows, target_size, BitDepth::U8, pad)
                    .map_err(|e| Error::external(e))?;

            let (_db2, _m2, s2_u8, _s2_u16) =
                process_complex_data_pipeline(&band2, BitDepth::U8, autoscale);
            let (_c2, _r2, final2_u8, _) =
                resize_image_data(&s2_u8, None, cols, rows, target_size, BitDepth::U8, pad)
                    .map_err(|e| Error::external(e))?;

            let rgb = create_synthetic_rgb(&final1_u8, &final2_u8);

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
                process_complex_data_pipeline(&processed, BitDepth::U8, autoscale);
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
            let combined: Array2<Complex<f64>> =
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
                        process_complex_data_pipeline(&combined, bit_depth, autoscale);
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
                        process_complex_data_pipeline(&combined, BitDepth::U8, autoscale);
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

/// Batch processing report
#[derive(Debug, Clone, Copy, Default)]
pub struct BatchReport {
    pub processed: usize,
    pub skipped: usize,
    pub errors: usize,
}

/// Return an iterator over immediate subdirectories of `input_dir` (candidate SAFE products)
pub fn iterate_safe_products(input_dir: &Path) -> Result<std::vec::IntoIter<std::path::PathBuf>> {
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(input_dir).map_err(Error::from)? {
        let entry = entry.map_err(Error::from)?;
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        }
    }
    Ok(dirs.into_iter())
}

/// Process all SAFE subdirectories from `input_dir` into `output_dir` using `params`.
/// If `continue_on_error` is true, errors are logged in the report and processing continues; otherwise, the first error is returned.
pub fn process_directory_to_path(
    input_dir: &Path,
    output_dir: &Path,
    params: &ProcessingParams,
    continue_on_error: bool,
) -> Result<BatchReport> {
    std::fs::create_dir_all(output_dir).map_err(Error::from)?;

    let mut report = BatchReport::default();

    let mut iter = iterate_safe_products(input_dir)?;
    while let Some(path) = iter.next() {
        // Early viability check to allow skipping unsupported product types
        match SafeReader::open_with_warnings(&path, pol_to_reader_hint(&params.polarization))? {
            Some(_) => {
                // Determine output file name
                let safe_name = path.file_name().unwrap().to_string_lossy();
                let ext = match params.format {
                    OutputFormat::TIFF => "tiff",
                    OutputFormat::JPEG => "jpg",
                };
                let output_path = output_dir.join(format!("{}.{}", safe_name, ext));

                match process_safe_to_path(&path, &output_path, params) {
                    Ok(()) => report.processed += 1,
                    Err(e) => {
                        report.errors += 1;
                        if !continue_on_error {
                            return Err(e);
                        }
                    }
                }
            }
            None => {
                report.skipped += 1;
                continue;
            }
        }
    }

    Ok(report)
}

/// Process a SAFE input to an output path using ProcessingParams
pub fn process_safe_to_path(input: &Path, output: &Path, params: &ProcessingParams) -> Result<()> {
    let bit_depth = bitdepth_arg_to_bitdepth(params.bit_depth);

    // Open reader according to polarization
    let reader = SafeReader::open(input, pol_to_reader_hint(&params.polarization))?;

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
                &processed,
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
                    &vv,
                    &vh,
                    output,
                    params.format,
                    bit_depth,
                    params.size,
                    Some(reader.metadata()),
                    params.pad,
                    params.autoscale,
                    ProcessingOperation::MultibandVvVh,
                )
                .map_err(|e| Error::external(e))
            } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                let hh = reader.hh_data()?;
                let hv = reader.hv_data()?;
                save_processed_multiband_image_sequential(
                    &hh,
                    &hv,
                    output,
                    params.format,
                    bit_depth,
                    params.size,
                    Some(reader.metadata()),
                    params.pad,
                    params.autoscale,
                    ProcessingOperation::MultibandHhHv,
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
            let processed: Array2<Complex<f64>> =
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
    let reader = SafeReader::open(input, pol_to_reader_hint(&polarization))?;

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
                &processed,
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
                    &vv,
                    &vh,
                    output,
                    format,
                    bit_depth,
                    size,
                    Some(reader.metadata()),
                    pad,
                    autoscale,
                    ProcessingOperation::MultibandVvVh,
                )
                .map_err(|e| Error::external(e))
            } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                let hh = reader.hh_data()?;
                let hv = reader.hv_data()?;
                save_processed_multiband_image_sequential(
                    &hh,
                    &hv,
                    output,
                    format,
                    bit_depth,
                    size,
                    Some(reader.metadata()),
                    pad,
                    autoscale,
                    ProcessingOperation::MultibandHhHv,
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
            let processed: Array2<Complex<f64>> =
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

/// Typed save helper for single-band arrays
pub fn save_image(
    processed: &Array2<Complex<f64>>,
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

/// Typed save helper for multiband arrays (VV/VH or HH/HV)
pub fn save_multiband_image(
    processed1: &Array2<Complex<f64>>,
    processed2: &Array2<Complex<f64>>,
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
    )
    .map_err(|e| Error::external(e))
}

/// Load a single polarization's complex array and metadata
pub fn load_polarization(
    input: &Path,
    pol: Polarization,
) -> Result<(Array2<Complex<f64>>, SafeMetadata)> {
    match pol {
        Polarization::Multiband | Polarization::OP(_) => {
            return Err(Error::Processing(
                "load_polarization expects a single polarization (vv/vh/hh/hv)".to_string(),
            ));
        }
        _ => {}
    }

    let reader = SafeReader::open(input, pol_to_reader_hint(&pol))?;
    let data = match pol {
        Polarization::Vv => reader.vv_data()?,
        Polarization::Vh => reader.vh_data()?,
        Polarization::Hh => reader.hh_data()?,
        Polarization::Hv => reader.hv_data()?,
        _ => unreachable!(),
    };
    Ok((data, reader.metadata.clone()))
}

/// Compute an operation (sum/diff/ratio/...) over an available pair and return array + metadata
pub fn load_operation(
    input: &Path,
    op: PolarizationOperation,
) -> Result<(Array2<Complex<f64>>, SafeMetadata)> {
    let reader = SafeReader::open(input, Some("all_pairs"))?;

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
