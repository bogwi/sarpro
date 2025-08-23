use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tracing::{info, warn};
use gdal::raster::ResampleAlg;
use sarpro::core::processing::save::{
    save_processed_image, save_processed_multiband_image_sequential,
};
use sarpro::io::sentinel1::TargetCrsArg;
use sarpro::io::gdal::to_gdal_path;
use sarpro::io::SafeReader;
use sarpro::types::{BitDepth, OutputFormat, ProcessingOperation};
use sarpro::{BitDepthArg, Polarization, PolarizationOperation};

use crate::cli::args::CliArgs;
use crate::cli::errors::AppError;

pub fn process_safe_zip_url(args: &CliArgs) -> Result<(), Box<dyn std::error::Error>> {
    let zip_url = args.safe_zip_url.as_ref().unwrap();
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
    Ok(())
}

pub fn process_safe_zip_dir_url(args: &CliArgs) -> Result<(), Box<dyn std::error::Error>> {
    let dir_url = args.safe_zip_dir_url.as_ref().unwrap();

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
                                Err(sarpro::io::SafeError::MissingField("polarization pair for operation"))
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
    Ok(())
}

pub fn process_measurement_urls(args: &CliArgs) -> Result<(), Box<dyn std::error::Error>> {
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
    Ok(())
}
