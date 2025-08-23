use std::fs;
use tracing::{info, warn};
use sarpro::types::OutputFormat;
use sarpro::core::params::ProcessingOptions;

use crate::cli::args::CliArgs;
use crate::cli::errors::AppError;

use super::single_file::process_single_file;

pub fn process_batch(args: &CliArgs, proc_opts: &ProcessingOptions) -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = args.input_dir.clone().ok_or(AppError::MissingArgument {
        arg: "--input-dir".to_string(),
    })?;
    let output_dir = args.output_dir.clone().ok_or(AppError::MissingArgument {
        arg: "--output-dir".to_string(),
    })?;

    fs::create_dir_all(&output_dir)?;

    info!("Starting batch processing from directory: {:?}", input_dir);
    info!("Output directory: {:?}", output_dir);

    let mut processed = 0;
    let mut skipped = 0;
    let mut errors = 0;

    for entry in fs::read_dir(&input_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let safe_name = path.file_name().unwrap().to_string_lossy();
            let output_name = format!(
                "{}.{}",
                safe_name,
                match args.format {
                    OutputFormat::TIFF => "tiff",
                    OutputFormat::JPEG => "jpg",
                }
            );
            let output_path = output_dir.join(&output_name);

            info!("Processing: {:?} -> {:?}", path, output_path);

            match process_single_file(
                &path,
                &output_path,
                args.format,
                args.bit_depth,
                args.input_format,
                args.polarization,
                args.autoscale,
                &args.size,
                true,
                args.pad,
                args.target_crs.as_deref(),
                args.resample_alg.as_deref(),
                args.synrgb_mode,
                proc_opts,
            ) {
                Ok(()) => {
                    info!("Successfully processed: {:?}\n", path);
                    processed += 1;
                }
                Err(e) => {
                    warn!("Error processing {:?}: {}", path, e);
                    errors += 1;
                }
            }
        } else {
            info!("Skipping non-directory: {:?}", path);
            skipped += 1;
        }
    }

    info!("Batch processing complete!");
    info!("Processed: {}", processed);
    info!("Skipped: {}", skipped);
    info!("Errors: {}", errors);

    Ok(())
}
