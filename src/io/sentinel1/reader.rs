use std::path::{Path, PathBuf};
use ndarray::Array2;
use tracing::{info, warn};
use chrono;

use crate::io::gdal::to_gdal_path;
use crate::Polarization;
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
    /// Helper to load polarization data and update metadata dimensions/labels
    fn load_and_note(
        path: &std::path::Path,
        label: &str,
        meta: &mut SafeMetadata,
        target_crs: Option<&str>,
        resample_alg: Option<gdal::raster::ResampleAlg>,
        target_size: Option<usize>,
    ) -> Result<ndarray::Array2<f32>, SafeError> {
        let arr = load_polarization_data_with_options(
            path, meta, target_crs, resample_alg, target_size
        )?;
        if meta.lines == 0 || meta.samples == 0 {
            meta.lines = arr.nrows();
            meta.samples = arr.ncols();
        }
        meta.polarizations.push(label.to_string());
        Ok(arr)
    }

    /// Internal helper for unified SAFE directory opening with different error policies
    fn open_dir_internal(
        base: &std::path::Path,
        polarization: Polarization,
        target_crs: Option<TargetCrsArg>,
        resample_alg: Option<ResampleAlg>,
        target_size: Option<usize>,
        warn_mode: bool,
    ) -> Result<Option<SafeReader>, SafeError> {
        use super::discovery::identify_polarization_files;

        use super::manifest_parser::parse_comprehensive_metadata;
        use super::crs_utils::resolve_auto_target_crs;

        let annotation = base.join("annotation");
        let measurement = base.join("measurement");
        if !annotation.is_dir() {
            return if warn_mode {
                Ok(None)
            } else {
                Err(SafeError::MissingField("annotation directory"))
            };
        }
        if !measurement.is_dir() {
            return if warn_mode {
                Ok(None)
            } else {
                Err(SafeError::MissingField("measurement directory"))
            };
        }

        // Parse comprehensive metadata from manifest.safe and annotation files
        let mut metadata = parse_comprehensive_metadata(&base)?;

        // Detect product type from metadata
        info!("Detecting product type from metadata");
        let product_type = match metadata.product_type.to_uppercase().as_str() {
            "GRD" => ProductType::GRD,
            unsupported => {
                if warn_mode {
                    warn!("Skipping unsupported product type: {} (file: {:?})", unsupported, base);
                    return Ok(None);
                } else {
                    return Err(SafeError::UnsupportedProduct(unsupported.to_string()));
                }
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

        let missing_field = |field: &str| -> Result<Option<SafeReader>, SafeError> {
            if warn_mode {
                warn!("{} measurement file not found, skipping product", field);
                Ok(None)
            } else {
                Err(SafeError::MissingField("measurement file"))
            }
        };

        match polarization {
            Polarization::Vv => {
                metadata.polarizations.clear();
                if let Some(path) = vv_path {
                    info!("Loading VV polarization data");
                    vv_data = Some(Self::load_and_note(
                        &path, "VV", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                } else {
                    return missing_field("VV");
                }
            }
            Polarization::Vh => {
                metadata.polarizations.clear();
                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(Self::load_and_note(
                        &path, "VH", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                } else {
                    return missing_field("VH");
                }
            }
            Polarization::Hh => {
                metadata.polarizations.clear();
                if let Some(path) = hh_path {
                    info!("Loading HH polarization data");
                    hh_data = Some(Self::load_and_note(
                        &path, "HH", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                } else {
                    return missing_field("HH");
                }
            }
            Polarization::Hv => {
                metadata.polarizations.clear();
                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(Self::load_and_note(
                        &path, "HV", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                } else {
                    return missing_field("HV");
                }
            }
            Polarization::Multiband => {
                metadata.polarizations.clear();
                if let Some(path) = vv_path {
                    info!("Loading VV polarization data");
                    vv_data = Some(Self::load_and_note(
                        &path, "VV", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                }
                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(Self::load_and_note(
                        &path, "VH", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                }
                if let Some(path) = hh_path {
                    info!("Loading HH polarization data");
                    hh_data = Some(Self::load_and_note(
                        &path, "HH", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                }
                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(Self::load_and_note(
                        &path, "HV", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                }
                if vv_data.is_none() && vh_data.is_none() && hh_data.is_none() && hv_data.is_none() {
                    if warn_mode {
                        warn!("No polarization files found, skipping product");
                        return Ok(None);
                    } else {
                        return Err(SafeError::MissingField("No measurement files found"));
                    }
                }
            }
            Polarization::OP(_) => {
                metadata.polarizations.clear();
                if let Some(path) = vv_path {
                    info!("Loading VV polarization data");
                    vv_data = Some(Self::load_and_note(
                        &path, "VV", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                }
                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(Self::load_and_note(
                        &path, "VH", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                }
                if let Some(path) = hh_path {
                    info!("Loading HH polarization data");
                    hh_data = Some(Self::load_and_note(
                        &path, "HH", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                }
                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(Self::load_and_note(
                        &path, "HV", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                }
            }
        }

        Ok(Some(SafeReader {
            base_path: base.to_path_buf(),
            metadata,
            product_type,
            vv_data,
            vh_data,
            hh_data,
            hv_data,
        }))
    }

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
            product_type: "GRD".to_string(),
            conversion_tool: "SARPRO".to_string(),
            conversion_version: env!("CARGO_PKG_VERSION").to_string(),
            conversion_timestamp: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
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

        if let Some(href) = vv {
            reader_vv = Some(Self::load_and_note(
                Path::new(href), "VV", &mut meta,
                effective_target_crs.as_deref(), resample_alg, target_size
            )?);
        }
        if let Some(href) = vh {
            reader_vh = Some(Self::load_and_note(
                Path::new(href), "VH", &mut meta,
                effective_target_crs.as_deref(), resample_alg, target_size
            )?);
        }
        if let Some(href) = hh {
            reader_hh = Some(Self::load_and_note(
                Path::new(href), "HH", &mut meta,
                effective_target_crs.as_deref(), resample_alg, target_size
            )?);
        }
        if let Some(href) = hv {
            reader_hv = Some(Self::load_and_note(
                Path::new(href), "HV", &mut meta,
                effective_target_crs.as_deref(), resample_alg, target_size
            )?);
        }

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
        polarization: Polarization,
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
        let mut auto_crs_candidate: Option<PathBuf> = None;

        for rel in entries {
            let name_lc = rel.to_string_lossy().to_lowercase();
            if !(name_lc.ends_with(".tif") || name_lc.ends_with(".tiff")) {
                continue;
            }
            let full = PathBuf::from(format!("{}/{}", measurement_dir, rel.to_string_lossy()));

            // Set AUTO CRS candidate from first TIFF found (will be overridden by vv/vh if present)
            if auto_crs_candidate.is_none() {
                auto_crs_candidate = Some(full.clone());
            }

            if name_lc.contains("vv") {
                vv_path = Some(full.clone());
                auto_crs_candidate = Some(full.clone()); // Prefer VV for AUTO CRS
                info!("Found VV file: {:?}", full);
            } else if name_lc.contains("vh") {
                vh_path = Some(full.clone());
                if auto_crs_candidate.as_ref().map_or(true, |p| !p.to_string_lossy().to_lowercase().contains("vv")) {
                    auto_crs_candidate = Some(full.clone()); // Prefer VH over non-VV/non-VH
                }
                info!("Found VH file: {:?}", full);
            } else if name_lc.contains("hh") {
                hh_path = Some(full.clone());
                if auto_crs_candidate.as_ref().map_or(true, |p| {
                    let p_lc = p.to_string_lossy().to_lowercase();
                    !p_lc.contains("vv") && !p_lc.contains("vh")
                }) {
                    auto_crs_candidate = Some(full.clone()); // Prefer HH over non-polarimetric
                }
                info!("Found HH file: {:?}", full);
            } else if name_lc.contains("hv") {
                hv_path = Some(full.clone());
                if auto_crs_candidate.as_ref().map_or(true, |p| {
                    let p_lc = p.to_string_lossy().to_lowercase();
                    !p_lc.contains("vv") && !p_lc.contains("vh")
                }) {
                    auto_crs_candidate = Some(full.clone()); // Prefer HV over non-polarimetric
                }
                info!("Found HV file: {:?}", full);
            }
        }

        // Minimal metadata
        let mut metadata = SafeMetadata {
            product_type: "GRD".to_string(),
            conversion_tool: "SARPRO".to_string(),
            conversion_version: env!("CARGO_PKG_VERSION").to_string(),
            conversion_timestamp: chrono::Utc::now().to_rfc3339(),
            ..Default::default()
        };

        // Resolve effective target CRS once (using candidate from single-pass directory scan)
        let effective_target_crs: Option<String> = match target_crs {
            Some(TargetCrsArg::Custom(s)) => Some(s),
            Some(TargetCrsArg::None) => None,
            Some(TargetCrsArg::Auto) => {
                auto_crs_candidate
                    .as_ref()
                    .and_then(|p| super::crs_utils::resolve_auto_target_crs_from_dataset_path(p))
            }
            None => None,
        };

        let mut vv_data = None;
        let mut vh_data = None;
        let mut hh_data = None;
        let mut hv_data = None;

        match polarization {
            Polarization::Vv => {
                if let Some(p) = &vv_path {
                    vv_data = Some(Self::load_and_note(
                        p.as_path(), "VV", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                }
                if vv_data.is_none() { return Err(SafeError::MissingField("VV measurement file")); }
            }
            Polarization::Vh => {
                if let Some(p) = &vh_path {
                    vh_data = Some(Self::load_and_note(
                        p.as_path(), "VH", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                }
                if vh_data.is_none() { return Err(SafeError::MissingField("VH measurement file")); }
            }
            Polarization::Hh => {
                if let Some(p) = &hh_path {
                    hh_data = Some(Self::load_and_note(
                        p.as_path(), "HH", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                }
                if hh_data.is_none() { return Err(SafeError::MissingField("HH measurement file")); }
            }
            Polarization::Hv => {
                if let Some(p) = &hv_path {
                    hv_data = Some(Self::load_and_note(
                        p.as_path(), "HV", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                }
                if hv_data.is_none() { return Err(SafeError::MissingField("HV measurement file")); }
            }
            Polarization::Multiband => {
                // Prefer VV/VH if available, else HH/HV
                if vv_path.is_some() && vh_path.is_some() {
                    vv_data = Some(Self::load_and_note(
                        vv_path.as_ref().unwrap(), "VV", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                    vh_data = Some(Self::load_and_note(
                        vh_path.as_ref().unwrap(), "VH", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                } else if hh_path.is_some() && hv_path.is_some() {
                    hh_data = Some(Self::load_and_note(
                        hh_path.as_ref().unwrap(), "HH", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                    hv_data = Some(Self::load_and_note(
                        hv_path.as_ref().unwrap(), "HV", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                }
                if (vv_data.is_none() || vh_data.is_none()) && (hh_data.is_none() || hv_data.is_none()) {
                    return Err(SafeError::MissingField("VV/VH or HH/HV measurement pair"));
                }
            }
            Polarization::OP(_op) => {
                // Operations require a pair; prefer VV/VH else HH/HV
                if vv_path.is_some() && vh_path.is_some() {
                    vv_data = Some(Self::load_and_note(
                        vv_path.as_ref().unwrap(), "VV", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                    vh_data = Some(Self::load_and_note(
                        vh_path.as_ref().unwrap(), "VH", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                } else if hh_path.is_some() && hv_path.is_some() {
                    hh_data = Some(Self::load_and_note(
                        hh_path.as_ref().unwrap(), "HH", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
                    hv_data = Some(Self::load_and_note(
                        hv_path.as_ref().unwrap(), "HV", &mut metadata,
                        effective_target_crs.as_deref(), resample_alg, target_size
                    )?);
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
        polarization: Polarization,
    ) -> Result<Self, SafeError> {
        Self::open_with_options(safe_dir, polarization, None, None, None)
    }

    /// Open and parse a SAFE directory with optional reprojection to target CRS
    pub fn open_with_options<P: AsRef<Path>>(
        safe_dir: P,
        polarization: Polarization,
        target_crs: Option<TargetCrsArg>,
        resample_alg: Option<ResampleAlg>,
        target_size: Option<usize>,
    ) -> Result<Self, SafeError> {
        let base = safe_dir.as_ref();
        match Self::open_dir_internal(base, polarization, target_crs, resample_alg, target_size, false)? {
            Some(reader) => Ok(reader),
            None => Err(SafeError::MissingField("Reader creation failed")), // This shouldn't happen in strict mode
        }
    }

    /// Open and parse a SAFE directory with warnings instead of errors for unsupported products
    /// This is useful for batch processing where you want to continue processing other files
    pub fn open_with_warnings<P: AsRef<Path>>(
        safe_dir: P,
        polarization: Polarization,
    ) -> Result<Option<Self>, SafeError> {
        Self::open_with_warnings_with_options(safe_dir, polarization, None, None, None)
    }

    /// Open and parse a SAFE directory with warnings and optional reprojection and target-size downsampling
    pub fn open_with_warnings_with_options<P: AsRef<Path>>(
        safe_dir: P,
        polarization: Polarization,
        target_crs: Option<TargetCrsArg>,
        resample_alg: Option<ResampleAlg>,
        target_size: Option<usize>,
    ) -> Result<Option<Self>, SafeError> {
        let base = safe_dir.as_ref();
        Self::open_dir_internal(base, polarization, target_crs, resample_alg, target_size, true)
    }
}