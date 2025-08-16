use clap::Parser;
use std::path::PathBuf;

use sarpro::types::OutputFormat;
use sarpro::{AutoscaleStrategy, BitDepthArg, InputFormat, Polarization};

#[derive(Parser)]
#[command(name = "sarpro", version, about = "SARPRO CLI")]
pub struct CliArgs {
    /// Input SAFE directory (single file mode)
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

    /// Autoscaling strategy (standard, robust, adaptive, equalized, default)
    #[arg(long, value_enum, default_value_t = AutoscaleStrategy::Tamed)]
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

    /// Optional target CRS for map reprojection (e.g., EPSG:4326, EPSG:32633)
    #[arg(long)]
    pub target_crs: Option<String>,

    /// Optional resampling algorithm (nearest, bilinear, cubic, lanczos)
    #[arg(long)]
    pub resample_alg: Option<String>,
}
