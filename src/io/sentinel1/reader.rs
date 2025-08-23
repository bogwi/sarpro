use std::path::{Path, PathBuf};
use ndarray::Array2;
use tracing::{info, warn};
use chrono;

use crate::io::gdal::to_gdal_path;
use gdal::raster::ResampleAlg;

use super::errors::SafeError;
use super::types::{ProductType, SafeMetadata, TargetCrsArg};
use super::data_loader::load_polarization_data_with_options;
use super::crs_utils::resolve_auto_target_crs_from_dataset_path;

// Include the implementation methods from reader_methods.rs
// Note: reader_methods.rs contains the implementation methods using `impl super::SafeReader`

/// Reader for Sentinel-1 SAFE archives
pub struct SafeReader {
    pub base_path: PathBuf,
    pub metadata: SafeMetadata,
    pub product_type: ProductType,
    pub vv_data: Option<Array2<f32>>,
    pub vh_data: Option<Array2<f32>>,
    pub hh_data: Option<Array2<f32>>,
    pub hv_data: Option<Array2<f32>>,
}

impl SafeReader {
    /// Open explicit measurement URLs/paths (VV/VH/HH/HV) without a SAFE folder.
    /// Minimal metadata is constructed from datasets; product_type is set to GRD.
    pub fn open_from_measurements(
        vv: Option<&str>,
        vh: Option<&str>,
        hh: Option<&str>,
        hv: Option<&str>,
        target_crs: Option<TargetCrsArg>,
        resample_alg: Option<ResampleAlg>,
        target_size: Option<usize>,
    ) -> Result<Self, SafeError> {
        let mut meta = SafeMetadata {
            instrument: String::new(),
            platform: String::new(),
            acquisition_start: String::new(),
            acquisition_stop: String::new(),
            orbit_number: 0,
            polarizations: Vec::new(),
            lines: 0,
            samples: 0,
            product_type: "GRD".to_string(),
            range_sampling_rate: None,
            radar_frequency: None,
            prf: None,
            tx_pulse_length: None,
            tx_pulse_ramp_rate: None,
            velocity: None,
            slant_range_near: None,
            geotransform: None,
            projection: None,
            crs: None,
            pixel_spacing_range: None,
            pixel_spacing_azimuth: None,
            instrument_mode: None,
            pass_direction: None,
            data_take_id: None,
            product_id: None,
            processing_level: None,
            multilook_factor: None,
            calibration_type: None,
            noise_estimate: None,
            processing_center: None,
            software_version: None,
            pixel_data_type: None,
            bits_per_sample: None,
            sample_format: None,
            incidence_angle: None,
            look_angle: None,
            doppler_centroid: None,
            radiometric_calibration: None,
            geometric_calibration: None,
            conversion_tool: "SARPRO".to_string(),
            conversion_version: env!("CARGO_PKG_VERSION").to_string(),
            conversion_timestamp: chrono::Utc::now().to_rfc3339(),
        };

        // Resolve effective target CRS; support AUTO by probing the first provided dataset
        let effective_target_crs = match target_crs {
            Some(TargetCrsArg::Custom(s)) => Some(s),
            Some(TargetCrsArg::None) => None,
            Some(TargetCrsArg::Auto) => {
                // Pick the first provided measurement URL/path as candidate
                let candidate = vv.or(vh).or(hh).or(hv);
                candidate
                    .and_then(|href| resolve_auto_target_crs_from_dataset_path(Path::new(href)))
            }
            None => None,
        };

        let mut reader_vv: Option<Array2<f32>> = None;
        let mut reader_vh: Option<Array2<f32>> = None;
        let mut reader_hh: Option<Array2<f32>> = None;
        let mut reader_hv: Option<Array2<f32>> = None;

        // Helper to load and also update base georef/dims if empty
        let mut load_and_update = |
            href: &str,
            pol_label: &str,
        | -> Result<Array2<f32>, SafeError> {
            let p = Path::new(href);
            let arr = load_polarization_data_with_options(
                p,
                &mut meta,
                effective_target_crs.as_deref(),
                resample_alg,
                target_size,
            )?;
            if meta.lines == 0 || meta.samples == 0 {
                meta.lines = arr.nrows();
                meta.samples = arr.ncols();
            }
            meta.polarizations.push(pol_label.to_uppercase());
            Ok(arr)
        };

        if let Some(href) = vv { reader_vv = Some(load_and_update(href, "VV")?); }
        if let Some(href) = vh { reader_vh = Some(load_and_update(href, "VH")?); }
        if let Some(href) = hh { reader_hh = Some(load_and_update(href, "HH")?); }
        if let Some(href) = hv { reader_hv = Some(load_and_update(href, "HV")?); }

        if reader_vv.is_none() && reader_vh.is_none() && reader_hh.is_none() && reader_hv.is_none() {
            return Err(SafeError::MissingField("At least one measurement URL must be provided"));
        }

        Ok(SafeReader {
            base_path: PathBuf::from(vv.or(vh).or(hh).or(hv).unwrap_or("remote")),
            metadata: meta,
            product_type: ProductType::GRD,
            vv_data: reader_vv,
            vh_data: reader_vh,
            hh_data: reader_hh,
            hv_data: reader_hv,
        })
    }

    /// Open a remote SAFE packaged as a ZIP (URL or local .zip) using VSIZIP/VSICURL.
    pub fn open_from_safe_zip_url(
        zip_href: &str,
        polarization: crate::types::Polarization,
        target_crs: Option<TargetCrsArg>,
        resample_alg: Option<ResampleAlg>,
        target_size: Option<usize>,
    ) -> Result<Self, SafeError> {
        // Build VSI base for the ZIP
        let base_vsi = to_gdal_path(Path::new(zip_href)).into_owned();
        // Discover top-level SAFE folder inside the ZIP root
        let root_entries = gdal::vsi::read_dir(&base_vsi, false)
            .map_err(|e| SafeError::Parse(format!("VSIReadDir error: {}", e)))?;
        let mut safe_root_rel: Option<String> = None;
        for rel in root_entries.iter() {
            let name = rel.to_string_lossy();
            let name_str = name.as_ref();
            if name_str.ends_with(".SAFE") || name_str.ends_with(".SAFE/") {
                // Normalize to no trailing slash when joining
                let trimmed = name_str.trim_end_matches('/');
                safe_root_rel = Some(trimmed.to_string());
                break;
            }
        }
        let safe_root_rel = safe_root_rel.ok_or_else(|| SafeError::Parse("No .SAFE root found inside ZIP".to_string()))?;
        let safe_root = format!("{}/{}", base_vsi, safe_root_rel);
        let measurement_dir = format!("{}/measurement", safe_root);

        // List measurement directory via VSI (non-recursive)
        let entries = gdal::vsi::read_dir(&measurement_dir, false)
            .map_err(|e| SafeError::Parse(format!("VSIReadDir error: {}", e)))?;

        let mut vv_path: Option<PathBuf> = None;
        let mut vh_path: Option<PathBuf> = None;
        let mut hh_path: Option<PathBuf> = None;
        let mut hv_path: Option<PathBuf> = None;

        for rel in entries {
            let name_lc = rel.to_string_lossy().to_lowercase();
            if !(name_lc.ends_with(".tif") || name_lc.ends_with(".tiff")) {
                continue;
            }
            let full = PathBuf::from(format!("{}/{}", measurement_dir, rel.to_string_lossy()));
            if name_lc.contains("vv") {
                vv_path = Some(full.clone());
                info!("Found VV file: {:?}", full);
            } else if name_lc.contains("vh") {
                vh_path = Some(full.clone());
                info!("Found VH file: {:?}", full);
            } else if name_lc.contains("hh") {
                hh_path = Some(full.clone());
                info!("Found HH file: {:?}", full);
            } else if name_lc.contains("hv") {
                hv_path = Some(full.clone());
                info!("Found HV file: {:?}", full);
            }
        }

        // Minimal metadata
        let mut metadata = SafeMetadata {
            instrument: String::new(),
            platform: String::new(),
            acquisition_start: String::new(),
            acquisition_stop: String::new(),
            orbit_number: 0,
            polarizations: Vec::new(),
            lines: 0,
            samples: 0,
            product_type: "GRD".to_string(),
            range_sampling_rate: None,
            radar_frequency: None,
            prf: None,
            tx_pulse_length: None,
            tx_pulse_ramp_rate: None,
            velocity: None,
            slant_range_near: None,
            geotransform: None,
            projection: None,
            crs: None,
            pixel_spacing_range: None,
            pixel_spacing_azimuth: None,
            instrument_mode: None,
            pass_direction: None,
            data_take_id: None,
            product_id: None,
            processing_level: None,
            multilook_factor: None,
            calibration_type: None,
            noise_estimate: None,
            processing_center: None,
            software_version: None,
            pixel_data_type: None,
            bits_per_sample: None,
            sample_format: None,
            incidence_angle: None,
            look_angle: None,
            doppler_centroid: None,
            radiometric_calibration: None,
            geometric_calibration: None,
            conversion_tool: "SARPRO".to_string(),
            conversion_version: env!("CARGO_PKG_VERSION").to_string(),
            conversion_timestamp: chrono::Utc::now().to_rfc3339(),
        };

        // Resolve effective target CRS once
        let effective_target_crs: Option<String> = match target_crs {
            Some(TargetCrsArg::Custom(s)) => Some(s),
            Some(TargetCrsArg::None) => None,
            Some(TargetCrsArg::Auto) => {
                // Build a candidate measurement path inside the ZIP if present
                // Prefer VV/VH, else HH/HV, else first TIFF found
                let mut candidate: Option<PathBuf> = None;
                let meas_entries = gdal::vsi::read_dir(&measurement_dir, false)
                    .map_err(|e| SafeError::Parse(format!("VSIReadDir error: {}", e)))?;
                for rel in meas_entries.iter() {
                    let name = rel.to_string_lossy().to_lowercase();
                    if name.ends_with(".tif") || name.ends_with(".tiff") {
                        if candidate.is_none() {
                            candidate = Some(PathBuf::from(format!("{}/{}", measurement_dir, rel.to_string_lossy())));
                        }
                        if name.contains("vv") || name.contains("vh") || name.contains("hh") || name.contains("hv") {
                            candidate = Some(PathBuf::from(format!("{}/{}", measurement_dir, rel.to_string_lossy())));
                            if name.contains("vv") || name.contains("vh") { break; }
                        }
                    }
                }
                candidate
                    .as_ref()
                    .and_then(|p| super::crs_utils::resolve_auto_target_crs_from_dataset_path(p))
            }
            None => None,
        };

        let mut vv_data = None;
        let mut vh_data = None;
        let mut hh_data = None;
        let mut hv_data = None;

        let load_if = |
            opt_path: &Option<PathBuf>,
            label: &str,
            meta: &mut SafeMetadata,
        | -> Result<Option<Array2<f32>>, SafeError> {
            if let Some(p) = opt_path {
                let arr = super::data_loader::load_polarization_data_with_options(
                    p.as_path(),
                    meta,
                    effective_target_crs.as_deref(),
                    resample_alg,
                    target_size,
                )?;
                if meta.lines == 0 || meta.samples == 0 {
                    meta.lines = arr.nrows();
                    meta.samples = arr.ncols();
                }
                meta.polarizations.push(label.to_string());
                Ok(Some(arr))
            } else {
                Ok(None)
            }
        };

        match polarization {
            crate::types::Polarization::Vv => {
                vv_data = load_if(&vv_path, "VV", &mut metadata)?;
                if vv_data.is_none() { return Err(SafeError::MissingField("VV measurement file")); }
            }
            crate::types::Polarization::Vh => {
                vh_data = load_if(&vh_path, "VH", &mut metadata)?;
                if vh_data.is_none() { return Err(SafeError::MissingField("VH measurement file")); }
            }
            crate::types::Polarization::Hh => {
                hh_data = load_if(&hh_path, "HH", &mut metadata)?;
                if hh_data.is_none() { return Err(SafeError::MissingField("HH measurement file")); }
            }
            crate::types::Polarization::Hv => {
                hv_data = load_if(&hv_path, "HV", &mut metadata)?;
                if hv_data.is_none() { return Err(SafeError::MissingField("HV measurement file")); }
            }
            crate::types::Polarization::Multiband => {
                // Prefer VV/VH if available, else HH/HV
                if vv_path.is_some() && vh_path.is_some() {
                    vv_data = load_if(&vv_path, "VV", &mut metadata)?;
                    vh_data = load_if(&vh_path, "VH", &mut metadata)?;
                } else if hh_path.is_some() && hv_path.is_some() {
                    hh_data = load_if(&hh_path, "HH", &mut metadata)?;
                    hv_data = load_if(&hv_path, "HV", &mut metadata)?;
                }
                if (vv_data.is_none() || vh_data.is_none()) && (hh_data.is_none() || hv_data.is_none()) {
                    return Err(SafeError::MissingField("VV/VH or HH/HV measurement pair"));
                }
            }
            crate::types::Polarization::OP(_op) => {
                // Operations require a pair; prefer VV/VH else HH/HV
                if vv_path.is_some() && vh_path.is_some() {
                    vv_data = load_if(&vv_path, "VV", &mut metadata)?;
                    vh_data = load_if(&vh_path, "VH", &mut metadata)?;
                } else if hh_path.is_some() && hv_path.is_some() {
                    hh_data = load_if(&hh_path, "HH", &mut metadata)?;
                    hv_data = load_if(&hv_path, "HV", &mut metadata)?;
                }
                if (vv_data.is_none() || vh_data.is_none()) && (hh_data.is_none() || hv_data.is_none()) {
                    return Err(SafeError::MissingField("VV/VH or HH/HV measurement pair"));
                }
            }
        }

        Ok(SafeReader {
            base_path: PathBuf::from(base_vsi),
            metadata,
            product_type: ProductType::GRD,
            vv_data,
            vh_data,
            hh_data,
            hv_data,
        })
    }

    /// Open and parse a SAFE directory with polarization awareness
    pub fn open<P: AsRef<Path>>(
        safe_dir: P,
        polarization: Option<&str>,
    ) -> Result<Self, SafeError> {
        Self::open_with_options(safe_dir, polarization, None, None, None)
    }

    /// Open and parse a SAFE directory with optional reprojection to target CRS
    pub fn open_with_options<P: AsRef<Path>>(
        safe_dir: P,
        polarization: Option<&str>,
        target_crs: Option<TargetCrsArg>,
        resample_alg: Option<ResampleAlg>,
        target_size: Option<usize>,
    ) -> Result<Self, SafeError> {
        use super::discovery::identify_polarization_files;
        use super::data_loader::load_polarization_data_with_options;
        use super::manifest_parser::parse_comprehensive_metadata;
        use super::crs_utils::resolve_auto_target_crs;

        let base = safe_dir.as_ref().to_path_buf();
        let annotation = base.join("annotation");
        let measurement = base.join("measurement");
        if !annotation.is_dir() {
            return Err(SafeError::MissingField("annotation directory"));
        }
        if !measurement.is_dir() {
            return Err(SafeError::MissingField("measurement directory"));
        }

        // Parse comprehensive metadata from manifest.safe and annotation files
        let mut metadata = parse_comprehensive_metadata(&base)?;

        // Detect product type from metadata
        info!("Detecting product type from metadata");
        let product_type = match metadata.product_type.to_uppercase().as_str() {
            "GRD" => ProductType::GRD,
            unsupported => {
                warn!("Unsupported product type: {}", unsupported);
                return Err(SafeError::UnsupportedProduct(unsupported.to_string()));
            }
        };

        // Identify polarization files based on metadata and requested polarization
        info!("Identifying polarization files");
        let (vv_path, vh_path, hh_path, hv_path) =
            identify_polarization_files(&measurement, &metadata.polarizations)?;

        // Resolve effective target CRS exactly once per product
        let effective_target_crs: Option<String> = match target_crs {
            Some(TargetCrsArg::Custom(s)) => Some(s),
            Some(TargetCrsArg::None) => None,
            Some(TargetCrsArg::Auto) => resolve_auto_target_crs(&base),
            None => None,
        };

        // Load data based on requested polarization
        let mut vv_data = None;
        let mut vh_data = None;
        let mut hh_data = None;
        let mut hv_data = None;

        match polarization {
            Some("vv") | None => {
                metadata.polarizations = vec!["VV".to_string()];
                // Load VV data (default)
                if let Some(path) = vv_path {
                    info!("Loading VV polarization data");
                    vv_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    return Err(SafeError::MissingField("VV measurement file"));
                }
            }
            Some("vh") => {
                metadata.polarizations = vec!["VH".to_string()];
                // Load VH data
                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    return Err(SafeError::MissingField("VH measurement file"));
                }
            }
            Some("hh") => {
                metadata.polarizations = vec!["HH".to_string()];
                // Load HH data
                if let Some(path) = hh_path {
                    info!("Loading HH polarization data");
                    hh_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    return Err(SafeError::MissingField("HH measurement file"));
                }
            }
            Some("hv") => {
                metadata.polarizations = vec!["HV".to_string()];
                // Load HV data
                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    return Err(SafeError::MissingField("HV measurement file"));
                }
            }
            Some("multiband") => {
                // Load both VV and VH data
                if let Some(path) = vv_path {
                    info!("Loading VV polarization data");
                    vv_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    return Err(SafeError::MissingField("VV measurement file"));
                }

                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    return Err(SafeError::MissingField("VH measurement file"));
                }
            }
            Some("vv_vh_pair") => {
                metadata.polarizations = vec!["VV".to_string(), "VH".to_string()];
                // Load both VV and VH data for operations
                if let Some(path) = vv_path {
                    info!("Loading VV polarization data");
                    vv_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    return Err(SafeError::MissingField("VV measurement file"));
                }

                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    return Err(SafeError::MissingField("VH measurement file"));
                }
            }
            Some("hh_hv_pair") => {
                metadata.polarizations = vec!["HH".to_string(), "HV".to_string()];
                // Load both HH and HV data for operations
                if let Some(path) = hh_path {
                    info!("Loading HH polarization data");
                    hh_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    return Err(SafeError::MissingField("HH measurement file"));
                }

                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    return Err(SafeError::MissingField("HV measurement file"));
                }
            }
            Some("all_pairs") => {
                metadata.polarizations = vec![
                    "VV".to_string(),
                    "VH".to_string(),
                    "HH".to_string(),
                    "HV".to_string(),
                ];
                // Load all available polarization data for operations
                if let Some(path) = vv_path {
                    info!("Loading VV polarization data");
                    vv_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                }
                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                }
                if let Some(path) = hh_path {
                    info!("Loading HH polarization data");
                    hh_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                }
                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                }
            }
            Some(unsupported) => {
                return Err(SafeError::Parse(format!(
                    "Unsupported polarization: {}",
                    unsupported
                )));
            }
        }

        Ok(SafeReader {
            base_path: base,
            metadata,
            product_type,
            vv_data,
            vh_data,
            hh_data,
            hv_data,
        })
    }

    /// Open and parse a SAFE directory with warnings instead of errors for unsupported products
    /// This is useful for batch processing where you want to continue processing other files
    pub fn open_with_warnings<P: AsRef<Path>>(
        safe_dir: P,
        polarization: Option<&str>,
    ) -> Result<Option<Self>, SafeError> {
        Self::open_with_warnings_with_options(safe_dir, polarization, None, None, None)
    }

    /// Open and parse a SAFE directory with warnings and optional reprojection and target-size downsampling
    pub fn open_with_warnings_with_options<P: AsRef<Path>>(
        safe_dir: P,
        polarization: Option<&str>,
        target_crs: Option<TargetCrsArg>,
        resample_alg: Option<ResampleAlg>,
        target_size: Option<usize>,
    ) -> Result<Option<Self>, SafeError> {
        use super::discovery::identify_polarization_files;
        use super::data_loader::load_polarization_data_with_options;
        use super::manifest_parser::parse_comprehensive_metadata;
        use super::crs_utils::resolve_auto_target_crs;
        use tracing::warn;

        let base = safe_dir.as_ref().to_path_buf();
        let annotation = base.join("annotation");
        let measurement = base.join("measurement");
        if !annotation.is_dir() {
            return Err(SafeError::MissingField("annotation directory"));
        }
        if !measurement.is_dir() {
            return Err(SafeError::MissingField("measurement directory"));
        }

        let mut metadata = parse_comprehensive_metadata(&base)?;

        let product_type = match metadata.product_type.to_uppercase().as_str() {
            "GRD" => ProductType::GRD,
            unsupported => {
                warn!(
                    "Skipping unsupported product type: {} (file: {:?})",
                    unsupported, base
                );
                return Ok(None); // Return None instead of error for unsupported products
            }
        };

        let (vv_path, vh_path, hh_path, hv_path) =
            identify_polarization_files(&measurement, &metadata.polarizations)?;

        // Resolve effective target CRS exactly once per product
        let effective_target_crs: Option<String> = match target_crs {
            Some(TargetCrsArg::Custom(s)) => Some(s),
            Some(TargetCrsArg::None) => None,
            Some(TargetCrsArg::Auto) => resolve_auto_target_crs(&base),
            None => None,
        };

        let mut vv_data = None;
        let mut vh_data = None;
        let mut hh_data = None;
        let mut hv_data = None;

        match polarization {
            Some("vv") | None => {
                metadata.polarizations = vec!["VV".to_string()];
                if let Some(path) = vv_path {
                    info!("Loading VV polarization data");
                    vv_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    warn!("VV measurement file not found, skipping product");
                    return Ok(None);
                }
            }
            Some("vh") => {
                metadata.polarizations = vec!["VH".to_string()];
                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    warn!("VH measurement file not found, skipping product");
                    return Ok(None);
                }
            }
            Some("hh") => {
                metadata.polarizations = vec!["HH".to_string()];
                if let Some(path) = hh_path {
                    info!("Loading HH polarization data");
                    hh_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    warn!("HH measurement file not found, skipping product");
                    return Ok(None);
                }
            }
            Some("hv") => {
                metadata.polarizations = vec!["HV".to_string()];
                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    warn!("HV measurement file not found, skipping product");
                    return Ok(None);
                }
            }
            Some("multiband") | Some("vv_vh_pair") | Some("all_pairs") => {
                if let Some(path) = vv_path {
                    info!("Loading VV polarization data");
                    vv_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                }
                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                }
                if let Some(path) = hh_path {
                    info!("Loading HH polarization data");
                    hh_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                }
                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                }
                if vv_data.is_none() && vh_data.is_none() && hh_data.is_none() && hv_data.is_none()
                {
                    warn!("No polarization files found, skipping product");
                    return Ok(None);
                }
            }
            Some("hh_hv_pair") => {
                if let Some(path) = hh_path {
                    info!("Loading HH polarization data");
                    hh_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    warn!("HH measurement file not found, skipping product");
                    return Ok(None);
                }
                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        effective_target_crs.as_deref(),
                        resample_alg,
                        target_size,
                    )?);
                } else {
                    warn!("HV measurement file not found, skipping product");
                    return Ok(None);
                }
            }
            Some(unsupported) => {
                warn!(
                    "Unsupported polarization: {}, skipping product",
                    unsupported
                );
                return Ok(None);
            }
        }

        Ok(Some(SafeReader {
            base_path: base,
            metadata,
            product_type,
            vv_data,
            vh_data,
            hh_data,
            hv_data,
        }))
    }
}