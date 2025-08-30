use tracing::{info, warn};

use sarpro::io::sentinel1::TargetCrsArg;
use sarpro::io::SafeReader;

use crate::cli::args::CliArgs;
use crate::cli::errors::AppError;

use super::common::{map_resample_alg, map_target_crs, parse_target_size, map_bit_depth, list_remote_safe_dirs};
use super::save_dispatch::resolve_polarization_and_save_image;

pub fn process_safe_dir_url(args: &CliArgs) -> Result<(), Box<dyn std::error::Error>> {
    let dir_url = args.input_dir.as_ref().unwrap();
    let dir_url_str = dir_url.to_string_lossy();

    if !args.batch {
        warn!("--input-dir <URL> implies batch processing; add --batch to process all .SAFE folders");
    }
    let output_dir = args.output_dir.clone().ok_or(AppError::MissingArgument { arg: "--output-dir".to_string() })?;
    std::fs::create_dir_all(&output_dir)?;

    let safes = list_remote_safe_dirs(dir_url_str.as_ref())?;

    if safes.is_empty() {
        warn!("No .SAFE directories found under {}", dir_url_str);
        return Ok(());
    }

    info!("Found {} SAFE directories to process", safes.len());

    let resample_alg = map_resample_alg(args);
    let target_arg: Option<TargetCrsArg> = map_target_crs(args);
    let target_size = parse_target_size(args)?;

    let mut processed = 0usize;
    let mut errors = 0usize;

    for safe_name in safes.into_iter() {
        if safe_name == "." || safe_name == ".." { continue; }
        let safe_url = format!("{}/{}", dir_url_str.trim_end_matches('/'), safe_name);
        let out_name = format!("{}.{}", safe_name, match args.format { sarpro::types::OutputFormat::TIFF => "tiff", sarpro::types::OutputFormat::JPEG => "jpg" });
        let out_path = output_dir.join(out_name);

        info!("Processing remote SAFE directory: {} -> {:?}", safe_url, out_path);
        let reader_res = SafeReader::open_from_safe_dir_url(
            &safe_url,
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
            Err(e) => { errors += 1; warn!("Error opening {}: {}", safe_url, e); }
        }
    }

    info!("Remote SAFE dir batch complete. processed={}, errors={}", processed, errors);
    Ok(())
}


