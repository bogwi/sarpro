use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::types::{OutputFormat, SyntheticRgbMode};
use crate::{AutoscaleStrategy, BitDepthArg, InputFormat, Polarization};

/// Processing parameters suitable for config files and GUI presets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingParams {
    pub format: OutputFormat,
    pub input_format: InputFormat,
    pub bit_depth: BitDepthArg,
    pub polarization: Polarization,
    pub autoscale: AutoscaleStrategy,
    /// Synthetic RGB mode for multiband JPEG outputs; ignored otherwise
    pub synrgb_mode: SyntheticRgbMode,
    /// Target long side in pixels; None means original size
    pub size: Option<usize>,
    /// If true, zero-pad to square after resizing
    pub pad: bool,
    /// Optional target CRS for map reprojection (e.g., "EPSG:4326", "EPSG:32633")
    pub target_crs: Option<String>,
    /// Optional resampling algorithm name (nearest, bilinear, cubic)
    pub resample_alg: Option<String>,
}

impl Default for ProcessingParams {
    fn default() -> Self {
        Self {
            format: OutputFormat::TIFF,
            input_format: InputFormat::Safe,
            bit_depth: BitDepthArg::U8,
            polarization: Polarization::Vv,
            autoscale: AutoscaleStrategy::Clahe,
            synrgb_mode: SyntheticRgbMode::Default,
            size: None,
            pad: false,
            target_crs: None,
            resample_alg: Some("lanczos".to_string()),
        }
    }
}

/// Internal, unified processing options aggregate for Phase 1+
/// This keeps existing public `ProcessingParams` intact while allowing
/// new capabilities (RTC, masking, filters, tiling, COG, STAC, remote I/O)
/// to be threaded end-to-end without CLI/GUI churn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingOptions {
    // Existing public parameters mirrored here for convenience and stability
    pub format: OutputFormat,
    pub input_format: InputFormat,
    pub bit_depth: BitDepthArg,
    pub polarization: Polarization,
    pub autoscale: AutoscaleStrategy,
    pub synrgb_mode: SyntheticRgbMode,
    pub size: Option<usize>,
    pub pad: bool,
    pub target_crs: Option<String>,
    pub resample_alg: Option<String>,

    // Phase 1+ option groups
    pub rtc: RtcOptions,
    pub mask: MaskOptions,
    pub filter: FilterOptions,
    pub tiling: TilingOptions,
    pub cog: CogOptions,
    pub stac: StacOptions,
    pub io: IoOptions,
    pub http: HttpOptions,
}

impl Default for ProcessingOptions {
    fn default() -> Self {
        let base = ProcessingParams::default();
        Self {
            format: base.format,
            input_format: base.input_format,
            bit_depth: base.bit_depth,
            polarization: base.polarization,
            autoscale: base.autoscale,
            synrgb_mode: base.synrgb_mode,
            size: base.size,
            pad: base.pad,
            target_crs: base.target_crs,
            resample_alg: base.resample_alg,

            rtc: RtcOptions::default(),
            mask: MaskOptions::default(),
            filter: FilterOptions::default(),
            tiling: TilingOptions::default(),
            cog: CogOptions::default(),
            stac: StacOptions::default(),
            io: IoOptions::default(),
            http: HttpOptions::default(),
        }
    }
}

impl ProcessingOptions {
    /// Create `ProcessingOptions` from existing public `ProcessingParams` preserving defaults.
    pub fn from_params(params: &ProcessingParams) -> Self {
        Self {
            format: params.format,
            input_format: params.input_format,
            bit_depth: params.bit_depth,
            polarization: params.polarization,
            autoscale: params.autoscale,
            synrgb_mode: params.synrgb_mode,
            size: params.size,
            pad: params.pad,
            target_crs: params.target_crs.clone(),
            resample_alg: params.resample_alg.clone(),

            // Newly introduced groups keep their own defaults for Phase 1
            rtc: RtcOptions::default(),
            mask: MaskOptions::default(),
            filter: FilterOptions::default(),
            tiling: TilingOptions::default(),
            cog: CogOptions::default(),
            stac: StacOptions::default(),
            io: IoOptions::default(),
            http: HttpOptions::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtcOptions {
    pub enabled: bool,
    pub mode: Option<String>, // e.g., "gamma0" | "sigma0"
    pub dem: Option<String>,
    pub orbit: Option<String>,
}

impl Default for RtcOptions {
    fn default() -> Self {
        Self { enabled: false, mode: None, dem: None, orbit: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskOptions {
    pub border: Option<bool>,
    pub nesz_threshold: Option<f32>,
    pub incidence_minmax: Option<(f32, f32)>,
    pub layover_shadow: Option<bool>,
}

impl Default for MaskOptions {
    fn default() -> Self {
        Self { border: None, nesz_threshold: None, incidence_minmax: None, layover_shadow: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterOptions {
    pub method: Option<String>, // none|lee|refined-lee|kuan|frost
    pub window: Option<u8>,
    pub strength: Option<f32>,
}

impl Default for FilterOptions {
    fn default() -> Self {
        Self { method: None, window: None, strength: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TilingOptions {
    pub enabled: bool,
    pub grid: Option<String>, // webmercator|utm
    pub z: Option<u8>,
    pub size_px: Option<u32>,
    pub overlap_px: Option<u16>,
}

impl Default for TilingOptions {
    fn default() -> Self {
        Self { enabled: false, grid: None, z: None, size_px: None, overlap_px: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CogOptions {
    pub enabled: bool,
    pub block: Option<(u16, u16)>,
    pub compress: Option<String>, // deflate|zstd|lzw
    pub predictor: Option<u8>,
    pub overviews: Vec<u8>,
}

impl Default for CogOptions {
    fn default() -> Self {
        Self { enabled: false, block: None, compress: None, predictor: None, overviews: Vec::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StacOptions {
    pub in_path: Option<PathBuf>,
    pub out: Option<PathBuf>,
}

impl Default for StacOptions {
    fn default() -> Self {
        Self { in_path: None, out: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoOptions {
    pub remote: bool,
    pub cache_dir: Option<PathBuf>,
    pub zip_strategy: ZipStrategy,
}

impl Default for IoOptions {
    fn default() -> Self {
        Self { remote: false, cache_dir: None, zip_strategy: ZipStrategy::Stream }
    }
}

/// Controls how remote SAFE ZIPs are accessed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ZipStrategy {
    /// Materialize remote ZIPs into a local cache directory, then open locally.
    Materialize,
    /// Stream remote ZIPs via VSIZIP+VSICURL without local materialization.
    Stream,
}

impl IoOptions {
    /// Recompute `zip_strategy` from the presence of `cache_dir`.
    pub fn compute_zip_strategy(&mut self) {
        self.zip_strategy = if self.cache_dir.is_some() {
            ZipStrategy::Materialize
        } else {
            ZipStrategy::Stream
        };
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpOptions {
    /// HTTP timeout in seconds applied to remote requests (GDAL HTTP timeout)
    pub timeout_s: u64,
    /// Number of retries for transient HTTP errors (5xx/429)
    pub retries: u8,
    /// Base backoff in milliseconds between retries
    pub backoff_ms: u64,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self { timeout_s: 120, retries: 3, backoff_ms: 500 }
    }
}
