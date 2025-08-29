pub mod single_file;
pub mod batch_processing;
pub mod remote_processing;
pub mod argument_mapping;
pub mod input_validation;

use tracing::info;

use crate::cli::args::CliArgs;
use crate::cli::errors::AppError;

use self::single_file::process_single_file;
use self::batch_processing::process_batch;
use self::remote_processing::{process_safe_zip_url, process_safe_zip_dir_url, process_measurement_urls};
use self::argument_mapping::build_processing_options_from_args;
use self::input_validation::validate_input_modes;

pub fn run(args: CliArgs) -> Result<(), Box<dyn std::error::Error>> {
    if args.log {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    }

    let batch_mode = args.batch || args.input_dir.is_some();

    // Build ProcessingOptions aggregate (threaded but not yet acted upon by legacy path)
    let proc_opts = build_processing_options_from_args(&args);

    // Validate input modes
    validate_input_modes(&args)?;

    // Reserve STAC Item for Step 4 — currently not implemented
    if let Some(item) = args.stac_item.as_ref() {
        return Err(AppError::MissingArgument { arg: format!("--stac-item not implemented yet: {}", item) }.into());
    }

    // Handle new remote ZIP/explicit measurement modes
    if let Some(_) = args.safe_zip_url.as_ref() {
        return process_safe_zip_url(&args);
    }

    // Handle remote directory of SAFE ZIPs for batch processing via VSICURL directory listing
    if let Some(_) = args.safe_zip_dir_url.as_ref() {
        return process_safe_zip_dir_url(&args);
    }

    // If --input-dir is an HTTP(S) URL, treat it as a remote directory of unpacked .SAFE folders
    if let Some(input_dir) = args.input_dir.as_ref() {
        if let Some(s) = input_dir.to_str() {
            let s_trim = s.trim();
            if s_trim.starts_with("http://") || s_trim.starts_with("https://") || s_trim.starts_with("/vsicurl/") {
                return remote_processing::process_safe_dir_url(&args);
            }
        }
    }

    if args.vv_url.is_some() || args.vh_url.is_some() || args.hh_url.is_some() || args.hv_url.is_some() {
        return process_measurement_urls(&args);
    }

    if batch_mode {
        return process_batch(&args, &proc_opts);
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
