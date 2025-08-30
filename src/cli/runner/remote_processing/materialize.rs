use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn, debug};
use indicatif::{ProgressBar, ProgressStyle};

use crate::cli::errors::AppError;

use sarpro::io::credentials::{CdseProvider, AsfProvider, CredentialProvider};
use sarpro::io::cache::{CacheEntry, load_index, save_index, get_url as cache_get_url, get_alias as cache_get_alias, put_url as cache_put_url, put_alias as cache_put_alias};

pub fn materialize_zip(
    zip_href: &str,
    cache_dir: &Option<PathBuf>,
    http_timeout_s: u64,
    http_retries: u8,
    http_backoff_ms: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let is_cdse = zip_href.contains("download.dataspace.copernicus.eu") && zip_href.contains("/odata/") && zip_href.contains("/Products(");
    let is_asf = zip_href.contains("asf.alaska.edu");

    if cache_dir.is_none() {
        return Err(AppError::CacheRequired {
            url: zip_href.to_string(),
            hint: "Use --remote-cache-dir <PATH_TO_CACHE> to materialize downloads.".to_string(),
        }.into());
    }
    let cache_dir = cache_dir.as_ref().unwrap();
    std::fs::create_dir_all(cache_dir)?;

    let mut index = load_index(cache_dir);

    let cdse_uuid: Option<String> = if is_cdse {
        let mut uuid_opt: Option<String> = None;
        if let Some(start_idx) = zip_href.find("Products(") {
            let s = &zip_href[start_idx + 9..];
            if let Some(end_idx) = s.find(')') {
                uuid_opt = Some(s[..end_idx].to_string());
            }
        }
        uuid_opt
    } else { None };

    let download_url: String = if is_cdse {
        if zip_href.ends_with("/$value") { zip_href.to_string() } else { format!("{}/$value", zip_href.trim_end_matches('/')) }
    } else {
        zip_href.to_string()
    };

    if let Some(entry) = cache_get_url(&index, zip_href) {
        if entry.local_path.exists() {
            info!("Cache hit (by URL): using materialized ZIP {:?}", entry.local_path);
            return Ok(entry.local_path.to_string_lossy().to_string());
        }
    }
    if let Some(entry) = cache_get_url(&index, &download_url) {
        if entry.local_path.exists() {
            info!("Cache hit (by normalized URL): using materialized ZIP {:?}", entry.local_path);
            return Ok(entry.local_path.to_string_lossy().to_string());
        }
    }
    if let Some(uuid) = cdse_uuid.as_ref() {
        if let Some(entry) = cache_get_alias(&index, "cdse", uuid) {
            if entry.local_path.exists() {
                info!("Cache hit (by alias cdse:{}): using materialized ZIP {:?}", uuid, entry.local_path);
                return Ok(entry.local_path.to_string_lossy().to_string());
            }
        }
    }

    let client = reqwest::blocking::Client::builder()
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(http_timeout_s))
        .build()
        .map_err(|e| AppError::RemoteDirListing { url: zip_href.to_string(), hint: format!("http client build error: {}", e) })?;

    let mut cdse = if is_cdse { Some(CdseProvider::new(http_timeout_s)) } else { None };
    let mut asf = if is_asf { Some(AsfProvider::new(http_timeout_s)) } else { None };
    if let Some(provider) = cdse.as_mut() { let _ = provider.prepare(); }
    if let Some(provider) = asf.as_mut() { let _ = provider.prepare(); }

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

    if is_asf {
        if let Some(p) = asf.as_ref() {
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

    if canonical_name.is_none() {
        if let Some(uuid) = cdse_uuid.as_ref() {
            canonical_name = Some(format!("{}.zip", uuid));
        } else {
            let tail_opt = if zip_href.contains('/') { Some(zip_href.rsplit('/').next().unwrap_or("remote.SAFE.zip")) } else { None };
            let fname = match tail_opt { Some(t) if !t.is_empty() => t.to_string(), _ => "remote.SAFE.zip".to_string() };
            canonical_name = Some(fname);
        }
    }
    if canonical_name.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        canonical_name = Some("remote.SAFE.zip".to_string());
    }
    let local_path = cache_dir.join(canonical_name.as_ref().unwrap());

    if Path::new(zip_href).exists() {
        if !local_path.exists() {
            std::fs::copy(zip_href, &local_path)?;
        }
        let entry = CacheEntry { local_path: local_path.clone(), etag: None, content_length: None };
        cache_put_url(&mut index, zip_href, entry.clone());
        cache_put_url(&mut index, &download_url, entry.clone());
        if let Some(uuid) = cdse_uuid.as_ref() { cache_put_alias(&mut index, "cdse", uuid, entry.clone()); }
        save_index(cache_dir, &index);
        return Ok(local_path.to_string_lossy().to_string());
    }

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

    let partial = local_path.with_extension("partial");

    if local_path.exists() {
        if let Ok(md) = std::fs::metadata(&local_path) {
            if let Some(cl) = content_len_opt {
                if md.len() == cl {
                    info!("Cache hit: using materialized ZIP {:?} ({} bytes)", local_path, md.len());
                    return Ok(local_path.to_string_lossy().to_string());
                }
            } else {
                info!("Cache hit: using materialized ZIP {:?}", local_path);
                return Ok(local_path.to_string_lossy().to_string());
            }
        }
    }

    if !is_asf {
        let _f = std::fs::File::create(&partial)
            .map_err(|e| AppError::RemoteDirListing { url: zip_href.to_string(), hint: format!("open partial error: {}", e) })?;
    }
    info!("Downloading to cache: {} -> {:?}", zip_href, partial);

    let mut attempts = 0u8;
    let max_retries = http_retries.max(1);
    loop {
        attempts += 1;
        if is_asf {
            let resume_from = std::fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
            let pb = ProgressBar::new_spinner();
            info!("ASF curl download: resume at {} of {:?}", resume_from, content_len_opt);

            let partial_path = partial.clone();
            let pb_clone = pb.clone();
            let monitor_handle = std::thread::spawn(move || {
                let mut last_size = std::fs::metadata(&partial_path).map(|m| m.len()).unwrap_or(resume_from);
                while !pb_clone.is_finished() {
                    if let Ok(metadata) = std::fs::metadata(&partial_path) {
                        let current_size = metadata.len();
                        if current_size != last_size {
                            pb_clone.set_position(current_size);
                            last_size = current_size;
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            });
            if let Some(total_bytes) = content_len_opt {
                pb.set_style(
                    ProgressStyle::with_template(
                        "{spinner:.green} [{elapsed_precise}] {bytes}/{total_bytes} ({percent}%) downloaded ({eta})"
                    )
                    .unwrap()
                );
                pb.set_length(total_bytes);
                pb.set_position(resume_from);
            } else {
                pb.set_style(
                    ProgressStyle::with_template(
                        "{spinner:.green} [{elapsed_precise}] {bytes} downloaded"
                    )
                    .unwrap()
                );
            }

            let mut cmd = Command::new("curl");
            cmd.arg("-L")
                .arg("--fail")
                .arg("-sS")
                .arg("--connect-timeout").arg(format!("{}", http_timeout_s))
                .arg("--netrc-file").arg(asf.as_ref().unwrap().netrc_path.as_os_str())
                .arg("-c").arg(asf.as_ref().unwrap().cookie_jar.as_os_str())
                .arg("-b").arg(asf.as_ref().unwrap().cookie_file.as_os_str())
                .arg("-C").arg("-")
                .arg("--retry").arg(format!("{}", http_retries.max(1)))
                .arg("--retry-all-errors")
                .arg("--retry-delay").arg(format!("{}", (http_backoff_ms / 1000).max(1)))
                .arg("-o").arg(&partial)
                .arg(&download_url);
            let status = cmd.status();

            if let Ok(metadata) = std::fs::metadata(&partial) { pb.set_position(metadata.len()); }
            pb.finish();
            let _ = monitor_handle.join();

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
            let pb = ProgressBar::new_spinner();

            let mut req = client.get(&download_url)
                .header(reqwest::header::ACCEPT_ENCODING, "identity");
            if let Some(provider) = cdse.as_ref() {
                if let Ok(hmap) = provider.headers_for(&download_url) {
                    for (k, v) in hmap { req = req.header(k, v); }
                }
            }
            let resp = req.send();
            if let Some(total_bytes) = content_len_opt {
                pb.set_style(
                    ProgressStyle::with_template(
                        "{spinner:.green} [{elapsed_precise}] {bytes}/{total_bytes} ({percent}%) downloaded ({eta})"
                    )
                    .unwrap()
                );
                pb.set_length(total_bytes);
            } else {
                pb.set_style(
                    ProgressStyle::with_template(
                        "{spinner:.green} [{elapsed_precise}] {bytes} downloaded"
                    )
                    .unwrap()
                );
            }
            match resp {
                Ok(r) if r.status().is_success() => {
                    let mut file = std::fs::File::create(&partial)
                        .map_err(|e| AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("open partial error: {}", e) })?;
                    let body = r
                        .error_for_status()
                        .map_err(|e| AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("http status error: {}", e) })?;

                    let mut reader = pb.wrap_read(body);
                    let copied = std::io::copy(&mut reader, &mut file)
                        .map_err(|e| AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("write error: {}", e) })?;
                    let _ = file.sync_all();

                    pb.finish();

                    if let Some(total) = content_len_opt {
                        let have = std::fs::metadata(&partial).ok().map(|m| m.len()).unwrap_or(0);
                        if have != total {
                            if attempts < max_retries { std::thread::sleep(std::time::Duration::from_millis(http_backoff_ms * attempts as u64)); continue; }
                            return Err(AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("size mismatch: have {}, want {} (copied {})", have, total, copied) }.into());
                        }
                    }
                    std::fs::rename(&partial, &local_path)
                        .map_err(|e| AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("rename error: {}", e) })?;
                    let final_len = std::fs::metadata(&local_path).ok().map(|m| m.len()).unwrap_or(0);
                    info!("Cached ZIP ready: {:?} ({} bytes)", local_path, final_len);
                    let entry = CacheEntry { local_path: local_path.clone(), etag: None, content_length: content_len_opt };
                    cache_put_url(&mut index, zip_href, entry.clone());
                    cache_put_url(&mut index, &download_url, entry.clone());
                    if let Some(uuid) = cdse_uuid.as_ref() { cache_put_alias(&mut index, "cdse", uuid, entry.clone()); }
                    save_index(cache_dir, &index);
                    return Ok(local_path.to_string_lossy().to_string());
                }
                Ok(r) if r.status().as_u16() >= 500 || r.status().as_u16() == 429 => {
                    pb.finish();
                    if attempts < max_retries { std::thread::sleep(std::time::Duration::from_millis(http_backoff_ms * attempts as u64)); continue; }
                    return Err(AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("http error {} after retries", r.status()) }.into());
                }
                Ok(r) => {
                    pb.finish();
                    return Err(AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("http error {}", r.status()) }.into());
                }
                Err(e) => {
                    pb.finish();
                    if attempts < max_retries { std::thread::sleep(std::time::Duration::from_millis(http_backoff_ms * attempts as u64)); continue; }
                    return Err(AppError::RemoteDirListing { url: download_url.to_string(), hint: format!("network error: {}", e) }.into());
                }
            }
        }
    }
}


