use std::fs;
use std::path::Path;

use super::errors::SafeError;

/// Identify VV and VH polarization files in the measurement directory
pub fn identify_polarization_files(
    measurement_path: &Path,
    available_polarizations: &[String],
) -> Result<
    (
        Option<std::path::PathBuf>,
        Option<std::path::PathBuf>,
        Option<std::path::PathBuf>,
        Option<std::path::PathBuf>,
    ),
    SafeError,
> {
    let mut vv_path = None;
    let mut vh_path = None;
    let mut hh_path = None;
    let mut hv_path = None;

    // First, try to find files based on polarization in filename
    for entry in fs::read_dir(measurement_path)? {
        let path = entry?.path();
        if let Some(name) = path.file_name() {
            let name_str = name.to_string_lossy().to_lowercase();
            if name_str.ends_with(".tiff") || name_str.ends_with(".tif") {
                // Skip any previously warped intermediates to avoid double-processing
                if name_str.contains("_warped.tif") || name_str.contains("_warped.tiff") {
                    continue;
                }
                if name_str.contains("vv") {
                    vv_path = Some(path.clone());
                    tracing::info!("Found VV file: {:?}", path);
                } else if name_str.contains("vh") {
                    vh_path = Some(path.clone());
                    tracing::info!("Found VH file: {:?}", path);
                } else if name_str.contains("hh") {
                    hh_path = Some(path.clone());
                    tracing::info!("Found HH file: {:?}", path);
                } else if name_str.contains("hv") {
                    hv_path = Some(path.clone());
                    tracing::info!("Found HV file: {:?}", path);
                }
            }
        }
    }

    // If we didn't find polarization-specific files, try to infer from available polarizations
    if vv_path.is_none() && vh_path.is_none() && hh_path.is_none() && hv_path.is_none() {
        tracing::info!(
            "No polarization-specific files found, checking available polarizations: {:?}",
            available_polarizations
        );

        // Look for any TIFF file and assume it's the available polarization
        for entry in fs::read_dir(measurement_path)? {
            let path = entry?.path();
            if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                if ext == "tiff" || ext == "tif" {
                    // Check if this polarization is available in metadata
                    for pol in available_polarizations {
                        if pol.to_lowercase() == "vv" {
                            vv_path = Some(path.clone());
                            tracing::info!("Found VV file (inferred): {:?}", path);
                            break;
                        } else if pol.to_lowercase() == "vh" {
                            vh_path = Some(path.clone());
                            tracing::info!("Found VH file (inferred): {:?}", path);
                            break;
                        } else if pol.to_lowercase() == "hh" {
                            hh_path = Some(path.clone());
                            tracing::info!("Found HH file (inferred): {:?}", path);
                            break;
                        }
                    }
                    // If we found a file, break (assume single polarization product)
                    if vv_path.is_some() || vh_path.is_some() || hh_path.is_some() {
                        break;
                    }
                }
            }
        }
    }

    Ok((vv_path, vh_path, hh_path, hv_path))
}
