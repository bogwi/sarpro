use tracing::info;

use sarpro::io::sentinel1::TargetCrsArg;
use sarpro::io::SafeReader;

use crate::cli::args::CliArgs;
use crate::cli::errors::AppError;

use super::common::{map_resample_alg, map_target_crs, parse_target_size, map_bit_depth};
use super::save_dispatch::resolve_polarization_and_save_image;

pub fn process_measurement_urls(args: &CliArgs) -> Result<(), Box<dyn std::error::Error>> {
    let output = args.output.clone().ok_or(AppError::MissingArgument { arg: "--output".to_string() })?;
    let resample_alg = map_resample_alg(args);
    let target_arg: Option<TargetCrsArg> = map_target_crs(args);
    let target_size = parse_target_size(args)?;

    let reader = SafeReader::open_from_measurements(
        args.vv_url.as_deref(),
        args.vh_url.as_deref(),
        args.hh_url.as_deref(),
        args.hv_url.as_deref(),
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
    info!("Successfully processed remote measurements -> {:?}", output);
    Ok(())
}


