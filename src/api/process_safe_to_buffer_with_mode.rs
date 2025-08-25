//! Process SAFE input to in-memory buffers with explicit synthetic RGB mode

use std::path::Path;
use crate::core::processing::pipeline::process_scalar_data_pipeline;
use crate::core::processing::resize::resize_image_data;
use crate::core::processing::synthetic_rgb::create_synthetic_rgb_by_mode_and_strategy;
use crate::error::{Error, Result};
use crate::io::sentinel1::SafeReader;
use crate::types::{
    AutoscaleStrategy, BitDepth, OutputFormat, Polarization,
    SyntheticRgbMode,
};
use super::processed_image::ProcessedImage;
use super::process_safe_to_buffer::process_safe_to_buffer;

/// Process a SAFE input to in-memory buffers with explicit synthetic RGB mode for multiband JPEG
pub fn process_safe_to_buffer_with_mode(
    input: &Path,
    polarization: Polarization,
    autoscale: AutoscaleStrategy,
    bit_depth: BitDepth,
    target_size: Option<usize>,
    pad: bool,
    output_format: OutputFormat,
    synrgb_mode: SyntheticRgbMode,
) -> Result<ProcessedImage> {
    let reader = SafeReader::open_with_options(
        input,
        polarization,
        None,
        None,
        target_size,
    )?;

    match (output_format, polarization) {
        // Delegate to existing branches with_same logic; only Multiband+JPEG uses mode
        (OutputFormat::JPEG, Polarization::Multiband) => {
            let (band1, band2) = if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                (reader.vv_data()?, reader.vh_data()?)
            } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                (reader.hh_data()?, reader.hv_data()?)
            } else {
                return Err(Error::Processing(format!(
                    "Multiband requires VV+VH or HH+HV; available: {}",
                    reader.get_available_polarizations()
                )));
            };

            let (db1, _m1, s1_u8, _s1_u16) =
                process_scalar_data_pipeline(band1, BitDepth::U8, autoscale);
            let (rows, cols) = db1.dim();
            let (final_cols, final_rows, final1_u8, _) =
                resize_image_data(&s1_u8, None, cols, rows, target_size, BitDepth::U8, pad)
                    .map_err(|e| Error::external(e))?;

            let (_db2, _m2, s2_u8, _s2_u16) =
                process_scalar_data_pipeline(band2, BitDepth::U8, autoscale);
            let (_c2, _r2, final2_u8, _) =
                resize_image_data(&s2_u8, None, cols, rows, target_size, BitDepth::U8, pad)
                    .map_err(|e| Error::external(e))?;

            let rgb = create_synthetic_rgb_by_mode_and_strategy(
                synrgb_mode,
                autoscale,
                &final1_u8,
                &final2_u8,
            );

            Ok(ProcessedImage {
                width: final_cols,
                height: final_rows,
                bit_depth: BitDepth::U8,
                format: OutputFormat::JPEG,
                gray: None,
                gray16: None,
                rgb: Some(rgb),
                gray_band2: None,
                gray16_band2: None,
                metadata: reader.metadata.clone(),
            })
        }
        _ => process_safe_to_buffer(
            input,
            polarization,
            autoscale,
            bit_depth,
            target_size,
            pad,
            output_format,
        ),
    }
}
