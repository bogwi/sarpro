use tracing::info;

use sarpro::io::sentinel1::TargetCrsArg;
use sarpro::io::SafeReader;
use sarpro::io::credentials::CredentialProvider;

use crate::cli::args::CliArgs;
use crate::cli::errors::AppError;

use super::common::{map_resample_alg, map_target_crs, parse_target_size, map_bit_depth, derive_output_path_for_zip};
use super::save_dispatch::resolve_polarization_and_save_image;
use super::materialize::materialize_zip;

pub fn process_safe_zip_url(args: &CliArgs) -> Result<(), Box<dyn std::error::Error>> {
    let zip_url = args.safe_zip_url.as_ref().unwrap();
    let output_arg = args.output.clone().ok_or(AppError::MissingArgument { arg: "--output".to_string() })?;

    let resample_alg = map_resample_alg(args);
    let target_arg: Option<TargetCrsArg> = map_target_crs(args);
    let target_size = parse_target_size(args)?;

    let href = materialize_zip(
        zip_url,
        &args.remote_cache_dir,
        args.http_timeout,
        args.http_retries as u8,
        args.http_backoff_ms,
    )?;

    let output = derive_output_path_for_zip(&href, &output_arg, args.format);

    if args.remote_cache_dir.is_none() && zip_url.contains("asf.alaska.edu") {
        let mut asf = sarpro::io::credentials::AsfProvider::new(args.http_timeout);
        let _ = asf.prepare();
    }

    let reader = SafeReader::open_from_safe_zip_url(
        &href,
        args.polarization,
        target_arg,
        resample_alg,
        target_size,
    )?;

    let bit_depth_enum = map_bit_depth(args);
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


