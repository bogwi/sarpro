use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn, debug};
use gdal::raster::ResampleAlg;
use sarpro::core::processing::save::{
    save_processed_image, save_processed_multiband_image_sequential,
};
use sarpro::io::sentinel1::TargetCrsArg;
use sarpro::io::gdal::to_gdal_path;
use sarpro::io::SafeReader;
use sarpro::io::credentials::{CdseProvider, AsfProvider, CredentialProvider};
use sarpro::io::cache::{CacheEntry, load_index, save_index, get_url as cache_get_url, get_alias as cache_get_alias, put_url as cache_put_url, put_alias as cache_put_alias};
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

fn materialize_zip_if_requested(
    zip_href: &str,
    cache_dir: &Option<PathBuf>,
    http_timeout_s: u64,
    http_retries: u8,
    http_backoff_ms: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let is_cdse = zip_href.contains("download.dataspace.copernicus.eu") && zip_href.contains("/odata/") && zip_href.contains("/Products(");
    let is_asf = zip_href.contains("asf.alaska.edu");

    // If no cache dir requested, return original href (stream via VSIZIP+VSICURL)
    if cache_dir.is_none() {
        // Prepare provider envs for streaming when needed
        if is_asf {
            let mut asf = AsfProvider::new(http_timeout_s);
            let _ = asf.prepare();
        }
        if is_cdse {
            let mut cdse = CdseProvider::new(http_timeout_s);
            let _ = cdse.prepare();
        }
        return Ok(zip_href.to_string());
    }
    let cache_dir = cache_dir.as_ref().unwrap();
    fs::create_dir_all(cache_dir)?;

    // Load cache index
    let mut index = load_index(cache_dir);

    let cdse_uuid: Option<String> = if is_cdse {
        // extract between Products( and )
        let mut uuid_opt: Option<String> = None;
        if let Some(start_idx) = zip_href.find("Products(") {
            let s = &zip_href[start_idx + 9..];
            if let Some(end_idx) = s.find(')') {
                uuid_opt = Some(s[..end_idx].to_string());
            }
        }
        uuid_opt
    } else { None };

    // Normalize CDSE download URL to include /$value
    let download_url: String = if is_cdse {
        if zip_href.ends_with("/$value") { zip_href.to_string() } else { format!("{}/$value", zip_href.trim_end_matches('/')) }
    } else {
        zip_href.to_string()
    };

    // Quick cache hit by URL
    if let Some(entry) = cache_get_url(&index, zip_href) {
        if entry.local_path.exists() {
            info!("Cache hit (by URL): using materialized ZIP {:?}", entry.local_path);
            return Ok(entry.local_path.to_string_lossy().to_string());
        }
    }
    // Also check normalized download URL
    if let Some(entry) = cache_get_url(&index, &download_url) {
        if entry.local_path.exists() {
            info!("Cache hit (by normalized URL): using materialized ZIP {:?}", entry.local_path);
            return Ok(entry.local_path.to_string_lossy().to_string());
        }
    }
    // Quick cache hit by CDSE UUID alias
    if let Some(uuid) = cdse_uuid.as_ref() {
        if let Some(entry) = cache_get_alias(&index, "cdse", uuid) {
            if entry.local_path.exists() {
                info!("Cache hit (by alias cdse:{}): using materialized ZIP {:?}", uuid, entry.local_path);
                return Ok(entry.local_path.to_string_lossy().to_string());
            }
        }
    }

    // Build HTTP client (used mainly for CDSE OData)
    let client = reqwest::blocking::Client::builder()
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(http_timeout_s))
        .build()
        .map_err(|e| AppError::RemoteDirListing { url: zip_href.to_string(), hint: format!("http client build error: {}", e) })?;

    // Prepare CredentialProvider for CDSE
    let mut cdse = if is_cdse { Some(CdseProvider::new(http_timeout_s)) } else { None };
    let mut asf = if is_asf { Some(AsfProvider::new(http_timeout_s)) } else { None };
    if let Some(provider) = cdse.as_mut() {
        // Best-effort prepare; if missing creds, HEAD/GET may fail and will surface clearly
        let _ = provider.prepare();
    }
    if let Some(provider) = asf.as_mut() {
        let _ = provider.prepare();
    }

    // Preflight: discover Content-Length and filename in a provider-agnostic way
    fn parse_disposition_filename(header_val: &str) -> Option<String> {
        let mut fname: Option<String> = None;
        for part in header_val.split(';') {
            let p = part.trim();
            if let Some(eq) = p.find("filename=") {
                let val = p[eq+9..].trim().trim_matches('"');
                if !val.is_empty() { fname = Some(val.to_string()); break; }
            }
        }
        fname
    }

    let mut content_len_opt: Option<u64> = None;
    let mut cd_filename_opt: Option<String> = None;

    // A) ASF: use curl with netrc/cookies to get headers deterministically
    if is_asf {
        if let Some(p) = asf.as_ref() {
            // Try HEAD first
            info!("ASF preflight HEAD: {}", download_url);
            let head_out = Command::new("curl")
                .arg("-I").arg("-L").arg("-sS")
                .arg("--connect-timeout").arg(format!("{}", http_timeout_s))
                .arg("--netrc-file").arg(p.netrc_path.as_os_str())
                .arg("-c").arg(p.cookie_jar.as_os_str())
                .arg("-b").arg(p.cookie_file.as_os_str())
                .arg(&download_url)
                .output();
            if let Ok(o) = head_out {
                let headers = String::from_utf8_lossy(&o.stdout);
                for line in headers.lines() {
                    let l = line.trim();
                    if l.to_ascii_lowercase().starts_with("content-length:") {
                        if let Some(v) = l.splitn(2, ':').nth(1) {
                            if let Ok(val) = v.trim().parse::<u64>() {
                                if val > 0 { content_len_opt = Some(val); } else { content_len_opt = None; }
                            }
                        }
                    } else if l.to_ascii_lowercase().starts_with("content-disposition:") {
                        if let Some(v) = l.splitn(2, ':').nth(1) { cd_filename_opt = parse_disposition_filename(v.trim()); }
                    }
                }
                if let Some(len) = content_len_opt { info!("ASF HEAD: Content-Length={}", len); }
                if let Some(ref fname) = cd_filename_opt { info!("ASF HEAD: Content-Disposition filename={}", fname); }
            }
            // If still no size, try 0-0 range to pick Content-Range total
            if content_len_opt.is_none() {
                info!("ASF preflight RANGE 0-0: {}", download_url);
                let range_out = Command::new("curl")
                    .arg("-L").arg("-sS")
                    .arg("--connect-timeout").arg(format!("{}", http_timeout_s))
                    .arg("-r").arg("0-0")
                    .arg("-D").arg("-")
                    .arg("-o").arg("/dev/null")
                    .arg("--netrc-file").arg(p.netrc_path.as_os_str())
                    .arg("-c").arg(p.cookie_jar.as_os_str())
                    .arg("-b").arg(p.cookie_file.as_os_str())
                    .arg(&download_url)
                    .output();
                if let Ok(o) = range_out {
                    let headers = String::from_utf8_lossy(&o.stdout);
                    for line in headers.lines() {
                        let l = line.trim();
                        if l.to_ascii_lowercase().starts_with("content-range:") {
                            if let Some(v) = l.splitn(2, ':').nth(1) {
                                let s = v.trim();
                                if let Some(total) = s.rsplit('/').next() { content_len_opt = total.trim().parse::<u64>().ok(); }
                            }
                        } else if l.to_ascii_lowercase().starts_with("content-disposition:") {
                            if let Some(v) = l.splitn(2, ':').nth(1) { if cd_filename_opt.is_none() { cd_filename_opt = parse_disposition_filename(v.trim()); } }
                        }
                    }
                    if let Some(len) = content_len_opt { info!("ASF RANGE: Content-Length={}", len); }
                    if let Some(ref fname) = cd_filename_opt { info!("ASF RANGE: Content-Disposition filename={}", fname); }
                }
            }
        }
    }

    // B) CDSE: reqwest range probe with Authorization
    if is_cdse {
        let mut probe_req = client.get(&download_url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .header(reqwest::header::ACCEPT_ENCODING, "identity");
        if let Some(provider) = cdse.as_ref() {
            if let Ok(hmap) = provider.headers_for(&download_url) {
                for (k, v) in hmap { probe_req = probe_req.header(k, v); }
            }
        }
        if let Ok(resp) = probe_req.send() {
            if resp.status().is_success() {
                content_len_opt = content_len_opt.or_else(|| resp.headers().get(reqwest::header::CONTENT_RANGE)
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| {
                        let parts: Vec<&str> = s.split('/').collect();
                        if parts.len() == 2 { parts[1].parse::<u64>().ok() } else { None }
                    }))
                    .or_else(|| resp.headers().get(reqwest::header::CONTENT_LENGTH)
                        .and_then(|h| h.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok()));
                if cd_filename_opt.is_none() {
                    cd_filename_opt = resp.headers().get(reqwest::header::CONTENT_DISPOSITION)
                        .or_else(|| resp.headers().get("content-disposition"))
                        .and_then(|h| h.to_str().ok())
                        .and_then(|cd| parse_disposition_filename(cd));
                }
            }
        }
        // If still missing name/length, try OData select
        if (cd_filename_opt.is_none() || content_len_opt.is_none()) && cdse_uuid.is_some() {
            if let Some(provider) = cdse.as_ref() {
                let base_meta = if download_url.ends_with("/$value") { &download_url[..download_url.len()-"/$value".len()] } else { zip_href };
                let meta_url = format!("{}?$select=Name,ContentLength", base_meta);
                let mut req = client.get(&meta_url);
                if let Ok(hmap) = provider.headers_for(&meta_url) { for (k, v) in hmap { req = req.header(k, v); } }
                if let Ok(resp) = req.send() {
                    if resp.status().is_success() {
                        if let Ok(json) = resp.json::<serde_json::Value>() {
                            if cd_filename_opt.is_none() {
                                if let Some(name) = json.get("Name").and_then(|v| v.as_str()) {
                                    let mut fname = name.to_string();
                                    if fname.to_lowercase().ends_with(".safe.zip") { cd_filename_opt = Some(fname); }
                                    else if fname.to_lowercase().ends_with(".safe") { fname.push_str(".zip"); cd_filename_opt = Some(fname); }
                                }
                            }
                            if content_len_opt.is_none() {
                                if let Some(len) = json.get("ContentLength").and_then(|v| v.as_u64()) { content_len_opt = Some(len); }
                            }
                        }
                    }
                }
            }
        }
    }

    debug!("content_len_opt: {:?}, cd_filename_opt: {:?}", content_len_opt, cd_filename_opt);

    // If CDSE and no Content-Disposition filename, try OData metadata Name
    let mut canonical_name: Option<String> = cd_filename_opt;
    if canonical_name.is_none() {
        if cdse_uuid.is_some() {
            if let Some(provider) = cdse.as_ref() {
                let base_meta = if download_url.ends_with("/$value") { &download_url[..download_url.len()-"/$value".len()] } else { zip_href };
                let meta_url = format!("{}?$select=Name", base_meta);
                let mut req = client.get(&meta_url);
                if let Ok(hmap) = provider.headers_for(&meta_url) {
                    for (k, v) in hmap { req = req.header(k, v); }
                }
                if let Ok(resp) = req.send() {
                    if resp.status().is_success() {
                        if let Ok(json) = resp.json::<serde_json::Value>() {
                            if let Some(name) = json.get("Name").and_then(|v| v.as_str()) {
                                let mut fname = name.to_string();
                                if fname.to_lowercase().ends_with(".safe.zip") {
                                    canonical_name = Some(fname);
                                } else if fname.to_lowercase().ends_with(".safe") {
                                    fname.push_str(".zip");
                                    canonical_name = Some(fname);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback canonical name
    if canonical_name.is_none() {
        if let Some(uuid) = cdse_uuid.as_ref() {
            canonical_name = Some(format!("{}.zip", uuid));
        } else {
            // Derive from URL tail as last resort
            let tail_opt = if zip_href.contains('/') { Some(zip_href.rsplit('/').next().unwrap_or("remote.SAFE.zip")) } else { None };
            let fname = match tail_opt { Some(t) if !t.is_empty() => t.to_string(), _ => "remote.SAFE.zip".to_string() };
            canonical_name = Some(fname);
        }
    }
    if canonical_name.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        canonical_name = Some("remote.SAFE.zip".to_string());
    }
    let local_path = cache_dir.join(canonical_name.as_ref().unwrap());

    // If the href is already a local path, just copy/link it
    if Path::new(zip_href).exists() {
        // If destination exists, reuse; otherwise copy
        if !local_path.exists() {
            fs::copy(zip_href, &local_path)?;
        }
        // Update index (by URL and alias if applicable)
        let entry = CacheEntry { local_path: local_path.clone(), etag: None, content_length: None };
        cache_put_url(&mut index, zip_href, entry.clone());
        cache_put_url(&mut index, &download_url, entry.clone());
        if let Some(uuid) = cdse_uuid.as_ref() { cache_put_alias(&mut index, "cdse", uuid, entry.clone()); }
        save_index(cache_dir, &index);
        return Ok(local_path.to_string_lossy().to_string());
    }

    // If the canonical file already exists and (when known) size matches, reuse
    // Pre-download cache hit using the canonical filename. It verifies size when HEAD provided Content-Length, updates the cache index (by original URL, normalized $value URL, and cdse:UUID alias), logs the hit, and returns the local path without downloading.
    if local_path.exists() {
        if let Ok(md) = std::fs::metadata(&local_path) {
            if let Some(cl) = content_len_opt {
                if md.len() == cl {
                    let entry = CacheEntry { local_path: local_path.clone(), etag: None, content_length: content_len_opt };
                    cache_put_url(&mut index, zip_href, entry.clone());
                    cache_put_url(&mut index, &download_url, entry.clone());
                    if let Some(uuid) = cdse_uuid.as_ref() { cache_put_alias(&mut index, "cdse", uuid, entry.clone()); }
                    save_index(cache_dir, &index);
                    info!("Cache hit: using materialized ZIP {:?} ({} bytes)", local_path, md.len());
                    return Ok(local_path.to_string_lossy().to_string());
                }
            } else {
                let entry = CacheEntry { local_path: local_path.clone(), etag: None, content_length: content_len_opt };
                cache_put_url(&mut index, zip_href, entry.clone());
                cache_put_url(&mut index, &download_url, entry.clone());
                if let Some(uuid) = cdse_uuid.as_ref() { cache_put_alias(&mut index, "cdse", uuid, entry.clone()); }
                save_index(cache_dir, &index);
                info!("Cache hit: using materialized ZIP {:?}", local_path);
                return Ok(local_path.to_string_lossy().to_string());
            }
        }
    }

    // Prepare partial path
    let partial = local_path.with_extension("partial");

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

    // Create/truncate partial only for reqwest path. For ASF curl path we must not truncate to enable resume.
    if !is_asf {
        let _f = std::fs::File::create(&partial)
            .map_err(|e| AppError::RemoteDirListing { url: zip_href.to_string(), hint: format!("open partial error: {}", e) })?;
    }
    info!("Downloading to cache: {} -> {:?}", zip_href, partial);

    // GET with retries/backoff and optional Range for resume
    let mut attempts = 0u8;
    let max_retries = http_retries.max(1);
    loop {
        attempts += 1;
        if is_asf {
            // Use curl with netrc and cookie jar for ASF, enable resume and avoid overall max-time
            let mut cmd = Command::new("curl");
            let resume_from = std::fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
            info!("ASF curl download: resume at {} of {:?}", resume_from, content_len_opt);
            cmd.arg("-L")
                .arg("--fail")
                .arg("-sS")
                .arg("--connect-timeout").arg(format!("{}", http_timeout_s))
                .arg("--netrc-file").arg(asf.as_ref().unwrap().netrc_path.as_os_str())
                .arg("-c").arg(asf.as_ref().unwrap().cookie_jar.as_os_str())
                .arg("-b").arg(asf.as_ref().unwrap().cookie_file.as_os_str())
                .arg("-C").arg("-") // resume from current size if exists
                .arg("--retry").arg(format!("{}", http_retries.max(1)))
                .arg("--retry-all-errors")
                .arg("--retry-delay").arg(format!("{}", (http_backoff_ms / 1000).max(1)))
                .arg("-o").arg(&partial)
                .arg(&download_url);
            let status = cmd.status();
            match status {
                Ok(s) if s.success() => {
                    std::fs::rename(&partial, &local_path)
                        .map_err(|e| AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("rename error: {}", e) })?;
                    let final_len = std::fs::metadata(&local_path).ok().map(|m| m.len()).unwrap_or(0);
                    info!("Cached ZIP ready: {:?} ({} bytes)", local_path, final_len);
                    let entry = CacheEntry { local_path: local_path.clone(), etag: None, content_length: None };
                    cache_put_url(&mut index, zip_href, entry.clone());
                    cache_put_url(&mut index, &download_url, entry.clone());
                    save_index(cache_dir, &index);
                    return Ok(local_path.to_string_lossy().to_string());
                }
                Ok(s) => {
                    warn!("ASF curl exited with status {}. Partial may exist at {:?}", s, partial);
                    if attempts < max_retries { std::thread::sleep(std::time::Duration::from_millis(http_backoff_ms * attempts as u64)); continue; }
                    return Err(AppError::RemoteDirListing { url: download_url.to_string(), hint: "curl download failed for ASF after retries".to_string() }.into());
                }
                Err(e) => {
                    warn!("ASF curl failed to start: {}", e);
                    if attempts < max_retries { std::thread::sleep(std::time::Duration::from_millis(http_backoff_ms * attempts as u64)); continue; }
                    return Err(AppError::RemoteDirListing { url: download_url.to_string(), hint: "curl download failed for ASF after retries".to_string() }.into());
                }
            }
        } else {
            // Simple GET with redirects handled by client (CDSE and generic)
            let mut req = client.get(&download_url)
                .header(reqwest::header::ACCEPT_ENCODING, "identity");
            if let Some(provider) = cdse.as_ref() {
                if let Ok(hmap) = provider.headers_for(&download_url) {
                    for (k, v) in hmap { req = req.header(k, v); }
                }
            }
            let resp = req.send();
            match resp {
                Ok(r) if r.status().is_success() => {
                    let mut file = std::fs::File::create(&partial)
                        .map_err(|e| AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("open partial error: {}", e) })?;
                    let mut body = r
                        .error_for_status()
                        .map_err(|e| AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("http status error: {}", e) })?;
                    let copied = std::io::copy(&mut body, &mut file)
                        .map_err(|e| AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("write error: {}", e) })?;
                    let _ = file.sync_all();
                    // Verify length when known
                    if let Some(total) = content_len_opt {
                        let have = std::fs::metadata(&partial).ok().map(|m| m.len()).unwrap_or(0);
                        if have != total {
                            // allow retry
                            if attempts < max_retries { std::thread::sleep(std::time::Duration::from_millis(http_backoff_ms * attempts as u64)); continue; }
                            return Err(AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("size mismatch: have {}, want {} (copied {})", have, total, copied) }.into());
                        }
                    }
                    std::fs::rename(&partial, &local_path)
                        .map_err(|e| AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("rename error: {}", e) })?;
                    let final_len = std::fs::metadata(&local_path).ok().map(|m| m.len()).unwrap_or(0);
                    info!("Cached ZIP ready: {:?} ({} bytes)", local_path, final_len);
                    // Update index with URL and alias
                    let entry = CacheEntry { local_path: local_path.clone(), etag: None, content_length: content_len_opt };
                    cache_put_url(&mut index, zip_href, entry.clone());
                    cache_put_url(&mut index, &download_url, entry.clone());
                    if let Some(uuid) = cdse_uuid.as_ref() { cache_put_alias(&mut index, "cdse", uuid, entry.clone()); }
                    save_index(cache_dir, &index);
                    return Ok(local_path.to_string_lossy().to_string());
                }
                Ok(r) if r.status().as_u16() >= 500 || r.status().as_u16() == 429 => {
                    if attempts < max_retries { std::thread::sleep(std::time::Duration::from_millis(http_backoff_ms * attempts as u64)); continue; }
                    return Err(AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("http error {} after retries", r.status()) }.into());
                }
                Ok(r) => {
                    return Err(AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("http error {}", r.status()) }.into());
                }
                Err(e) => {
                    if attempts < max_retries { std::thread::sleep(std::time::Duration::from_millis(http_backoff_ms * attempts as u64)); continue; }
                    return Err(AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("network error: {}", e) }.into());
                }
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

    // Ensure ASF cookies/envs are prepared for streaming opens
    if args.remote_cache_dir.is_none() && zip_url.contains("asf.alaska.edu") {
        let mut asf = AsfProvider::new(args.http_timeout);
        let _ = asf.prepare();
    }

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
