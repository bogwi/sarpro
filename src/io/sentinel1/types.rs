/// Sentinel-1 product types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductType {
    GRD,
}

/// Internal target CRS argument used to defer resolution of 'auto' until reader open
#[derive(Debug, Clone)]
pub enum TargetCrsArg {
    None,
    Auto,
    Custom(String),
}

/// Metadata extracted from SAFE
#[derive(Debug, Clone, Default)]
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
