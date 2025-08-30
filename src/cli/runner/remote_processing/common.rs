use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use gdal::raster::ResampleAlg;

use sarpro::io::gdal::to_gdal_path;
use sarpro::io::sentinel1::TargetCrsArg;
use sarpro::types::{BitDepth, OutputFormat};
use sarpro::BitDepthArg;

use crate::cli::args::CliArgs;
use crate::cli::errors::AppError;

pub fn map_resample_alg(args: &CliArgs) -> Option<ResampleAlg> {
    match args.resample_alg.as_deref() {
        Some("nearest") => Some(ResampleAlg::NearestNeighbour),
        Some("bilinear") => Some(ResampleAlg::Bilinear),
        Some("cubic") => Some(ResampleAlg::Cubic),
        Some("lanczos") => Some(ResampleAlg::Lanczos),
        _ => None,
    }
}

pub fn map_target_crs(args: &CliArgs) -> Option<TargetCrsArg> {
    match args.target_crs.as_deref() {
        Some(t) if t.eq_ignore_ascii_case("none") => Some(TargetCrsArg::None),
        Some(t) if t.eq_ignore_ascii_case("auto") => Some(TargetCrsArg::Auto),
        Some(t) => Some(TargetCrsArg::Custom(t.to_string())),
        None => None,
    }
}

pub fn parse_target_size(args: &CliArgs) -> Result<Option<usize>, Box<dyn std::error::Error>> {
    if args.size == "original" {
        Ok(None)
    } else {
        Ok(Some(args.size.parse::<usize>().map_err(|_| AppError::InvalidSize { size: args.size.clone() })?))
    }
}

pub fn map_bit_depth(args: &CliArgs) -> BitDepth {
    match args.bit_depth { BitDepthArg::U8 => BitDepth::U8, BitDepthArg::U16 => BitDepth::U16 }
}

pub fn derive_output_path_for_zip(href: &str, output_arg: &Path, format: OutputFormat) -> PathBuf {
    if output_arg.is_dir() {
        let fname = std::path::Path::new(href)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("remote.SAFE.zip");
        let fname_lc = fname.to_ascii_lowercase();
        let stem: String = if fname_lc.ends_with(".safe.zip") {
            fname[..fname.len() - ".SAFE.zip".len()].to_string()
        } else if fname_lc.ends_with(".zip") {
            fname[..fname.len() - ".zip".len()].to_string()
        } else if fname_lc.ends_with(".safe") {
            fname[..fname.len() - ".SAFE".len()].to_string()
        } else {
            fname.to_string()
        };
        let ext = match format { OutputFormat::TIFF => "tiff", OutputFormat::JPEG => "jpg" };
        let out_name = format!("{}.{}", stem, ext);
        let out_path = output_arg.join(out_name);
        if let Some(parent) = out_path.parent() { let _ = fs::create_dir_all(parent); }
        out_path
    } else {
        if let Some(parent) = std::path::Path::new(output_arg).parent() { let _ = fs::create_dir_all(parent); }
        output_arg.to_path_buf()
    }
}

pub fn list_remote_zip_entries(dir_url: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let vsi_base = to_gdal_path(std::path::Path::new(dir_url)).into_owned();
    let vsi_with_slash = format!("{}/", vsi_base.trim_end_matches('/'));
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
        let url_with_slash = format!("{}/", dir_url.trim_end_matches('/'));
        let out = Command::new("curl")
            .arg("-L")
            .arg("-s")
            .arg(&url_with_slash)
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let body = String::from_utf8_lossy(&o.stdout);
                let mut names: Vec<String> = Vec::new();
                let mut start = 0usize;
                while let Some(h) = body[start..].find("href=\"") {
                    let i = start + h + 6;
                    if let Some(end) = body[i..].find('"') {
                        let candidate = &body[i..i + end];
                        let cand_lc = candidate.to_lowercase();
                        if cand_lc.ends_with(".safe.zip") || cand_lc.ends_with(".zip") {
                            if !candidate.contains("://") && !candidate.starts_with('/') {
                                names.push(candidate.to_string());
                            } else if let Some(pos) = candidate.rsplit('/').next() {
                                let pos_lc = pos.to_lowercase();
                                if pos_lc.ends_with(".safe.zip") || pos_lc.ends_with(".zip") {
                                    names.push(pos.to_string());
                                }
                            }
                        }
                        start = i + end + 1;
                    } else {
                        break;
                    }
                }
                for line in body.lines() {
                    let t = line.trim();
                    let t_lc = t.to_lowercase();
                    if t_lc.ends_with(".safe.zip") || t_lc.ends_with(".zip") {
                        let fname = t.split_whitespace().last().unwrap_or(t);
                        names.push(fname.to_string());
                    }
                }
                if names.is_empty() {
                    return Err(AppError::RemoteDirListing {
                        url: dir_url.to_string(),
                        hint: "No .zip entries found. Ensure server lists files or use --safe-zip-url.".to_string(),
                    }.into());
                }
                names.into_iter().map(PathBuf::from).collect()
            }
            _ => {
                return Err(AppError::RemoteDirListing {
                    url: dir_url.to_string(),
                    hint: "Failed to fetch listing. Ensure the URL is reachable or use --safe-zip-url.".to_string(),
                }.into());
            }
        }
    };

    let mut zips: Vec<String> = Vec::new();
    for name_os in entries.iter() {
        let name = name_os.to_string_lossy().to_string();
        let name_lc = name.to_lowercase();
        if name_lc.ends_with(".safe.zip") || name_lc.ends_with(".zip") {
            zips.push(name);
        }
    }
    Ok(zips)
}

pub fn list_remote_safe_dirs(dir_url: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let dir_url_str = dir_url;
    let vsi_base = to_gdal_path(std::path::Path::new(dir_url_str)).into_owned();
    let vsi_with_slash = format!("{}/", vsi_base.trim_end_matches('/'));
    let opt_form = format!(
        "/vsicurl?use_head=no&list_dir=yes&url={}",
        dir_url_str
    );
    let opt_form_slash = format!(
        "/vsicurl?use_head=no&list_dir=yes&url={}/",
        dir_url_str.trim_end_matches('/')
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
        let url_with_slash = format!("{}/", dir_url_str.trim_end_matches('/'));
        let out = Command::new("curl")
            .arg("-L")
            .arg("-s")
            .arg(&url_with_slash)
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let body = String::from_utf8_lossy(&o.stdout);
                let mut names: Vec<String> = Vec::new();
                let mut start = 0usize;
                while let Some(h) = body[start..].find("href=\"") {
                    let i = start + h + 6;
                    if let Some(end) = body[i..].find('"') {
                        let candidate = &body[i..i + end];
                        let cand_lc = candidate.to_lowercase();
                        if cand_lc.ends_with(".safe/") || cand_lc.ends_with(".safe") {
                            if !candidate.contains("://") && !candidate.starts_with('/') {
                                names.push(candidate.trim_end_matches('/').to_string());
                            } else if let Some(pos) = candidate.rsplit('/').next() {
                                let pos_lc = pos.to_lowercase();
                                if pos_lc.ends_with(".safe") || pos_lc.ends_with(".safe/") {
                                    names.push(pos.trim_end_matches('/').to_string());
                                }
                            }
                        }
                        start = i + end + 1;
                    } else {
                        break;
                    }
                }
                for line in body.lines() {
                    let t = line.trim();
                    let t_lc = t.to_lowercase();
                    if t_lc.ends_with(".safe") || t_lc.ends_with(".safe/") {
                        let fname = t.split_whitespace().last().unwrap_or(t);
                        names.push(fname.trim_end_matches('/').to_string());
                    }
                }
                if names.is_empty() {
                    return Err(AppError::RemoteDirListing {
                        url: dir_url_str.to_string(),
                        hint: "No .SAFE directories found. Ensure server lists directories or pass a direct --input URL to a .SAFE.".to_string(),
                    }.into());
                }
                names.into_iter().map(PathBuf::from).collect()
            }
            _ => {
                return Err(AppError::RemoteDirListing {
                    url: dir_url_str.to_string(),
                    hint: "Failed to fetch listing. Ensure the URL is reachable or use a local --input-dir.".to_string(),
                }.into());
            }
        }
    };

    let mut safes: Vec<String> = Vec::new();
    for name_os in entries.iter() {
        let mut name = name_os.to_string_lossy().to_string();
        if name.ends_with('/') { name.pop(); }
        let name_lc = name.to_lowercase();
        if name_lc.ends_with(".safe") { safes.push(name); }
    }
    Ok(safes)
}


