use tracing::{info, warn};

use sarpro::io::sentinel1::TargetCrsArg;
use sarpro::io::SafeReader;

use crate::cli::args::CliArgs;
use crate::cli::errors::AppError;

use super::common::{map_resample_alg, map_target_crs, parse_target_size, map_bit_depth, list_remote_zip_entries};
use super::save_dispatch::resolve_polarization_and_save_image;
use super::materialize::materialize_zip;

pub fn process_safe_zip_dir_url(args: &CliArgs) -> Result<(), Box<dyn std::error::Error>> {
    let dir_url = args.safe_zip_dir_url.as_ref().unwrap();

    if !args.batch {
        warn!("--safe-zip-dir-url implies batch processing; add --batch to process all ZIPs");
    }
    let output_dir = args.output_dir.clone().ok_or(AppError::MissingArgument { arg: "--output-dir".to_string() })?;
    std::fs::create_dir_all(&output_dir)?;

    let zips = list_remote_zip_entries(dir_url)?;

    if zips.is_empty() {
        warn!("No .SAFE.zip files found under {}", dir_url);
        return Ok(());
    }

    info!("Found {} SAFE ZIPs to process", zips.len());

    let resample_alg = map_resample_alg(args);
    let target_arg: Option<TargetCrsArg> = map_target_crs(args);
    let target_size = parse_target_size(args)?;

    let mut processed = 0usize;
    let mut errors = 0usize;

    for zip_name in zips.into_iter() {
        if zip_name == "." || zip_name == ".." { continue; }
        let zip_url = format!("{}/{}", dir_url.trim_end_matches('/'), zip_name);
        let href = materialize_zip(
            &zip_url,
            &args.remote_cache_dir,
            args.http_timeout,
            args.http_retries as u8,
            args.http_backoff_ms,
        )?;
        let name_lc = zip_name.to_lowercase();
        let out_stem: String = if name_lc.ends_with(".safe.zip") {
            zip_name[..zip_name.len() - ".SAFE.zip".len()].to_string()
        } else if name_lc.ends_with(".zip") {
            zip_name[..zip_name.len() - ".zip".len()].to_string()
        } else {
            zip_name.clone()
        };
        let out_name = format!("{}.{}", out_stem, match args.format { sarpro::types::OutputFormat::TIFF => "tiff", sarpro::types::OutputFormat::JPEG => "jpg" });
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
                let bit_depth_enum = map_bit_depth(args);
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


