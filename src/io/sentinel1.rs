use chrono;
use ndarray::Array2;
use num_complex::Complex;
use quick_xml::Reader;
use quick_xml::events::Event;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::info;
use tracing::warn;

use crate::io::gdal::GdalSarReader;
use gdal::Dataset;
use gdal::raster::Buffer;
use gdal::raster::ResampleAlg;
use std::process::Command;

/// Errors encountered when reading SAFE archives
#[derive(Debug, Error)]
pub enum SafeError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("XML parse error: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("TIFF error: {0}")]
    Tiff(#[from] tiff::TiffError),
    #[error("Missing field `{0}` in SAFE metadata")]
    MissingField(&'static str),
    #[error("Unsupported SAFE product type: {0}")]
    UnsupportedProduct(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Provided SLC measurement file is not a TIFF file: {0}")]
    NotTiff(String),
}

/// Sentinel-1 product types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductType {
    GRD,
}

/// Metadata extracted from SAFE
#[derive(Debug, Clone)]
pub struct SafeMetadata {
    // Basic product information
    pub instrument: String,
    pub platform: String,
    pub acquisition_start: String,
    pub acquisition_stop: String,
    pub orbit_number: u64,
    pub polarizations: Vec<String>,
    pub lines: usize,
    pub samples: usize,
    pub product_type: String,

    // SAR parameters
    pub range_sampling_rate: Option<f64>,
    pub radar_frequency: Option<f64>,
    pub prf: Option<f64>,
    pub tx_pulse_length: Option<f64>,
    pub tx_pulse_ramp_rate: Option<f64>,
    pub velocity: Option<f64>,
    pub slant_range_near: Option<f64>,

    // Georeferencing information
    pub geotransform: Option<[f64; 6]>,
    pub projection: Option<String>,
    pub crs: Option<String>,
    pub pixel_spacing_range: Option<f64>,
    pub pixel_spacing_azimuth: Option<f64>,

    // Acquisition details
    pub instrument_mode: Option<String>,
    pub pass_direction: Option<String>,
    pub data_take_id: Option<String>,
    pub product_id: Option<String>,

    // Processing parameters
    pub processing_level: Option<String>,
    pub multilook_factor: Option<u32>,
    pub calibration_type: Option<String>,
    pub noise_estimate: Option<f64>,
    pub processing_center: Option<String>,
    pub software_version: Option<String>,

    // Image characteristics
    pub pixel_data_type: Option<String>,
    pub bits_per_sample: Option<u32>,
    pub sample_format: Option<String>,

    // Additional SAR-specific metadata
    pub incidence_angle: Option<f64>,
    pub look_angle: Option<f64>,
    pub doppler_centroid: Option<f64>,
    pub radiometric_calibration: Option<String>,
    pub geometric_calibration: Option<String>,

    // Conversion provenance
    pub conversion_tool: String,
    pub conversion_version: String,
    pub conversion_timestamp: String,
}

/// Reader for Sentinel-1 SAFE archives
pub struct SafeReader {
    pub base_path: PathBuf,
    pub metadata: SafeMetadata,
    pub product_type: ProductType,
    pub vv_data: Option<Array2<Complex<f64>>>,
    pub vh_data: Option<Array2<Complex<f64>>>,
    pub hh_data: Option<Array2<Complex<f64>>>,
    pub hv_data: Option<Array2<Complex<f64>>>,
}

impl SafeReader {
    /// Open and parse a SAFE directory with polarization awareness
    pub fn open<P: AsRef<Path>>(
        safe_dir: P,
        polarization: Option<&str>,
    ) -> Result<Self, SafeError> {
        Self::open_with_options(safe_dir, polarization, None, None)
    }

    /// Open and parse a SAFE directory with optional reprojection to target CRS
    pub fn open_with_options<P: AsRef<Path>>(
        safe_dir: P,
        polarization: Option<&str>,
        target_crs: Option<&str>,
        resample_alg: Option<ResampleAlg>,
    ) -> Result<Self, SafeError> {
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
        let mut metadata = Self::parse_comprehensive_metadata(&base)?;

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
            Self::identify_polarization_files(&measurement, &metadata.polarizations)?;

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
                    vv_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
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
                    vh_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
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
                    hh_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
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
                    hv_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
                    )?);
                } else {
                    return Err(SafeError::MissingField("HV measurement file"));
                }
            }
            Some("multiband") => {
                // Load both VV and VH data
                if let Some(path) = vv_path {
                    info!("Loading VV polarization data");
                    vv_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
                    )?);
                } else {
                    return Err(SafeError::MissingField("VV measurement file"));
                }

                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
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
                    vv_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
                    )?);
                } else {
                    return Err(SafeError::MissingField("VV measurement file"));
                }

                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
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
                    hh_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
                    )?);
                } else {
                    return Err(SafeError::MissingField("HH measurement file"));
                }

                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
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
                    vv_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
                    )?);
                }
                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
                    )?);
                }
                if let Some(path) = hh_path {
                    info!("Loading HH polarization data");
                    hh_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
                    )?);
                }
                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(Self::load_polarization_data_with_options(
                        &path,
                        &mut metadata,
                        target_crs,
                        resample_alg,
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
        let mut metadata = Self::parse_comprehensive_metadata(&base)?;

        // Detect product type from metadata
        info!("Detecting product type from metadata");
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

        // Identify polarization files based on metadata and requested polarization
        info!("Identifying polarization files");
        let (vv_path, vh_path, hh_path, hv_path) =
            Self::identify_polarization_files(&measurement, &metadata.polarizations)?;

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
                    vv_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
                } else {
                    warn!("VV measurement file not found, skipping product");
                    return Ok(None);
                }
            }
            Some("vh") => {
                metadata.polarizations = vec!["VH".to_string()];
                // Load VH data
                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
                } else {
                    warn!("VH measurement file not found, skipping product");
                    return Ok(None);
                }
            }
            Some("hh") => {
                metadata.polarizations = vec!["HH".to_string()];
                // Load HH data
                if let Some(path) = hh_path {
                    info!("Loading HH polarization data");
                    hh_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
                } else {
                    warn!("HH measurement file not found, skipping product");
                    return Ok(None);
                }
            }
            Some("hv") => {
                metadata.polarizations = vec!["HV".to_string()];
                // Load HV data
                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
                } else {
                    warn!("HV measurement file not found, skipping product");
                    return Ok(None);
                }
            }
            Some("multiband") => {
                // Load both VV and VH data
                if let Some(path) = vv_path {
                    info!("Loading VV polarization data");
                    vv_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
                } else {
                    warn!("VV measurement file not found, skipping product");
                    return Ok(None);
                }

                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
                } else {
                    warn!("VH measurement file not found, skipping product");
                    return Ok(None);
                }
            }
            Some("vv_vh_pair") => {
                metadata.polarizations = vec!["VV".to_string(), "VH".to_string()];
                // Load both VV and VH data for operations
                if let Some(path) = vv_path {
                    info!("Loading VV polarization data");
                    vv_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
                } else {
                    warn!("VV measurement file not found, skipping product");
                    return Ok(None);
                }

                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
                } else {
                    warn!("VH measurement file not found, skipping product");
                    return Ok(None);
                }
            }
            Some("hh_hv_pair") => {
                metadata.polarizations = vec!["HH".to_string(), "HV".to_string()];
                // Load both HH and HV data for operations
                if let Some(path) = hh_path {
                    info!("Loading HH polarization data");
                    hh_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
                } else {
                    warn!("HH measurement file not found, skipping product");
                    return Ok(None);
                }

                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
                } else {
                    warn!("HV measurement file not found, skipping product");
                    return Ok(None);
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
                    vv_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
                }
                if let Some(path) = vh_path {
                    info!("Loading VH polarization data");
                    vh_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
                }
                if let Some(path) = hh_path {
                    info!("Loading HH polarization data");
                    hh_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
                }
                if let Some(path) = hv_path {
                    info!("Loading HV polarization data");
                    hv_data = Some(Self::load_polarization_data(&path, &mut metadata)?);
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

    /// Identify VV and VH polarization files in the measurement directory
    fn identify_polarization_files(
        measurement_path: &Path,
        available_polarizations: &[String],
    ) -> Result<
        (
            Option<PathBuf>,
            Option<PathBuf>,
            Option<PathBuf>,
            Option<PathBuf>,
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
                        info!("Found VV file: {:?}", path);
                    } else if name_str.contains("vh") {
                        vh_path = Some(path.clone());
                        info!("Found VH file: {:?}", path);
                    } else if name_str.contains("hh") {
                        hh_path = Some(path.clone());
                        info!("Found HH file: {:?}", path);
                    } else if name_str.contains("hv") {
                        hv_path = Some(path.clone());
                        info!("Found HV file: {:?}", path);
                    }
                }
            }
        }

        // If we didn't find polarization-specific files, try to infer from available polarizations
        if vv_path.is_none() && vh_path.is_none() && hh_path.is_none() && hv_path.is_none() {
            info!(
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
                                info!("Found VV file (inferred): {:?}", path);
                                break;
                            } else if pol.to_lowercase() == "vh" {
                                vh_path = Some(path.clone());
                                info!("Found VH file (inferred): {:?}", path);
                                break;
                            } else if pol.to_lowercase() == "hh" {
                                hh_path = Some(path.clone());
                                info!("Found HH file (inferred): {:?}", path);
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

    /// Load polarization data from a file and update metadata
    fn load_polarization_data(
        file_path: &Path,
        metadata: &mut SafeMetadata,
    ) -> Result<Array2<Complex<f64>>, SafeError> {
        info!("Loading underlying data from: {:?}", file_path);

        // Use GDAL to read GeoTIFF safely for large files
        let gdal_reader = GdalSarReader::open(file_path)
            .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;

        // Extract georeferencing information from GDAL
        metadata.geotransform = Some(gdal_reader.metadata.geotransform);
        metadata.projection = Some(gdal_reader.metadata.projection.clone());
        metadata.crs = Some(gdal_reader.metadata.projection.clone());

        // Read first band into f64 array
        let arr_f64 = gdal_reader
            .read_band(1, Some(ResampleAlg::NearestNeighbour))
            .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;

        // Update metadata dimensions
        let (rows, cols) = (arr_f64.nrows(), arr_f64.ncols());
        metadata.lines = rows;
        metadata.samples = cols;

        // Convert to Complex<f64>
        let mut data = Array2::<Complex<f64>>::zeros((rows, cols));
        for ((i, j), &v) in arr_f64.indexed_iter() {
            data[[i, j]] = Complex::new(v, 0.0);
        }

        Ok(data)
    }

    /// Load polarization data with optional warp to target CRS
    fn load_polarization_data_with_options(
        file_path: &Path,
        metadata: &mut SafeMetadata,
        target_crs: Option<&str>,
        resample_alg: Option<ResampleAlg>,
    ) -> Result<Array2<Complex<f64>>, SafeError> {
        if let Some(dst) = target_crs {
            info!("Warping to target CRS: {}", dst);
            let tmp_in = file_path;
            // Build a dedicated temp output file path outside SAFE tree
            let stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("warped");
            // Create a unique temporary file using tempfile crate (auto-cleanup fallback)
            let mut tmp_builder = tempfile::Builder::new();
            let tmp_file = tmp_builder
                .prefix(&format!("{}_", stem))
                .suffix("_warped.tif")
                .tempfile()
                .map_err(|e| SafeError::Parse(format!("tempfile error: {}", e)))?;
            let tmp_out = tmp_file.path().to_path_buf();
            let resample_str = match resample_alg.unwrap_or(ResampleAlg::Bilinear) {
                ResampleAlg::NearestNeighbour => "near",
                ResampleAlg::Bilinear => "bilinear",
                ResampleAlg::Cubic => "cubic",
                _ => "bilinear",
            };
            // Determine source CRS from dataset
            let src_ds = Dataset::open(tmp_in)
                .map_err(|e| SafeError::Parse(format!("GDAL open error: {}", e)))?;
            let ds_proj = src_ds.projection();
            // Helper: parse EPSG from a WKT string
            let parse_epsg = |wkt: &str| -> Option<String> {
                let key = "AUTHORITY[\"EPSG\",\"";
                if let Some(idx) = wkt.rfind(key) {
                    let start = idx + key.len();
                    if let Some(end) = wkt[start..].find('"') {
                        let code = &wkt[start..start + end];
                        return Some(format!("EPSG:{}", code));
                    }
                }
                None
            };
            // Guard: if dataset already has a projection equal to target, skip warping
            if !ds_proj.is_empty() {
                if let Some(epsg) = parse_epsg(&ds_proj).or_else(|| {
                    if ds_proj.starts_with("EPSG:") {
                        Some(ds_proj.clone())
                    } else {
                        None
                    }
                }) {
                    if epsg.eq_ignore_ascii_case(dst) {
                        info!("Input already in target CRS ({}); skipping warp", dst);
                        // Read directly using GDAL reader
                        let gdal_reader = GdalSarReader::open(file_path)
                            .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;
                        // Update metadata
                        metadata.geotransform = Some(gdal_reader.metadata.geotransform);
                        metadata.projection = Some(gdal_reader.metadata.projection.clone());
                        metadata.crs = Some(gdal_reader.metadata.projection.clone());
                        let arr_f64 = gdal_reader
                            .read_band(1, Some(ResampleAlg::NearestNeighbour))
                            .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;
                        let (rows, cols) = (arr_f64.nrows(), arr_f64.ncols());
                        metadata.lines = rows;
                        metadata.samples = cols;
                        let mut data = Array2::<Complex<f64>>::zeros((rows, cols));
                        for ((i, j), &v) in arr_f64.indexed_iter() {
                            data[[i, j]] = Complex::new(v, 0.0);
                        }
                        return Ok(data);
                    }
                }
            }
            // Choose warp mode: if dataset has no projection, use GCP+tps with s_srs; otherwise, standard warp
            let mut args: Vec<String> = vec![
                "-of".into(),
                "GTiff".into(),
                "-overwrite".into(),
                "-r".into(),
                resample_str.into(),
            ];
            if ds_proj.is_empty() {
                // Use GCPs via thin plate spline to geolocate Sentinel-1 GRD rasters
                args.push("-tps".into());
                // Source SRS from GCP projection (fallback to EPSG:4326)
                let src_gcp_proj = src_ds.gcp_projection().unwrap_or_else(|| "".to_string());
                let src_srs = if src_gcp_proj.trim().is_empty() {
                    "EPSG:4326".to_string()
                } else {
                    src_gcp_proj
                };
                args.push("-s_srs".into());
                args.push(src_srs);
            }
            args.push("-t_srs".into());
            args.push(dst.to_string());
            args.push(tmp_in.to_str().unwrap().to_string());
            args.push(tmp_out.to_str().unwrap().to_string());
            let status = Command::new("gdalwarp")
                .args(args.iter().map(|s| s.as_str()))
                .status()
                .map_err(|e| SafeError::Parse(format!("gdalwarp exec error: {}", e)))?;
            if !status.success() {
                // Best-effort cleanup
                let _ = std::fs::remove_file(&tmp_out);
                return Err(SafeError::Parse("gdalwarp failed".to_string()));
            }
            let ds: Dataset = Dataset::open(&tmp_out)
                .map_err(|e| SafeError::Parse(format!("GDAL open warped error: {}", e)))?;

            // Update metadata from warped dataset
            if let Ok(gt) = ds.geo_transform() {
                metadata.geotransform = Some(gt);
            }
            let proj = ds.projection();
            if !proj.is_empty() {
                metadata.projection = Some(proj.clone());
                metadata.crs = Some(proj);
            }

            // Read first band as f64 array
            let (size_x, size_y) = ds.raster_size();
            let band = ds
                .rasterband(1)
                .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;
            let buf: Buffer<f64> = band
                .read_as((0, 0), (size_x, size_y), (size_x, size_y), None)
                .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;
            let data_vec = buf.data().to_vec();
            let mut data = Array2::<Complex<f64>>::zeros((size_y as usize, size_x as usize));
            for i in 0..(size_y as usize) {
                for j in 0..(size_x as usize) {
                    let v = data_vec[i * (size_x as usize) + j];
                    data[[i, j]] = Complex::new(v, 0.0);
                }
            }
            // Update dims
            metadata.lines = size_y as usize;
            metadata.samples = size_x as usize;
            // Clean up temporary warped file
            let _ = std::fs::remove_file(&tmp_out);
            return Ok(data);
        }
        // Fallback: no warp
        Self::load_polarization_data(file_path, metadata)
    }

    fn parse_comprehensive_metadata(base_path: &Path) -> Result<SafeMetadata, SafeError> {
        // Initialize metadata with conversion provenance
        let mut meta = SafeMetadata {
            instrument: String::new(),
            platform: String::new(),
            acquisition_start: String::new(),
            acquisition_stop: String::new(),
            orbit_number: 0,
            polarizations: Vec::new(),
            lines: 0,
            samples: 0,
            product_type: String::new(),
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

        // Parse manifest.safe for product-level metadata
        let manifest_path = base_path.join("manifest.safe");
        if manifest_path.exists() {
            meta = Self::parse_manifest_safe(&manifest_path, meta)?;
        }

        // Parse annotation files for detailed metadata
        let annotation_path = base_path.join("annotation");
        if annotation_path.is_dir() {
            meta = Self::parse_annotation_files(&annotation_path, meta)?;
        }

        Ok(meta)
    }

    fn parse_manifest_safe(path: &Path, mut meta: SafeMetadata) -> Result<SafeMetadata, SafeError> {
        let mut reader = Reader::from_file(path)?;
        reader.trim_text(true);
        let mut buf = Vec::new();
        let mut curr = String::new();
        let mut _in_metadata_section = false;
        let mut in_platform_section = false;
        let mut in_acquisition_period = false;
        let mut in_orbit_reference = false;
        let mut _in_general_product_info = false;
        let mut _in_processing = false;
        let mut in_facility = false;
        let mut in_software = false;
        let mut in_standalone_product_info = false;
        let mut in_orbit_properties = false;

        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Start(ref e) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    curr = tag.clone();
                    match tag.as_str() {
                        "metadataSection" => _in_metadata_section = true,
                        "platform" => in_platform_section = true,
                        "acquisitionPeriod" => in_acquisition_period = true,
                        "orbitReference" => in_orbit_reference = true,
                        "generalProductInformation" => _in_general_product_info = true,
                        "processing" => _in_processing = true,
                        "facility" => in_facility = true,
                        "software" => in_software = true,
                        "standAloneProductInformation" => in_standalone_product_info = true,
                        "orbitProperties" => in_orbit_properties = true,
                        _ => {}
                    }
                }
                Event::End(ref e) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag.as_str() {
                        "metadataSection" => _in_metadata_section = false,
                        "platform" => in_platform_section = false,
                        "acquisitionPeriod" => in_acquisition_period = false,
                        "orbitReference" => in_orbit_reference = false,
                        "generalProductInformation" => _in_general_product_info = false,
                        "processing" => _in_processing = false,
                        "facility" => in_facility = false,
                        "software" => in_software = false,
                        "standAloneProductInformation" => in_standalone_product_info = false,
                        "orbitProperties" => in_orbit_properties = false,
                        _ => {}
                    }
                }
                Event::Text(e) => {
                    let txt = e.unescape().unwrap();
                    match curr.as_str() {
                        // Platform information
                        "familyName" if in_platform_section => meta.platform = txt.to_string(),
                        "instrument" if in_platform_section => meta.instrument = txt.to_string(),
                        "mode" if in_platform_section => {
                            meta.instrument_mode = Some(txt.to_string())
                        }

                        // Acquisition period
                        "startTime" if in_acquisition_period => {
                            meta.acquisition_start = txt.to_string()
                        }
                        "stopTime" if in_acquisition_period => {
                            meta.acquisition_stop = txt.to_string()
                        }

                        // Orbit information
                        "orbitNumber" if in_orbit_reference => {
                            meta.orbit_number = txt.parse().unwrap_or(0)
                        }
                        "pass" if in_orbit_properties => {
                            meta.pass_direction = Some(txt.to_string())
                        }

                        // Product information
                        "productType" if in_standalone_product_info => {
                            meta.product_type = txt.to_string()
                        }
                        "missionDataTakeID" if in_standalone_product_info => {
                            meta.data_take_id = Some(txt.to_string())
                        }
                        "productClass" if in_standalone_product_info => {
                            meta.processing_level = Some(txt.to_string())
                        }
                        "transmitterReceiverPolarisation" if in_standalone_product_info => {
                            meta.polarizations.push(txt.to_string());
                        }

                        // Processing information
                        "name" if in_facility => meta.processing_center = Some(txt.to_string()),
                        "name" if in_software => meta.software_version = Some(txt.to_string()),
                        "version" if in_software => meta.software_version = Some(txt.to_string()),

                        _ => {}
                    }
                }
                Event::Eof => break,
                _ => {}
            }
            buf.clear();
        }
        Ok(meta)
    }

    fn parse_annotation_files(
        annotation_path: &Path,
        mut meta: SafeMetadata,
    ) -> Result<SafeMetadata, SafeError> {
        // Find and parse annotation XML files
        for entry in fs::read_dir(annotation_path)? {
            let path = entry?.path();
            if path.extension().map(|e| e == "xml").unwrap_or(false) {
                meta = Self::parse_annotation_xml(&path, meta)?;
            }
        }
        Ok(meta)
    }

    fn parse_annotation_xml(
        path: &Path,
        mut meta: SafeMetadata,
    ) -> Result<SafeMetadata, SafeError> {
        let mut reader = Reader::from_file(path)?;
        reader.trim_text(true);
        let mut buf = Vec::new();
        let mut curr = String::new();
        let mut in_product_info = false;
        let mut in_downlink_info = false;
        let mut in_orbit_state = false;
        let mut in_image_annotation = false;
        let mut _in_geolocation_grid = false;
        let mut in_ads_header = false;
        let mut _in_quality_info = false;
        let mut _in_general_annotation = false;
        let mut in_downlink_values = false;
        let mut downlink_fields = 0;
        let mut state_vectors: Vec<(f64, f64, f64)> = Vec::new();
        let mut current_vector: (f64, f64, f64) = (0.0, 0.0, 0.0);

        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Start(ref e) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    curr = tag.clone();
                    match tag.as_str() {
                        "adsHeader" => in_ads_header = true,
                        "qualityInformation" => _in_quality_info = true,
                        "generalAnnotation" => _in_general_annotation = true,
                        "productInformation" => in_product_info = true,
                        "downlinkInformation" if downlink_fields == 0 => in_downlink_info = true,
                        "downlinkValues" => in_downlink_values = true,
                        "orbitStateVector" => in_orbit_state = true,
                        "imageAnnotation" => in_image_annotation = true,
                        "geolocationGrid" => _in_geolocation_grid = true,
                        _ => {}
                    }
                }
                Event::End(ref e) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    match tag.as_str() {
                        "adsHeader" => in_ads_header = false,
                        "qualityInformation" => _in_quality_info = false,
                        "generalAnnotation" => _in_general_annotation = false,
                        "productInformation" => in_product_info = false,
                        "downlinkInformation" if in_downlink_info => {
                            in_downlink_info = false;
                            downlink_fields += 1;
                        }
                        "downlinkValues" => in_downlink_values = false,
                        "orbitStateVector" => {
                            in_orbit_state = false;
                            state_vectors.push(current_vector);
                            current_vector = (0.0, 0.0, 0.0);
                        }
                        "imageAnnotation" => in_image_annotation = false,
                        "geolocationGrid" => _in_geolocation_grid = false,
                        _ => {}
                    }
                }
                Event::Text(e) => {
                    let txt = e.unescape().unwrap();
                    match curr.as_str() {
                        // ADS Header information
                        "missionId" if in_ads_header => meta.platform = txt.to_string(),
                        "productType" if in_ads_header => meta.product_type = txt.to_string(),
                        "polarisation" if in_ads_header => meta.polarizations.push(txt.to_string()),
                        "mode" if in_ads_header => meta.instrument_mode = Some(txt.to_string()),
                        "startTime" if in_ads_header => meta.acquisition_start = txt.to_string(),
                        "stopTime" if in_ads_header => meta.acquisition_stop = txt.to_string(),
                        "absoluteOrbitNumber" if in_ads_header => {
                            meta.orbit_number = txt.parse().unwrap_or(0)
                        }
                        "missionDataTakeId" if in_ads_header => {
                            meta.data_take_id = Some(txt.to_string())
                        }

                        // Product information
                        "pass" if in_product_info => meta.pass_direction = Some(txt.to_string()),
                        "rangeSamplingRate" if in_product_info => {
                            meta.range_sampling_rate = txt.parse().ok()
                        }
                        "radarFrequency" if in_product_info => {
                            meta.radar_frequency = txt.parse().ok()
                        }
                        "azimuthSteeringRate" if in_product_info => {
                            // Additional SAR parameter
                        }

                        // Downlink information
                        "prf" if in_downlink_info && meta.prf.is_none() => {
                            meta.prf = txt.parse().ok()
                        }

                        // Downlink values
                        "txPulseLength" if in_downlink_values && meta.tx_pulse_length.is_none() => {
                            meta.tx_pulse_length = txt.parse().ok()
                        }
                        "txPulseRampRate"
                            if in_downlink_values && meta.tx_pulse_ramp_rate.is_none() =>
                        {
                            meta.tx_pulse_ramp_rate = txt.parse().ok()
                        }

                        // Image annotation for slant range and pixel spacing
                        "slantRangeTime"
                            if in_image_annotation && meta.slant_range_near.is_none() =>
                        {
                            let srt = txt.parse::<f64>().unwrap_or(0.0);
                            meta.slant_range_near = Some(srt * 299_792_458.0 / 2.0);
                        }
                        "rangePixelSpacing" if in_image_annotation => {
                            meta.pixel_spacing_range = txt.parse().ok()
                        }
                        "azimuthPixelSpacing" if in_image_annotation => {
                            meta.pixel_spacing_azimuth = txt.parse().ok()
                        }

                        // Orbit state vectors
                        "vx" if in_orbit_state => current_vector.0 = txt.parse().unwrap_or(0.0),
                        "vy" if in_orbit_state => current_vector.1 = txt.parse().unwrap_or(0.0),
                        "vz" if in_orbit_state => current_vector.2 = txt.parse().unwrap_or(0.0),

                        // Image dimensions
                        "lines" => meta.lines = txt.parse().unwrap_or(0),
                        "samplesPerLine" => meta.samples = txt.parse().unwrap_or(0),
                        "numberOfSamples" => meta.samples = txt.parse().unwrap_or(0),

                        _ => {}
                    }
                }
                Event::Eof => break,
                _ => {}
            }
            buf.clear();
        }

        // Compute velocity from state vectors
        if !state_vectors.is_empty() {
            let (vx, vy, vz) = state_vectors[state_vectors.len() / 2];
            meta.velocity = Some((vx.powi(2) + vy.powi(2) + vz.powi(2)).sqrt());
        }

        Ok(meta)
    }

    /// Access parsed metadata
    pub fn metadata(&self) -> &SafeMetadata {
        &self.metadata
    }

    /// Retrieve the image data as a full Array2 (returns VV data if available, otherwise VH)
    pub fn data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        if let Some(ref arr) = self.vv_data {
            Ok(arr.clone())
        } else if let Some(ref arr) = self.vh_data {
            Ok(arr.clone())
        } else {
            Err(SafeError::MissingField("no polarization data available"))
        }
    }

    /// Get VV data
    pub fn vv_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        let arr = self
            .vv_data
            .as_ref()
            .ok_or(SafeError::MissingField("vv_data"))?;
        Ok(arr.clone())
    }

    /// Get VH data
    pub fn vh_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        let arr = self
            .vh_data
            .as_ref()
            .ok_or(SafeError::MissingField("vh_data"))?;
        Ok(arr.clone())
    }

    /// Get HH data
    pub fn hh_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        let arr = self
            .hh_data
            .as_ref()
            .ok_or(SafeError::MissingField("hh_data"))?;
        Ok(arr.clone())
    }

    /// Get HV data
    pub fn hv_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        let arr = self
            .hv_data
            .as_ref()
            .ok_or(SafeError::MissingField("hv_data"))?;
        Ok(arr.clone())
    }

    /// Get sum of VV and VH data (vv + vh)
    pub fn sum_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        info!("Summing VV and VH data");
        let vv = self.vv_data()?;
        let vh = self.vh_data()?;
        Ok(crate::core::processing::ops::sum_arrays(&vv, &vh))
    }

    /// Get difference of VV and VH data (vv - vh)
    pub fn difference_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        info!("Differencing VV and VH data");
        let vv = self.vv_data()?;
        let vh = self.vh_data()?;
        Ok(crate::core::processing::ops::difference_arrays(&vv, &vh))
    }

    /// Get ratio of VV and VH data (vv / vh, with zero handling)
    pub fn ratio_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        info!("Calculating ratio of VV and VH data");
        let vv = self.vv_data()?;
        let vh = self.vh_data()?;
        Ok(crate::core::processing::ops::ratio_arrays(&vv, &vh))
    }

    /// Get normalized difference of VV and VH data ((vv - vh) / (vv + vh))
    pub fn normalized_diff_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        info!("Calculating normalized difference of VV and VH data");
        let vv = self.vv_data()?;
        let vh = self.vh_data()?;
        Ok(crate::core::processing::ops::normalized_diff_arrays(
            &vv, &vh,
        ))
    }

    /// Get log ratio of VV and VH data (10 * log10(vv / vh))
    pub fn log_ratio_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        info!("Calculating log ratio of VV and VH data");
        let vv = self.vv_data()?;
        let vh = self.vh_data()?;
        Ok(crate::core::processing::ops::log_ratio_arrays(&vv, &vh))
    }

    // HH/HV pair operations
    /// Get sum of HH and HV data (hh + hv)
    pub fn sum_hh_hv_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        info!("Summing HH and HV data");
        let hh = self.hh_data()?;
        let hv = self.hv_data()?;
        Ok(crate::core::processing::ops::sum_arrays(&hh, &hv))
    }

    /// Get difference of HH and HV data (hh - hv)
    pub fn difference_hh_hv_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        info!("Differencing HH and HV data");
        let hh = self.hh_data()?;
        let hv = self.hv_data()?;
        Ok(crate::core::processing::ops::difference_arrays(&hh, &hv))
    }

    /// Get ratio of HH and HV data (hh / hv, with zero handling)
    pub fn ratio_hh_hv_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        info!("Calculating ratio of HH and HV data");
        let hh = self.hh_data()?;
        let hv = self.hv_data()?;
        Ok(crate::core::processing::ops::ratio_arrays(&hh, &hv))
    }

    /// Get normalized difference of HH and HV data ((hh - hv) / (hh + hv))
    pub fn normalized_diff_hh_hv_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        info!("Calculating normalized difference of HH and HV data");
        let hh = self.hh_data()?;
        let hv = self.hv_data()?;
        Ok(crate::core::processing::ops::normalized_diff_arrays(
            &hh, &hv,
        ))
    }

    /// Get log ratio of HH and HV data (10 * log10(hh / hv))
    pub fn log_ratio_hh_hv_data(&self) -> Result<Array2<Complex<f64>>, SafeError> {
        info!("Calculating log ratio of HH and HV data");
        let hh = self.hh_data()?;
        let hv = self.hv_data()?;
        Ok(crate::core::processing::ops::log_ratio_arrays(&hh, &hv))
    }

    /// Get a string representation of available polarizations for error reporting
    pub fn get_available_polarizations(&self) -> String {
        let mut available = Vec::new();

        if self.vv_data.is_some() {
            available.push("VV");
        }
        if self.vh_data.is_some() {
            available.push("VH");
        }
        if self.hh_data.is_some() {
            available.push("HH");
        }
        if self.hv_data.is_some() {
            available.push("HV");
        }

        if available.is_empty() {
            "none".to_string()
        } else {
            available.join(", ")
        }
    }
}
