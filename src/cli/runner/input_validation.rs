use crate::cli::args::CliArgs;
use crate::cli::errors::AppError;

pub fn validate_input_modes(args: &CliArgs) -> Result<(), AppError> {
    // Enforce mutually exclusive input modes when new flags are used
    let mut selected_modes = 0;
    if args.input.is_some() || args.input_dir.is_some() { selected_modes += 1; }
    if args.stac_item.is_some() { selected_modes += 1; }
    // (no deprecated flags)
    if args.safe_zip_url.is_some() { selected_modes += 1; }
    if args.vv_url.is_some() || args.vh_url.is_some() || args.hh_url.is_some() || args.hv_url.is_some() { selected_modes += 1; }
    if args.input_list.is_some() { selected_modes += 1; }
    if selected_modes > 1 {
        return Err(AppError::MissingArgument { arg: "Select exactly one input mode (legacy --input/--input-dir OR one of: --stac-item | --safe-zip-url | --*-url | --input-list)".to_string() });
    }
    Ok(())
}
