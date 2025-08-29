use clap::Parser;
use std::path::PathBuf;

use sarpro::types::{OutputFormat, SyntheticRgbMode};
use sarpro::{AutoscaleStrategy, BitDepthArg, InputFormat, Polarization};

#[derive(Parser)]
#[command(name = "sarpro", version, about = "SARPRO CLI")]
pub struct CliArgs {
    /// Input SAFE directory (single file mode) local or remote HTTP via GDAL VSI (unzipped .SAFE)
    #[arg(short, long)]
    pub input: Option<PathBuf>,

    /// Input directory containing SAFE subdirectories (batch mode)
    #[arg(long)]
    pub input_dir: Option<PathBuf>,

    /// Output filename (single file mode)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Output directory for batch processing (batch mode)
    #[arg(long)]
    pub output_dir: Option<PathBuf>,

    /// Output format (tiff or jpeg)
    #[arg(short = 'f', long, value_enum, default_value_t = OutputFormat::TIFF)]
    pub format: OutputFormat,

    /// Input format (only SAFE supported currently)
    #[arg(long, value_enum, default_value_t = InputFormat::Safe)]
    pub input_format: InputFormat,

    /// Output bit depth (8 or 16)
    #[arg(long, value_enum, default_value_t = BitDepthArg::U8)]
    pub bit_depth: BitDepthArg,

    /// Polarization mode (vv, vh, hh, hv or multiband)
    #[arg(long, value_enum, default_value_t = Polarization::Vv)]
    pub polarization: Polarization,

    /// Autoscaling strategy (standard, robust, adaptive, equalized, clahe, default)
    #[arg(long, value_enum, default_value_t = AutoscaleStrategy::Clahe)]
    pub autoscale: AutoscaleStrategy,

    /// Image size for scaling. Options:
    /// - Predefined: 512, 1024, 2048
    /// - Custom: any positive integer (e.g., 1536)
    /// - Original: "original" (no scaling)
    #[arg(long, default_value = "original")]
    pub size: String,

    /// Enable logging
    #[arg(long, default_value_t = false)]
    pub log: bool,

    /// Batch mode: continue processing other files when encountering unsupported products
    #[arg(long, default_value_t = false)]
    pub batch: bool,

    /// Add padding to make square images (centers image and adds zero padding to top/bottom)
    #[arg(long, default_value_t = false)]
    pub pad: bool,

    /// Optional target CRS for map reprojection (e.g., EPSG:4326, EPSG:32633).
    /// Special values: 'auto' (detect UTM from metadata), 'none' (disable)
    #[arg(long)]
    pub target_crs: Option<String>,

    /// Optional resampling algorithm (nearest, bilinear, cubic, lanczos)
    #[arg(long)]
    pub resample_alg: Option<String>,

    /// Synthetic RGB mode (only used when --format jpeg and --polarization multiband)
    #[arg(long = "synrgb-mode", value_enum, default_value_t = SyntheticRgbMode::Default)]
    pub synrgb_mode: SyntheticRgbMode,

    // --- Phase 1 additions (non-breaking) ---
    /// Load inputs from a STAC Item (local file path or HTTP URL)
    #[arg(long = "stac-item")]
    pub stac_item: Option<String>,

    /// Open a remote SAFE packaged as a ZIP via VSICURL/VSIZIP
    #[arg(long = "safe-zip-url")]
    pub safe_zip_url: Option<String>,

    /// Treat a remote URL as a directory and list SAFE ZIPs via GDAL VSICURL
    /// Example: --safe-zip-dir-url http://127.0.0.1:8000/ZIPs
    #[arg(long = "safe-zip-dir-url")]
    pub safe_zip_dir_url: Option<String>,

    /// Explicit measurement URLs (VSICURL). Use with --polarization appropriately
    #[arg(long = "vv-url")]
    pub vv_url: Option<String>,
    #[arg(long = "vh-url")]
    pub vh_url: Option<String>,
    #[arg(long = "hh-url")]
    pub hh_url: Option<String>,
    #[arg(long = "hv-url")]
    pub hv_url: Option<String>,

    /// Newline-separated list of inputs (STAC Items, SAFE ZIP URLs, local SAFE paths, or direct band URLs)
    #[arg(long = "input-list")]
    pub input_list: Option<PathBuf>,

    /// OAuth bearer token to access authenticated endpoints (mapped to GDAL headers later)
    #[arg(long = "auth-bearer")]
    pub auth_bearer: Option<String>,

    /// HTTP timeout in seconds for remote reads (GDAL)
    #[arg(long = "http-timeout", default_value_t = 120u64)]
    pub http_timeout: u64,

    /// Number of HTTP retries for transient errors
    #[arg(long = "http-retries", default_value_t = 3u32)]
    pub http_retries: u32,

    /// Base backoff in milliseconds between retries
    #[arg(long = "http-backoff-ms", default_value_t = 500u64)]
    pub http_backoff_ms: u64,

    /// Optional directory for remote cache/materialization
    #[arg(long = "remote-cache-dir")]
    pub remote_cache_dir: Option<PathBuf>,
    
    /// Maximum number of concurrent entries to process when using --input-list
    #[arg(long = "max-concurrent")]
    pub max_concurrent: Option<usize>,
    
}
