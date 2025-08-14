use std::fs;
use std::path::PathBuf;

use num_complex::Complex;
use tracing::{info, warn};

use gdal::raster::ResampleAlg;
use sarpro::core::processing::save::{
    save_processed_image, save_processed_multiband_image_sequential,
};
use sarpro::io::SafeReader;
use sarpro::types::{BitDepth, OutputFormat, ProcessingOperation};
use sarpro::{AutoscaleStrategy, BitDepthArg, InputFormat, Polarization, PolarizationOperation};

use super::args::CliArgs;
use super::errors::AppError;

fn process_single_file(
    input: &PathBuf,
    output: &PathBuf,
    format: OutputFormat,
    bit_depth: BitDepthArg,
    input_format: InputFormat,
    polarization: Polarization,
    autoscale: AutoscaleStrategy,
    size: &str,
    batch_mode: bool,
    pad: bool,
    target_crs: Option<&str>,
    resample_alg: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let polarization_str = match polarization {
        Polarization::Vv => None,
        Polarization::Vh => Some("vh"),
        Polarization::Hh => Some("hh"),
        Polarization::Hv => Some("hv"),
        Polarization::Multiband | Polarization::OP(_) => Some("all_pairs"),
    };

    // autoscale now passed as typed enum directly

    let target_size = if size == "original" {
        None
    } else {
        let parsed_size = size.parse::<usize>().map_err(|_| AppError::InvalidSize {
            size: size.to_string(),
        })?;

        if parsed_size == 0 {
            return Err(AppError::ZeroSize { size: parsed_size }.into());
        }

        Some(parsed_size)
    };

    let reader = if batch_mode {
        match input_format {
            InputFormat::Safe => match SafeReader::open_with_warnings_with_options(
                input,
                polarization_str,
                None,
                None,
                target_size,
            )? {
                Some(reader) => reader,
                None => {
                    warn!("Skipping unsupported product type: {:?}", input);
                    return Ok(());
                }
            },
        }
    } else {
        match input_format {
            InputFormat::Safe => {
                let resample_alg = match resample_alg {
                    Some("nearest") => Some(ResampleAlg::NearestNeighbour),
                    Some("bilinear") => Some(ResampleAlg::Bilinear),
                    Some("cubic") => Some(ResampleAlg::Cubic),
                    _ => None,
                };
                if let Some(tgt) = target_crs {
                    if tgt.eq_ignore_ascii_case("none") {
                        // Explicitly disable reprojection but allow target-size downsample
                        SafeReader::open_with_options(
                            input,
                            polarization_str,
                            None,
                            resample_alg,
                            target_size,
                        )?
                    } else {
                        SafeReader::open_with_options(
                            input,
                            polarization_str,
                            Some(tgt),
                            resample_alg,
                            target_size,
                        )?
                    }
                } else {
                    SafeReader::open_with_options(
                        input,
                        polarization_str,
                        None,
                        resample_alg,
                        target_size,
                    )?
                }
            }
        }
    };

    let bit_depth_enum = match bit_depth {
        BitDepthArg::U8 => BitDepth::U8,
        BitDepthArg::U16 => BitDepth::U16,
    };

    match polarization {
        Polarization::Vv | Polarization::Vh | Polarization::Hh | Polarization::Hv => {
            let processed = match polarization {
                Polarization::Vv => reader.vv_data()?,
                Polarization::Vh => reader.vh_data()?,
                Polarization::Hh => reader.hh_data()?,
                Polarization::Hv => reader.hv_data()?,
                _ => unreachable!(),
            };

            let bytes = processed.len() * std::mem::size_of::<Complex<f64>>();
            info!(
                "Memory usage (approx): {:.2} MB",
                bytes as f64 / 1024.0 / 1024.0
            );

            save_processed_image(
                &processed,
                output.as_path(),
                format,
                bit_depth_enum,
                target_size,
                Some(reader.metadata()),
                pad,
                autoscale,
                ProcessingOperation::SingleBand,
            )
        }
        Polarization::Multiband => {
            if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                let vv_processed = reader.vv_data()?;
                let vh_processed = reader.vh_data()?;

                let total_bytes =
                    (vv_processed.len() + vh_processed.len()) * std::mem::size_of::<Complex<f64>>();
                info!(
                    "Memory usage (Multiband VV/VH): {:.2} MB",
                    total_bytes as f64 / 1024.0 / 1024.0
                );

                save_processed_multiband_image_sequential(
                    &vv_processed,
                    &vh_processed,
                    output.as_path(),
                    format,
                    bit_depth_enum,
                    target_size,
                    Some(reader.metadata()),
                    pad,
                    autoscale,
                    ProcessingOperation::MultibandVvVh,
                )
            } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                let hh_processed = reader.hh_data()?;
                let hv_processed = reader.hv_data()?;

                let total_bytes =
                    (hh_processed.len() + hv_processed.len()) * std::mem::size_of::<Complex<f64>>();
                info!(
                    "Memory usage (Multiband HH/HV): {:.2} MB",
                    total_bytes as f64 / 1024.0 / 1024.0
                );

                save_processed_multiband_image_sequential(
                    &hh_processed,
                    &hv_processed,
                    output.as_path(),
                    format,
                    bit_depth_enum,
                    target_size,
                    Some(reader.metadata()),
                    pad,
                    autoscale,
                    ProcessingOperation::MultibandHhHv,
                )
            } else {
                let available = reader.get_available_polarizations();
                return Err(AppError::IncompleDataPair {
                    operation: "multiband".to_string(),
                    available,
                }
                .into());
            }
        }
        Polarization::OP(_) => {
            let processed = if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                match polarization {
                    Polarization::OP(PolarizationOperation::Sum) => reader.sum_data()?,
                    Polarization::OP(PolarizationOperation::Diff) => reader.difference_data()?,
                    Polarization::OP(PolarizationOperation::Ratio) => reader.ratio_data()?,
                    Polarization::OP(PolarizationOperation::NDiff) => {
                        reader.normalized_diff_data()?
                    }
                    Polarization::OP(PolarizationOperation::LogRatio) => reader.log_ratio_data()?,
                    _ => unreachable!(),
                }
            } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                match polarization {
                    Polarization::OP(PolarizationOperation::Sum) => reader.sum_hh_hv_data()?,
                    Polarization::OP(PolarizationOperation::Diff) => {
                        reader.difference_hh_hv_data()?
                    }
                    Polarization::OP(PolarizationOperation::Ratio) => reader.ratio_hh_hv_data()?,
                    Polarization::OP(PolarizationOperation::NDiff) => {
                        reader.normalized_diff_hh_hv_data()?
                    }
                    Polarization::OP(PolarizationOperation::LogRatio) => {
                        reader.log_ratio_hh_hv_data()?
                    }
                    _ => unreachable!(),
                }
            } else {
                let available = reader.get_available_polarizations();
                return Err(AppError::IncompleDataPair {
                    operation: polarization.to_string(),
                    available,
                }
                .into());
            };

            let bytes = processed.len() * std::mem::size_of::<Complex<f64>>();
            info!(
                "Memory usage (approx): {:.2} MB",
                bytes as f64 / 1024.0 / 1024.0
            );

            save_processed_image(
                &processed,
                output.as_path(),
                format,
                bit_depth_enum,
                target_size,
                Some(reader.metadata()),
                pad,
                autoscale,
                ProcessingOperation::PolarOp(match polarization {
                    Polarization::OP(op) => op,
                    _ => unreachable!(),
                }),
            )
        }
    }
}

pub fn run(args: CliArgs) -> Result<(), Box<dyn std::error::Error>> {
    if args.log {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    }

    let batch_mode = args.batch || args.input_dir.is_some();

    if batch_mode {
        let input_dir = args.input_dir.ok_or(AppError::MissingArgument {
            arg: "--input-dir".to_string(),
        })?;
        let output_dir = args.output_dir.ok_or(AppError::MissingArgument {
            arg: "--output-dir".to_string(),
        })?;

        fs::create_dir_all(&output_dir)?;

        info!("Starting batch processing from directory: {:?}", input_dir);
        info!("Output directory: {:?}", output_dir);

        let mut processed = 0;
        let mut skipped = 0;
        let mut errors = 0;

        for entry in fs::read_dir(&input_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let safe_name = path.file_name().unwrap().to_string_lossy();
                let output_name = format!(
                    "{}.{}",
                    safe_name,
                    match args.format {
                        OutputFormat::TIFF => "tiff",
                        OutputFormat::JPEG => "jpg",
                    }
                );
                let output_path = output_dir.join(&output_name);

                info!("Processing: {:?} -> {:?}", path, output_path);

                match process_single_file(
                    &path,
                    &output_path,
                    args.format,
                    args.bit_depth,
                    args.input_format,
                    args.polarization,
                    args.autoscale,
                    &args.size,
                    true,
                    args.pad,
                    args.target_crs.as_deref(),
                    args.resample_alg.as_deref(),
                ) {
                    Ok(()) => {
                        info!("Successfully processed: {:?}\n", path);
                        processed += 1;
                    }
                    Err(e) => {
                        warn!("Error processing {:?}: {}", path, e);
                        errors += 1;
                    }
                }
            } else {
                info!("Skipping non-directory: {:?}", path);
                skipped += 1;
            }
        }

        info!("Batch processing complete!");
        info!("Processed: {}", processed);
        info!("Skipped: {}", skipped);
        info!("Errors: {}", errors);
    } else {
        let input = args.input.ok_or(AppError::MissingArgument {
            arg: "--input".to_string(),
        })?;
        let output = args.output.ok_or(AppError::MissingArgument {
            arg: "--output".to_string(),
        })?;

        process_single_file(
            &input,
            &output,
            args.format,
            args.bit_depth,
            args.input_format,
            args.polarization,
            args.autoscale,
            &args.size,
            false,
            args.pad,
            args.target_crs.as_deref(),
            args.resample_alg.as_deref(),
        )?;
        info!("Successfully processed: {:?} -> {:?}\n", input, output);
    }

    Ok(())
}
