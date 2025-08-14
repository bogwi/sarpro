use gdal::raster::ResampleAlg;
use gdal::{Dataset, Metadata, errors::GdalError as GdalCrateError};
use ndarray::Array2;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

/// Errors encountered when using GDAL reader
#[derive(Debug, Error)]
pub enum GdalError {
    #[error("GDAL error: {0}")]
    Gdal(#[from] GdalCrateError),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("Dimension mismatch: expected {0}x{1}, got {2}x{3}")]
    DimensionMismatch(usize, usize, usize, usize),
}

/// Metadata extracted from a GDAL-supported dataset
#[derive(Debug, Clone)]
pub struct GdalMetadata {
    /// Width (pixels) of the raster
    pub size_x: usize,
    /// Height (lines) of the raster
    pub size_y: usize,
    /// Number of raster bands
    pub bands: usize,
    /// Affine geotransform coefficients ([origin_x, pixel_width, rot_x, origin_y, rot_y, pixel_height])
    pub geotransform: [f64; 6],
    /// Projection in WKT format
    pub projection: String,
    /// Additional metadata key-value pairs
    pub metadata: HashMap<String, String>,
}

/// Reader for generic geospatial formats via GDAL
pub struct GdalSarReader {
    pub dataset: Dataset,
    pub metadata: GdalMetadata,
}

// Helper to extract EPSG code from WKT authority tag
fn parse_epsg(wkt: &str) -> Option<String> {
    const KEY: &str = "AUTHORITY[\"EPSG\",\"";
    if let Some(idx) = wkt.rfind(KEY) {
        let start = idx + KEY.len();
        if let Some(end) = wkt[start..].find('"') {
            let code = &wkt[start..start + end];
            return Some(format!("EPSG:{}", code));
        }
    }
    None
}

impl GdalSarReader {
    /// Open a GDAL-supported dataset (e.g., GeoTIFF, NetCDF, HDF5, ENVI)
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, GdalError> {
        let dataset = Dataset::open(path.as_ref())?;
        let (size_x, size_y) = dataset.raster_size();
        let bands = dataset.raster_count() as usize;
        if bands == 0 {
            return Err(GdalError::UnsupportedFormat("No raster bands found".into()));
        }
        let geotransform = match dataset.geo_transform() {
            Ok(gt) => gt,
            Err(_) => [0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        };
        let mut proj = dataset.projection();
        if proj.is_empty() {
            // Fallback to GCP projection if available
            if let Some(gcp_proj) = dataset.gcp_projection() {
                if !gcp_proj.is_empty() {
                    proj = gcp_proj;
                }
            }
        }
        let projection = if proj.starts_with("EPSG:") {
            proj
        } else if let Some(code) = parse_epsg(&proj) {
            code
        } else {
            proj
        };
        // Collect metadata entries (domain "")
        let mut metadata_map = HashMap::new();
        if let Some(entries) = dataset.metadata_domain("") {
            for entry in entries {
                if let Some((key, val)) = entry.split_once('=') {
                    metadata_map.insert(key.to_string(), val.to_string());
                }
            }
        }
        Ok(GdalSarReader {
            dataset,
            metadata: GdalMetadata {
                size_x: size_x as usize,
                size_y: size_y as usize,
                bands,
                geotransform,
                projection,
                metadata: metadata_map,
            },
        })
    }

    /// Read a single band (1-based index) as an f64 ndarray of shape (height, width)
    pub fn read_band(
        &self,
        index: usize,
        e_resample_alg: Option<ResampleAlg>,
    ) -> Result<Array2<f64>, GdalError> {
        if index == 0 || index > self.metadata.bands {
            return Err(GdalError::UnsupportedFormat(format!(
                "Band index {} out of range",
                index
            )));
        }
        // Load the raster band
        let band = self.dataset.rasterband(index)?;
        // Define full window based on metadata
        let window = (self.metadata.size_x, self.metadata.size_y);
        // Read data into GDAL Buffer
        let buf = band.read_as::<f64>(
            (0, 0),         // offset
            window,         // window size
            window,         // shape
            e_resample_alg, // default resampling
        )?;
        // Convert Buffer into ndarray
        let data_vec = buf.data().to_vec();
        let array = Array2::from_shape_vec((self.metadata.size_y, self.metadata.size_x), data_vec)
            .map_err(|_| {
                GdalError::DimensionMismatch(
                    self.metadata.size_x,
                    self.metadata.size_y,
                    self.metadata.size_x,
                    self.metadata.size_y,
                )
            })?;
        Ok(array)
    }

    /// Read all bands into a vector of f64 ndarrays
    pub fn _read_all_bands(&self) -> Result<Vec<Array2<f64>>, GdalError> {
        let mut result = Vec::with_capacity(self.metadata.bands);
        for idx in 1..=self.metadata.bands {
            result.push(self.read_band(idx, Some(ResampleAlg::NearestNeighbour))?);
        }
        Ok(result)
    }
}
