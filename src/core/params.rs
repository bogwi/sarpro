use serde::{Deserialize, Serialize};

use crate::types::OutputFormat;
use crate::{AutoscaleStrategy, BitDepthArg, InputFormat, Polarization};

/// Processing parameters suitable for config files and GUI presets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingParams {
    pub format: OutputFormat,
    pub input_format: InputFormat,
    pub bit_depth: BitDepthArg,
    pub polarization: Polarization,
    pub autoscale: AutoscaleStrategy,
    /// Target long side in pixels; None means original size
    pub size: Option<usize>,
    /// If true, zero-pad to square after resizing
    pub pad: bool,
}

impl Default for ProcessingParams {
    fn default() -> Self {
        Self {
            format: OutputFormat::TIFF,
            input_format: InputFormat::Safe,
            bit_depth: BitDepthArg::U8,
            polarization: Polarization::Vv,
            autoscale: AutoscaleStrategy::Tamed,
            size: None,
            pad: false,
        }
    }
}


