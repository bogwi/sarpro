//! Process all SAFE subdirectories from input_dir into output_dir

use std::path::Path;
use crate::core::params::ProcessingParams;
use crate::error::{Error, Result};
use crate::io::sentinel1::{SafeReader, TargetCrsArg};
use super::batch_report::BatchReport;
use super::iterate_safe_products::iterate_safe_products;
use super::process_safe_to_path::process_safe_to_path;

/// Process all SAFE subdirectories from `input_dir` into `output_dir` using `params`.
/// If `continue_on_error` is true, errors are logged in the report and processing continues; otherwise, the first error is returned.
pub fn process_directory_to_path(
    input_dir: &Path,
    output_dir: &Path,
    params: &ProcessingParams,
    continue_on_error: bool,
) -> Result<BatchReport> {
    std::fs::create_dir_all(output_dir).map_err(Error::from)?;

    let mut report = BatchReport::default();

    let mut iter = iterate_safe_products(input_dir)?;
    while let Some(path) = iter.next() {
        // Early viability check to allow skipping unsupported product types
        // Map target CRS and resample algorithm similarly to single-file path
        let target_arg: Option<TargetCrsArg> = match params.target_crs.as_deref() {
            Some(t) if t.eq_ignore_ascii_case("none") => Some(TargetCrsArg::None),
            Some(t) if t.eq_ignore_ascii_case("auto") => Some(TargetCrsArg::Auto),
            Some(t) => Some(TargetCrsArg::Custom(t.to_string())),
            None => None,
        };
        let resample_alg = match params.resample_alg.as_deref() {
            Some("nearest") => Some(gdal::raster::ResampleAlg::NearestNeighbour),
            Some("bilinear") => Some(gdal::raster::ResampleAlg::Bilinear),
            Some("cubic") => Some(gdal::raster::ResampleAlg::Cubic),
            Some("lanczos") | None => Some(gdal::raster::ResampleAlg::Lanczos),
            Some(_) => Some(gdal::raster::ResampleAlg::Lanczos),
        };

        match SafeReader::open_with_warnings_with_options(
            &path,
            params.polarization,
            target_arg,
            resample_alg,
            params.size,
        )? {
            Some(_) => {
                // Determine output file name
                let safe_name = path.file_name().unwrap().to_string_lossy();
                let ext = match params.format {
                    crate::types::OutputFormat::TIFF => "tiff",
                    crate::types::OutputFormat::JPEG => "jpg",
                };
                let output_path = output_dir.join(format!("{}.{}", safe_name, ext));

                match process_safe_to_path(&path, &output_path, params) {
                    Ok(()) => report.processed += 1,
                    Err(e) => {
                        report.errors += 1;
                        if !continue_on_error {
                            return Err(e);
                        }
                    }
                }
            }
            None => {
                report.skipped += 1;
                continue;
            }
        }
    }

    Ok(report)
}
