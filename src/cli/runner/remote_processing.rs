use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};
use gdal::raster::ResampleAlg;
use sarpro::core::processing::save::{
    save_processed_image, save_processed_multiband_image_sequential,
};
use sarpro::io::sentinel1::TargetCrsArg;
use sarpro::io::gdal::to_gdal_path;
use sarpro::io::SafeReader;
use sarpro::types::{AutoscaleStrategy, BitDepth, OutputFormat, ProcessingOperation, SyntheticRgbMode};
use sarpro::{BitDepthArg, Polarization, PolarizationOperation};

use crate::cli::args::CliArgs;
use crate::cli::errors::AppError;

/// Helper function to process and save data based on polarization type.
/// Eliminates code duplication by replacing repetitive match statements with conditional logic.
fn resolve_polarization_and_save_image(
    reader: &SafeReader,
    polarization: Polarization,
    output_path: &Path,
    format: OutputFormat,
    bit_depth: BitDepth,
    target_size: Option<usize>,
    synrgb_mode: SyntheticRgbMode,
    pad: bool,
    autoscale: AutoscaleStrategy,
) -> Result<(), Box<dyn std::error::Error>> {
    // Handle single band polarizations without match statements
    if polarization == Polarization::Vv || polarization == Polarization::Vh ||
       polarization == Polarization::Hh || polarization == Polarization::Hv {
        let processed = if polarization == Polarization::Vv {
            reader.vv_data()?
        } else if polarization == Polarization::Vh {
            reader.vh_data()?
        } else if polarization == Polarization::Hh {
            reader.hh_data()?
        } else if polarization == Polarization::Hv {
            reader.hv_data()?
        } else {
            unreachable!()
        };

        save_processed_image(
            &processed,
            output_path,
            format,
            bit_depth,
            target_size,
            Some(reader.metadata()),
            pad,
            autoscale,
            ProcessingOperation::SingleBand,
        )?
    }
    // Handle multiband processing
    else if polarization == Polarization::Multiband {
        if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
            let vv_processed = reader.vv_data()?;
            let vh_processed = reader.vh_data()?;
            save_processed_multiband_image_sequential(
                &vv_processed,
                &vh_processed,
                output_path,
                format,
                bit_depth,
                target_size,
                Some(reader.metadata()),
                pad,
                autoscale,
                ProcessingOperation::MultibandVvVh,
                synrgb_mode,
            )?
        } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
            let hh_processed = reader.hh_data()?;
            let hv_processed = reader.hv_data()?;
            save_processed_multiband_image_sequential(
                &hh_processed,
                &hv_processed,
                output_path,
                format,
                bit_depth,
                target_size,
                Some(reader.metadata()),
                pad,
                autoscale,
                ProcessingOperation::MultibandHhHv,
                synrgb_mode,
            )?
                    } else {
                return Err(Box::new(AppError::IncompleDataPair {
                    operation: "multiband".to_string(),
                    available: reader.get_available_polarizations(),
                }));
            }
    }
    // Handle polarization operations
    else if let Polarization::OP(op) = polarization {
        let processed = if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
            if op == PolarizationOperation::Sum {
                reader.sum_data()?
            } else if op == PolarizationOperation::Diff {
                reader.difference_data()?
            } else if op == PolarizationOperation::Ratio {
                reader.ratio_data()?
            } else if op == PolarizationOperation::NDiff {
                reader.normalized_diff_data()?
            } else if op == PolarizationOperation::LogRatio {
                reader.log_ratio_data()?
            } else {
                unreachable!()
            }
        } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
            if op == PolarizationOperation::Sum {
                reader.sum_hh_hv_data()?
            } else if op == PolarizationOperation::Diff {
                reader.difference_hh_hv_data()?
            } else if op == PolarizationOperation::Ratio {
                reader.ratio_hh_hv_data()?
            } else if op == PolarizationOperation::NDiff {
                reader.normalized_diff_hh_hv_data()?
            } else if op == PolarizationOperation::LogRatio {
                reader.log_ratio_hh_hv_data()?
            } else {
                unreachable!()
            }
                    } else {
                return Err(Box::new(AppError::IncompleDataPair {
                    operation: polarization.to_string(),
                    available: reader.get_available_polarizations(),
                }));
            };

        save_processed_image(
            &processed,
            output_path,
            format,
            bit_depth,
            target_size,
            Some(reader.metadata()),
            pad,
            autoscale,
            ProcessingOperation::PolarOp(op),
        )?
    }
    Ok(())
}

// Public-only headers provider for Step 3; Step 3.1 will plug real credentials
trait HeadersProvider {
    fn headers_for(&self, _url: &str) -> Vec<(String, String)> { Vec::new() }
}

struct NoAuthHeaders;
impl HeadersProvider for NoAuthHeaders {}

fn materialize_zip_if_requested(
    zip_href: &str,
    cache_dir: &Option<PathBuf>,
    http_timeout_s: u64,
    http_retries: u8,
    http_backoff_ms: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    // If no cache dir requested, return original href (stream via VSIZIP+VSICURL)
    if cache_dir.is_none() {
        return Ok(zip_href.to_string());
    }
    let cache_dir = cache_dir.as_ref().unwrap();
    fs::create_dir_all(cache_dir)?;

    // Derive a filename from the URL/path last segment
    let fname = if zip_href.contains('/') { zip_href.rsplit('/').next().unwrap_or("remote.SAFE.zip") } else { "remote.SAFE.zip" };
    let local_path = cache_dir.join(fname);

    // If the href is already a local path, just copy/link it
    if Path::new(zip_href).exists() {
        // If destination exists, reuse; otherwise copy
        if !local_path.exists() {
            fs::copy(zip_href, &local_path)?;
        }
        return Ok(local_path.to_string_lossy().to_string());
    }

    // Use reqwest blocking client with simple retries and atomic write
    let client = reqwest::blocking::Client::builder()
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .timeout(std::time::Duration::from_secs(http_timeout_s))
        .build()
        .map_err(|e| AppError::RemoteDirListing { url: zip_href.to_string(), hint: format!("http client build error: {}", e) })?;

    // Sidecars and resume state
    let partial = local_path.with_extension("partial");
    let etag_sidecar = local_path.with_extension("etag");

    // Preflight HEAD for Content-Length/ETag (best-effort)
    let mut head_req = client.head(zip_href);
    for (k, v) in NoAuthHeaders.headers_for(zip_href) { head_req = head_req.header(&k, &v); }
    let (content_len_opt, etag_opt, accept_ranges_bytes) = match head_req.send() {
        Ok(resp) if resp.status().is_success() => {
            (
                resp.headers().get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok()),
                resp.headers().get(reqwest::header::ETAG)
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string()),
                resp.headers().get(reqwest::header::ACCEPT_RANGES)
                    .and_then(|h| h.to_str().ok())
                    .map(|v| v.eq_ignore_ascii_case("bytes")).unwrap_or(false),
            )
        }
        _ => (None, None, false),
    };

    // If complete file exists and content length matches (when known), reuse
    if local_path.exists() {
        if let Ok(md) = std::fs::metadata(&local_path) {
            if let Some(cl) = content_len_opt {
                if md.len() == cl {
                    info!("Cache hit: using materialized ZIP {:?} ({} bytes)", local_path, md.len());
                    return Ok(local_path.to_string_lossy().to_string());
                }
            } else {
                // No known length; assume present is valid
                info!("Cache hit: using materialized ZIP {:?}", local_path);
                return Ok(local_path.to_string_lossy().to_string());
            }
        }
    }

    // Determine resume position
    let mut resume_from: u64 = 0;
    if partial.exists() && accept_ranges_bytes {
        if let Ok(md) = std::fs::metadata(&partial) { resume_from = md.len(); }
        // ETag guard: if sidecar exists and new ETag differs, restart
        if etag_sidecar.exists() {
            if let (Ok(old), Some(new)) = (std::fs::read_to_string(&etag_sidecar), etag_opt.as_ref()) {
                if old.trim() != new.trim() {
                    info!("Partial invalidated due to ETag change; restarting download for {}", zip_href);
                    let _ = std::fs::remove_file(&partial);
                    let _ = std::fs::remove_file(&etag_sidecar);
                    resume_from = 0;
                }
            }
        }
    } else {
        // Clean stale partial if cannot resume
        let _ = std::fs::remove_file(&partial);
        resume_from = 0;
    }

    // Store ETag sidecar for future resume
    if let Some(ref et) = etag_opt {
        let _ = std::fs::write(&etag_sidecar, et);
    }

    // Prepare destination file (append for resume, overwrite otherwise)
    let mut file = if resume_from > 0 { 
        std::fs::OpenOptions::new().create(true).append(true).open(&partial)
    } else {
        std::fs::File::create(&partial)
    }.map_err(|e| AppError::RemoteDirListing { url: zip_href.to_string(), hint: format!("open partial error: {}", e) })?;

    if resume_from > 0 {
        info!("Resuming download: {} from {} bytes", zip_href, resume_from);
    } else {
        info!("Downloading to cache: {} -> {:?}", zip_href, partial);
    }

    // GET with retries/backoff and optional Range for resume
    let mut attempts = 0u8;
    let max_retries = http_retries.max(1);
    loop {
        attempts += 1;
        let mut req = client.get(zip_href);
        // no-auth headers for Step 3 (empty)
        for (k, v) in NoAuthHeaders.headers_for(zip_href) { req = req.header(&k, &v); }
        if resume_from > 0 { req = req.header(reqwest::header::RANGE, format!("bytes={}-", resume_from)); }
        let resp = req.send();
        match resp {
            Ok(r) if r.status().is_success() => {
                let mut body = r
                    .error_for_status()
                    .map_err(|e| AppError::RemoteDirListing { url: zip_href.to_string(), hint: format!("http status error: {}", e) })?;
                let copied = std::io::copy(&mut body, &mut file)
                    .map_err(|e| AppError::RemoteDirListing { url: zip_href.to_string(), hint: format!("write error: {}", e) })?;
                let _ = file.sync_all();
                // Verify length when known
                if let Some(total) = content_len_opt { 
                    let have = std::fs::metadata(&partial).ok().map(|m| m.len()).unwrap_or(0);
                    if have != total { 
                        // allow retry
                        if attempts < max_retries { std::thread::sleep(std::time::Duration::from_millis(http_backoff_ms * attempts as u64)); continue; }
                        return Err(AppError::RemoteDirListing { url: zip_href.to_string(), hint: format!("size mismatch: have {}, want {} (copied {})", have, total, copied) }.into());
                    }
                }
                std::fs::rename(&partial, &local_path)
                    .map_err(|e| AppError::RemoteDirListing { url: zip_href.to_string(), hint: format!("rename error: {}", e) })?;
                let final_len = std::fs::metadata(&local_path).ok().map(|m| m.len()).unwrap_or(0);
                info!("Cached ZIP ready: {:?} ({} bytes)", local_path, final_len);
                return Ok(local_path.to_string_lossy().to_string());
            }
            Ok(r) if r.status().as_u16() >= 500 || r.status().as_u16() == 429 => {
                if attempts < max_retries { std::thread::sleep(std::time::Duration::from_millis(http_backoff_ms * attempts as u64)); continue; }
                return Err(AppError::RemoteDirListing { url: zip_href.to_string(), hint: format!("http error {} after retries", r.status()) }.into());
            }
            Ok(r) => {
                return Err(AppError::RemoteDirListing { url: zip_href.to_string(), hint: format!("http error {}", r.status()) }.into());
            }
            Err(e) => {
                if attempts < max_retries { std::thread::sleep(std::time::Duration::from_millis(http_backoff_ms * attempts as u64)); continue; }
                return Err(AppError::RemoteDirListing { url: zip_href.to_string(), hint: format!("network error: {}", e) }.into());
            }
        }
    }
}

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

    // Decide streaming vs materialization based on --remote-cache-dir
    let href = materialize_zip_if_requested(
        zip_url,
        &args.remote_cache_dir,
        args.http_timeout,
        args.http_retries as u8,
        args.http_backoff_ms,
    )?;

    let reader = SafeReader::open_from_safe_zip_url(
        &href,
        args.polarization,
        target_arg,
        resample_alg,
        target_size,
    )?;

    // Dispatch save using the unified helper function
    let bit_depth_enum = match args.bit_depth { BitDepthArg::U8 => BitDepth::U8, BitDepthArg::U16 => BitDepth::U16 };
    resolve_polarization_and_save_image(
        &reader,
        args.polarization,
        output.as_path(),
        args.format,
        bit_depth_enum,
        target_size,
        args.synrgb_mode,
        args.pad,
        args.autoscale,
    )?;
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
        let href = materialize_zip_if_requested(
            &zip_url,
            &args.remote_cache_dir,
            args.http_timeout,
            args.http_retries as u8,
            args.http_backoff_ms,
        )?;
        let safe_stem = zip_name.trim_end_matches(".SAFE.zip");
        let out_name = format!("{}.{}", safe_stem, match args.format { OutputFormat::TIFF => "tiff", OutputFormat::JPEG => "jpg" });
        let out_path = output_dir.join(out_name);

        info!("Processing remote SAFE ZIP: {} -> {:?}", href, out_path);
        let reader_res = SafeReader::open_from_safe_zip_url(
            &href,
            args.polarization,
            target_arg.clone(),
            resample_alg,
            target_size,
        );

        match reader_res {
            Ok(reader) => {
                let bit_depth_enum = match args.bit_depth { BitDepthArg::U8 => BitDepth::U8, BitDepthArg::U16 => BitDepth::U16 };
                let save_res = resolve_polarization_and_save_image(
                    &reader,
                    args.polarization,
                    out_path.as_path(),
                    args.format,
                    bit_depth_enum,
                    target_size,
                    args.synrgb_mode,
                    args.pad,
                    args.autoscale,
                );
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
    resolve_polarization_and_save_image(
        &reader,
        args.polarization,
        output.as_path(),
        args.format,
        bit_depth_enum,
        target_size,
        args.synrgb_mode,
        args.pad,
        args.autoscale,
    )?;
    info!("Successfully processed remote measurements -> {:?}", output);
    Ok(())
}
