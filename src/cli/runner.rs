use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tracing::{info, warn};

use gdal::raster::ResampleAlg;
use sarpro::core::processing::save::{
    save_processed_image, save_processed_multiband_image_sequential,
};
use sarpro::io::sentinel1::TargetCrsArg;
use sarpro::io::{SafeReader, SafeError};
use sarpro::io::gdal::to_gdal_path;
use sarpro::types::{BitDepth, OutputFormat, ProcessingOperation, SyntheticRgbMode};
use sarpro::{AutoscaleStrategy, BitDepthArg, InputFormat, Polarization, PolarizationOperation};
use sarpro::core::params::ProcessingOptions;

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
    synrgb_mode: SyntheticRgbMode,
    _proc_opts: &ProcessingOptions,
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
            InputFormat::Safe => {
                // Map resample algorithm
                let resample = match resample_alg {
                    Some("nearest") => Some(ResampleAlg::NearestNeighbour),
                    Some("bilinear") => Some(ResampleAlg::Bilinear),
                    Some("cubic") => Some(ResampleAlg::Cubic),
                    Some("lanczos") => Some(ResampleAlg::Lanczos),
                    _ => None,
                };
                // Map target CRS string into internal enum; resolution happens inside reader
                let target_arg: Option<TargetCrsArg> = match target_crs {
                    Some(t) if t.eq_ignore_ascii_case("none") => Some(TargetCrsArg::None),
                    Some(t) if t.eq_ignore_ascii_case("auto") => Some(TargetCrsArg::Auto),
                    Some(t) => Some(TargetCrsArg::Custom(t.to_string())),
                    None => None,
                };
                match SafeReader::open_with_warnings_with_options(
                    input,
                    polarization_str,
                    target_arg,
                    resample,
                    target_size,
                )? {
                Some(reader) => reader,
                None => {
                    warn!("Skipping unsupported product type: {:?}", input);
                    return Ok(());
                }
            }
        }
        }
    } else {
        match input_format {
            InputFormat::Safe => {
                let resample_alg = match resample_alg {
                    Some("nearest") => Some(ResampleAlg::NearestNeighbour),
                    Some("bilinear") => Some(ResampleAlg::Bilinear),
                    Some("cubic") => Some(ResampleAlg::Cubic),
                    Some("lanczos") => Some(ResampleAlg::Lanczos),
                    _ => None,
                };
                let target_arg: Option<TargetCrsArg> = match target_crs {
                    Some(t) if t.eq_ignore_ascii_case("none") => Some(TargetCrsArg::None),
                    Some(t) if t.eq_ignore_ascii_case("auto") => Some(TargetCrsArg::Auto),
                    Some(t) => Some(TargetCrsArg::Custom(t.to_string())),
                    None => None,
                };
                SafeReader::open_with_options(
                    input,
                    polarization_str,
                    target_arg,
                    resample_alg,
                    target_size,
                )?
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

            let bytes = processed.len() * std::mem::size_of::<f32>();
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
                    (vv_processed.len() + vh_processed.len()) * std::mem::size_of::<f32>();
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
                    synrgb_mode,
                )
            } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                let hh_processed = reader.hh_data()?;
                let hv_processed = reader.hv_data()?;

                let total_bytes =
                    (hh_processed.len() + hv_processed.len()) * std::mem::size_of::<f32>();
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
                    synrgb_mode,
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

            let bytes = processed.len() * std::mem::size_of::<f32>();
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

fn build_processing_options_from_args(args: &CliArgs) -> ProcessingOptions {
    // Map size string to Option<usize>
    let size_opt = if args.size == "original" {
        None
    } else {
        args.size.parse::<usize>().ok().filter(|v| *v > 0)
    };

    let mut opts = ProcessingOptions::default();
    opts.format = args.format;
    opts.input_format = args.input_format;
    opts.bit_depth = args.bit_depth;
    opts.polarization = args.polarization;
    opts.autoscale = args.autoscale;
    opts.synrgb_mode = args.synrgb_mode;
    opts.size = size_opt;
    opts.pad = args.pad;
    opts.target_crs = args.target_crs.clone();
    opts.resample_alg = args.resample_alg.clone();

    // Phase 1 flags: set STAC/IO hints; functionality comes in later steps
    opts.stac.in_path = args.stac_item.as_ref().map(|s| std::path::PathBuf::from(s));
    opts.io.remote = args.safe_zip_url.is_some()
        || args.vv_url.is_some()
        || args.vh_url.is_some()
        || args.hh_url.is_some()
        || args.hv_url.is_some()
        || opts.stac.in_path.is_some();
    opts.io.cache_dir = args.remote_cache_dir.clone();

    opts
}

pub fn run(args: CliArgs) -> Result<(), Box<dyn std::error::Error>> {
    if args.log {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    }

    let batch_mode = args.batch || args.input_dir.is_some();

    // Build ProcessingOptions aggregate (threaded but not yet acted upon by legacy path)
    let proc_opts = build_processing_options_from_args(&args);

    // Enforce mutually exclusive input modes when new flags are used
    let mut selected_modes = 0;
    if args.input.is_some() || args.input_dir.is_some() { selected_modes += 1; }
    if args.stac_item.is_some() { selected_modes += 1; }
    if args.safe_zip_url.is_some() { selected_modes += 1; }
    if args.vv_url.is_some() || args.vh_url.is_some() || args.hh_url.is_some() || args.hv_url.is_some() { selected_modes += 1; }
    if args.safe_zip_dir_url.is_some() { selected_modes += 1; }
    if selected_modes > 1 {
        return Err(AppError::MissingArgument { arg: "Select exactly one input mode (legacy --input/--input-dir OR one of: --stac-item | --safe-zip-url | --safe-zip-dir-url | --*-url)".to_string() }.into());
    }

    // Handle new remote/STAC modes (Step 3 implements remote explicit URLs and SAFE ZIP)
    if let Some(zip_url) = args.safe_zip_url.as_ref() {
        let output = args.output.clone().ok_or(AppError::MissingArgument { arg: "--output".to_string() })?;
        // Map resample/target args
        let resample_alg = match args.resample_alg.as_deref() {
            Some("nearest") => Some(ResampleAlg::NearestNeighbour),
            Some("bilinear") => Some(ResampleAlg::Bilinear),
            Some("cubic") => Some(ResampleAlg::Cubic),
            Some("lanczos") => Some(ResampleAlg::Lanczos),
            _ => None,
        };
        let target_arg: Option<TargetCrsArg> = match args.target_crs.as_deref() {
            Some(t) if t.eq_ignore_ascii_case("none") => Some(TargetCrsArg::None),
            Some(t) if t.eq_ignore_ascii_case("auto") => Some(TargetCrsArg::Auto),
            Some(t) => Some(TargetCrsArg::Custom(t.to_string())),
            None => None,
        };
        let target_size = if args.size == "original" { None } else { Some(args.size.parse::<usize>().map_err(|_| AppError::InvalidSize { size: args.size.clone() })?) };

        let reader = SafeReader::open_from_safe_zip_url(
            zip_url,
            args.polarization,
            target_arg,
            resample_alg,
            target_size,
        )?;

        // Dispatch save similar to legacy path
        let bit_depth_enum = match args.bit_depth { BitDepthArg::U8 => BitDepth::U8, BitDepthArg::U16 => BitDepth::U16 };
        match args.polarization {
            Polarization::Vv | Polarization::Vh | Polarization::Hh | Polarization::Hv => {
                let processed = match args.polarization {
                    Polarization::Vv => reader.vv_data()?,
                    Polarization::Vh => reader.vh_data()?,
                    Polarization::Hh => reader.hh_data()?,
                    Polarization::Hv => reader.hv_data()?,
                    _ => unreachable!(),
                };
                save_processed_image(
                    &processed,
                    output.as_path(),
                    args.format,
                    bit_depth_enum,
                    target_size,
                    Some(reader.metadata()),
                    args.pad,
                    args.autoscale,
                    ProcessingOperation::SingleBand,
                )?
            }
            Polarization::Multiband => {
                if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                    let vv_processed = reader.vv_data()?;
                    let vh_processed = reader.vh_data()?;
                    save_processed_multiband_image_sequential(
                        &vv_processed,
                        &vh_processed,
                        output.as_path(),
                        args.format,
                        bit_depth_enum,
                        target_size,
                        Some(reader.metadata()),
                        args.pad,
                        args.autoscale,
                        ProcessingOperation::MultibandVvVh,
                        args.synrgb_mode,
                    )?
                } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                    let hh_processed = reader.hh_data()?;
                    let hv_processed = reader.hv_data()?;
                    save_processed_multiband_image_sequential(
                        &hh_processed,
                        &hv_processed,
                        output.as_path(),
                        args.format,
                        bit_depth_enum,
                        target_size,
                        Some(reader.metadata()),
                        args.pad,
                        args.autoscale,
                        ProcessingOperation::MultibandHhHv,
                        args.synrgb_mode,
                    )?
                } else {
                    return Err(AppError::IncompleDataPair { operation: "multiband".to_string(), available: reader.get_available_polarizations() }.into());
                }
            }
            Polarization::OP(op) => {
                let processed = if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
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
                    return Err(AppError::IncompleDataPair { operation: args.polarization.to_string(), available: reader.get_available_polarizations() }.into());
                };
                save_processed_image(
                    &processed,
                    output.as_path(),
                    args.format,
                    bit_depth_enum,
                    target_size,
                    Some(reader.metadata()),
                    args.pad,
                    args.autoscale,
                    ProcessingOperation::PolarOp(op),
                )?
            }
        }
        info!("Successfully processed remote SAFE ZIP: {:?} -> {:?}", zip_url, output);
        return Ok(());
    }

    // Handle remote directory of SAFE ZIPs for batch processing via VSICURL directory listing
    if let Some(dir_url) = args.safe_zip_dir_url.as_ref() {
        if !args.batch {
            warn!("--safe-zip-dir-url implies batch processing; add --batch to process all ZIPs");
        }
        let output_dir = args.output_dir.clone().ok_or(AppError::MissingArgument { arg: "--output-dir".to_string() })?;
        fs::create_dir_all(&output_dir)?;

        // Use GDAL VSI to list directory. Build VSI base correctly and try common variants.
        let vsi_base = to_gdal_path(std::path::Path::new(dir_url)).into_owned();
        let vsi_with_slash = format!("{}/", vsi_base.trim_end_matches('/'));
        // Also try the alternative VSICURL option-style URL form with hints for HTML listings
        let opt_form = format!(
            "/vsicurl?use_head=no&list_dir=yes&allowed_extensions=.SAFE.zip,.zip&url={}",
            dir_url
        );
        let opt_form_slash = format!(
            "/vsicurl?use_head=no&list_dir=yes&allowed_extensions=.SAFE.zip,.zip&url={}/",
            dir_url.trim_end_matches('/')
        );
        let entries: Vec<PathBuf> = if let Ok(v) = gdal::vsi::read_dir(&vsi_with_slash, false) {
            v
        } else if let Ok(v) = gdal::vsi::read_dir(&vsi_with_slash, true) {
            v
        } else if let Ok(v) = gdal::vsi::read_dir(&vsi_base, false) {
            v
        } else if let Ok(v) = gdal::vsi::read_dir(&vsi_base, true) {
            v
        } else if let Ok(v) = gdal::vsi::read_dir(&opt_form_slash, false) {
            v
        } else if let Ok(v) = gdal::vsi::read_dir(&opt_form_slash, true) {
            v
        } else if let Ok(v) = gdal::vsi::read_dir(&opt_form, false) {
            v
        } else if let Ok(v) = gdal::vsi::read_dir(&opt_form, true) {
            v
        } else {
            // Fallback: fetch listing via curl and parse .SAFE.zip links/text
            let url_with_slash = format!("{}/", dir_url.trim_end_matches('/'));
            let out = Command::new("curl")
                .arg("-L")
                .arg("-s")
                .arg(&url_with_slash)
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    let body = String::from_utf8_lossy(&o.stdout);
                    // Extract candidates: either href links or plain lines
                    let mut names: Vec<String> = Vec::new();
                    // Parse hrefs
                    let mut start = 0usize;
                    while let Some(h) = body[start..].find("href=\"") {
                        let i = start + h + 6; // position after href="
                        if let Some(end) = body[i..].find('"') {
                            let candidate = &body[i..i + end];
                            if candidate.to_lowercase().ends_with(".safe.zip") {
                                if !candidate.contains("://") && !candidate.starts_with('/') {
                                    names.push(candidate.to_string());
                                } else if let Some(pos) = candidate.rsplit('/').next() {
                                    if pos.to_lowercase().ends_with(".safe.zip") {
                                        names.push(pos.to_string());
                                    }
                                }
                            }
                            start = i + end + 1;
                        } else {
                            break;
                        }
                    }
                    // Parse plain lines
                    for line in body.lines() {
                        let t = line.trim();
                        if t.to_lowercase().ends_with(".safe.zip") {
                            let fname = t.split_whitespace().last().unwrap_or(t);
                            names.push(fname.to_string());
                        }
                    }
                    if names.is_empty() {
                        return Err(AppError::RemoteDirListing {
                            url: dir_url.clone(),
                            hint: "No .SAFE.zip entries found. Ensure server lists files or use --safe-zip-url.".to_string(),
                        }
                        .into());
                    }
                    names.into_iter().map(PathBuf::from).collect()
                }
                _ => {
                    return Err(AppError::RemoteDirListing {
                        url: dir_url.clone(),
                        hint: "Failed to fetch listing. Ensure the URL is reachable or use --safe-zip-url.".to_string(),
                    }
                    .into());
                }
            }
        };

        let mut zips: Vec<String> = Vec::new();
        for name_os in entries.iter() {
            let name = name_os.to_string_lossy().to_string();
            let name_lc = name.to_lowercase();
            if name_lc.ends_with(".safe.zip") {
                zips.push(name);
            }
        }

        if zips.is_empty() {
            warn!("No .SAFE.zip files found under {}", dir_url);
            return Ok(());
        }

        info!("Found {} SAFE ZIPs to process", zips.len());

        // Map common params once
        let resample_alg = match args.resample_alg.as_deref() {
            Some("nearest") => Some(ResampleAlg::NearestNeighbour),
            Some("bilinear") => Some(ResampleAlg::Bilinear),
            Some("cubic") => Some(ResampleAlg::Cubic),
            Some("lanczos") => Some(ResampleAlg::Lanczos),
            _ => None,
        };
        let target_arg: Option<TargetCrsArg> = match args.target_crs.as_deref() {
            Some(t) if t.eq_ignore_ascii_case("none") => Some(TargetCrsArg::None),
            Some(t) if t.eq_ignore_ascii_case("auto") => Some(TargetCrsArg::Auto),
            Some(t) => Some(TargetCrsArg::Custom(t.to_string())),
            None => None,
        };
        let target_size = if args.size == "original" { None } else { Some(args.size.parse::<usize>().map_err(|_| AppError::InvalidSize { size: args.size.clone() })?) };

        let mut processed = 0usize;
        let mut errors = 0usize;

        for zip_name in zips.into_iter() {
            // Avoid stray entries
            if zip_name == "." || zip_name == ".." { continue; }
            let zip_url = format!("{}/{}", dir_url.trim_end_matches('/'), zip_name);
            let safe_stem = zip_name.trim_end_matches(".SAFE.zip");
            let out_name = format!("{}.{}", safe_stem, match args.format { OutputFormat::TIFF => "tiff", OutputFormat::JPEG => "jpg" });
            let out_path = output_dir.join(out_name);

            info!("Processing remote SAFE ZIP: {} -> {:?}", zip_url, out_path);
            let reader_res = SafeReader::open_from_safe_zip_url(
                &zip_url,
                args.polarization,
                target_arg.clone(),
                resample_alg,
                target_size,
            );

            match reader_res {
                Ok(reader) => {
                    let bit_depth_enum = match args.bit_depth { BitDepthArg::U8 => BitDepth::U8, BitDepthArg::U16 => BitDepth::U16 };
                    let save_res = match args.polarization {
                        Polarization::Vv | Polarization::Vh | Polarization::Hh | Polarization::Hv => {
                            let processed = match args.polarization {
                                Polarization::Vv => reader.vv_data(),
                                Polarization::Vh => reader.vh_data(),
                                Polarization::Hh => reader.hh_data(),
                                Polarization::Hv => reader.hv_data(),
                                _ => unreachable!(),
                            };
                            match processed {
                                Ok(arr) => save_processed_image(
                                    arr,
                                    out_path.as_path(),
                                    args.format,
                                    bit_depth_enum,
                                    target_size,
                                    Some(reader.metadata()),
                                    args.pad,
                                    args.autoscale,
                                    ProcessingOperation::SingleBand,
                                ),
                                Err(e) => Err(e.into()),
                            }
                        }
                        Polarization::Multiband => {
                            if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                                let vv_processed = reader.vv_data().unwrap();
                                let vh_processed = reader.vh_data().unwrap();
                                save_processed_multiband_image_sequential(
                                    vv_processed,
                                    vh_processed,
                                    out_path.as_path(),
                                    args.format,
                                    bit_depth_enum,
                                    target_size,
                                    Some(reader.metadata()),
                                    args.pad,
                                    args.autoscale,
                                    ProcessingOperation::MultibandVvVh,
                                    args.synrgb_mode,
                                )
                            } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                                let hh_processed = reader.hh_data().unwrap();
                                let hv_processed = reader.hv_data().unwrap();
                                save_processed_multiband_image_sequential(
                                    hh_processed,
                                    hv_processed,
                                    out_path.as_path(),
                                    args.format,
                                    bit_depth_enum,
                                    target_size,
                                    Some(reader.metadata()),
                                    args.pad,
                                    args.autoscale,
                                    ProcessingOperation::MultibandHhHv,
                                    args.synrgb_mode,
                                )
                            } else {
                                Err(AppError::IncompleDataPair { operation: "multiband".to_string(), available: reader.get_available_polarizations() }.into())
                            }
                        }
                        Polarization::OP(op) => {
                            let processed = if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                                match op {
                                    PolarizationOperation::Sum => reader.sum_data(),
                                    PolarizationOperation::Diff => reader.difference_data(),
                                    PolarizationOperation::Ratio => reader.ratio_data(),
                                    PolarizationOperation::NDiff => reader.normalized_diff_data(),
                                    PolarizationOperation::LogRatio => reader.log_ratio_data(),
                                }
                            } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                                match op {
                                    PolarizationOperation::Sum => reader.sum_hh_hv_data(),
                                    PolarizationOperation::Diff => reader.difference_hh_hv_data(),
                                    PolarizationOperation::Ratio => reader.ratio_hh_hv_data(),
                                    PolarizationOperation::NDiff => reader.normalized_diff_hh_hv_data(),
                                    PolarizationOperation::LogRatio => reader.log_ratio_hh_hv_data(),
                                }
                            } else {
                                Err(SafeError::MissingField("polarization pair for operation"))
                            };
                            match processed {
                                Ok(arr) => save_processed_image(
                                    &arr,
                                    out_path.as_path(),
                                    args.format,
                                    bit_depth_enum,
                                    target_size,
                                    Some(reader.metadata()),
                                    args.pad,
                                    args.autoscale,
                                    ProcessingOperation::PolarOp(op),
                                ),
                                Err(e) => Err(e.into()),
                            }
                        }
                    };
                    match save_res {
                        Ok(_) => { processed += 1; info!("Done: {:?}", out_path); }
                        Err(e) => { errors += 1; warn!("Error saving {:?}: {}", out_path, e); }
                    }
                }
                Err(e) => { errors += 1; warn!("Error opening {}: {}", zip_url, e); }
            }
        }

        info!("Remote ZIP batch complete. processed={}, errors={}", processed, errors);
        return Ok(());
    }

    if args.vv_url.is_some() || args.vh_url.is_some() || args.hh_url.is_some() || args.hv_url.is_some() {
        let output = args.output.clone().ok_or(AppError::MissingArgument { arg: "--output".to_string() })?;
        let resample_alg = match args.resample_alg.as_deref() {
            Some("nearest") => Some(ResampleAlg::NearestNeighbour),
            Some("bilinear") => Some(ResampleAlg::Bilinear),
            Some("cubic") => Some(ResampleAlg::Cubic),
            Some("lanczos") => Some(ResampleAlg::Lanczos),
            _ => None,
        };
        let target_arg: Option<TargetCrsArg> = match args.target_crs.as_deref() {
            Some(t) if t.eq_ignore_ascii_case("none") => Some(TargetCrsArg::None),
            Some(t) if t.eq_ignore_ascii_case("auto") => Some(TargetCrsArg::Auto),
            Some(t) => Some(TargetCrsArg::Custom(t.to_string())),
            None => None,
        };
        let target_size = if args.size == "original" { None } else { Some(args.size.parse::<usize>().map_err(|_| AppError::InvalidSize { size: args.size.clone() })?) };

        let reader = SafeReader::open_from_measurements(
            args.vv_url.as_deref(),
            args.vh_url.as_deref(),
            args.hh_url.as_deref(),
            args.hv_url.as_deref(),
            target_arg,
            resample_alg,
            target_size,
        )?;

        let bit_depth_enum = match args.bit_depth { BitDepthArg::U8 => BitDepth::U8, BitDepthArg::U16 => BitDepth::U16 };
        match args.polarization {
            Polarization::Vv | Polarization::Vh | Polarization::Hh | Polarization::Hv => {
                let processed = match args.polarization {
                    Polarization::Vv => reader.vv_data()?,
                    Polarization::Vh => reader.vh_data()?,
                    Polarization::Hh => reader.hh_data()?,
                    Polarization::Hv => reader.hv_data()?,
                    _ => unreachable!(),
                };
                save_processed_image(
                    &processed,
                    output.as_path(),
                    args.format,
                    bit_depth_enum,
                    target_size,
                    Some(reader.metadata()),
                    args.pad,
                    args.autoscale,
                    ProcessingOperation::SingleBand,
                )?
            }
            Polarization::Multiband => {
                if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                    let vv_processed = reader.vv_data()?;
                    let vh_processed = reader.vh_data()?;
                    save_processed_multiband_image_sequential(
                        &vv_processed,
                        &vh_processed,
                        output.as_path(),
                        args.format,
                        bit_depth_enum,
                        target_size,
                        Some(reader.metadata()),
                        args.pad,
                        args.autoscale,
                        ProcessingOperation::MultibandVvVh,
                        args.synrgb_mode,
                    )?
                } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                    let hh_processed = reader.hh_data()?;
                    let hv_processed = reader.hv_data()?;
                    save_processed_multiband_image_sequential(
                        &hh_processed,
                        &hv_processed,
                        output.as_path(),
                        args.format,
                        bit_depth_enum,
                        target_size,
                        Some(reader.metadata()),
                        args.pad,
                        args.autoscale,
                        ProcessingOperation::MultibandHhHv,
                        args.synrgb_mode,
                    )?
                } else {
                    return Err(AppError::IncompleDataPair { operation: "multiband".to_string(), available: reader.get_available_polarizations() }.into());
                }
            }
            Polarization::OP(op) => {
                let processed = if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
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
                    return Err(AppError::IncompleDataPair { operation: args.polarization.to_string(), available: reader.get_available_polarizations() }.into());
                };
                save_processed_image(
                    &processed,
                    output.as_path(),
                    args.format,
                    bit_depth_enum,
                    target_size,
                    Some(reader.metadata()),
                    args.pad,
                    args.autoscale,
                    ProcessingOperation::PolarOp(op),
                )?
            }
        }
        info!("Successfully processed remote measurements -> {:?}", output);
        return Ok(());
    }

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
                    args.synrgb_mode,
                    &proc_opts,
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
            args.synrgb_mode,
            &proc_opts,
        )?;
        info!("Successfully processed: {:?} -> {:?}\n", input, output);
    }

    Ok(())
}
