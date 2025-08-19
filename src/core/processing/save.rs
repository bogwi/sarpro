use ndarray::Array2;
use std::path::Path;
use tracing::info;

use crate::core::processing::pipeline::process_scalar_data_pipeline;
use crate::core::processing::autoscale::autoscale_db_image_tamed_synrgb_u8;
use crate::core::processing::resize::resize_image_data_with_meta;
use crate::core::processing::synthetic_rgb::create_synthetic_rgb_by_mode;
use crate::io::writers::jpeg::{write_gray_jpeg, write_rgb_jpeg};
use crate::io::writers::metadata::{
    create_jpeg_metadata_sidecar_with_overrides, create_jpeg_metadata_sidecar_with_overrides_and_extras, embed_tiff_metadata,
};
use crate::io::writers::tiff::{
    write_tiff_multiband_u8, write_tiff_multiband_u16, write_tiff_u8, write_tiff_u16,
};
use crate::io::writers::worldfile::{write_prj_file, write_world_file};
use crate::types::{
    AutoscaleStrategy, BitDepth, OutputFormat, PolarizationOperation, ProcessingOperation, SyntheticRgbMode,
};

// resize_image_data moved to crate::core::processing::resize

pub fn save_processed_image(
    processed: &Array2<f32>,
    output: &Path,
    format: OutputFormat,
    bit_depth: BitDepth,
    target_size: Option<usize>,
    metadata: Option<&crate::io::sentinel1::SafeMetadata>,
    pad: bool,
    strategy: AutoscaleStrategy,
    operation: ProcessingOperation,
) -> Result<(), Box<dyn std::error::Error>> {
    // Map operation enum to metadata label when needed
    let operation_label: Option<String> = match operation {
        ProcessingOperation::SingleBand => None,
        ProcessingOperation::MultibandVvVh => Some("multiband_vv_vh".to_string()),
        ProcessingOperation::MultibandHhHv => Some("multiband_hh_hv".to_string()),
        ProcessingOperation::PolarOp(PolarizationOperation::Sum) => Some("sum".to_string()),
        ProcessingOperation::PolarOp(PolarizationOperation::Diff) => Some("difference".to_string()),
        ProcessingOperation::PolarOp(PolarizationOperation::Ratio) => Some("ratio".to_string()),
        ProcessingOperation::PolarOp(PolarizationOperation::NDiff) => {
            Some("normalized_diff".to_string())
        }
        ProcessingOperation::PolarOp(PolarizationOperation::LogRatio) => {
            Some("log_ratio".to_string())
        }
    };
    match format {
        OutputFormat::TIFF => {
            let (db_data, _, scaled_u8, scaled_u16) =
                process_scalar_data_pipeline(processed, bit_depth, strategy);
            let shape = db_data.dim();
            let (rows, cols) = shape;

            let (final_cols, final_rows, final_u8, final_u16, scale_x, scale_y, pad_left, pad_top) =
                resize_image_data_with_meta(
                    &scaled_u8,
                    scaled_u16.as_deref(),
                    cols,
                    rows,
                    target_size,
                    bit_depth,
                    pad,
                )?;

            // Compute updated geotransform if metadata available
            let mut gt_override: Option<[f64; 6]> = None;
            let mut proj_override: Option<String> = None;
            if let Some(meta) = metadata {
                if let Some(mut gt) = meta.geotransform {
                    // Adjust pixel size by inverse of scaling
                    if scale_x > 0.0 {
                        gt[1] = gt[1] * (cols as f64 / final_cols as f64);
                    }
                    if scale_y > 0.0 {
                        gt[5] = gt[5] * (rows as f64 / final_rows as f64);
                    }
                    // Shift origin for padding
                    gt[0] = gt[0] - (pad_left as f64) * gt[1];
                    gt[3] = gt[3] - (pad_top as f64) * gt[5];
                    gt_override = Some(gt);
                }
                if let Some(p) = &meta.projection {
                    proj_override = Some(p.clone());
                }
            }

            match bit_depth {
                BitDepth::U8 => {
                    let mut ds = write_tiff_u8(output, final_cols, final_rows, &final_u8)?;
                    if let Some(meta) = metadata {
                        embed_tiff_metadata(
                            &mut ds,
                            meta,
                            operation_label.as_deref(),
                            gt_override,
                            proj_override.as_deref(),
                        )?;
                    }
                    info!("save_processed_image: U8 TIFF saved with metadata");
                }
                BitDepth::U16 => {
                    let mut ds =
                        write_tiff_u16(output, final_cols, final_rows, &final_u16.unwrap())?;
                    if let Some(meta) = metadata {
                        embed_tiff_metadata(
                            &mut ds,
                            meta,
                            operation_label.as_deref(),
                            gt_override,
                            proj_override.as_deref(),
                        )?;
                    }
                    info!("save_processed_image: U16 TIFF saved with metadata");
                }
            }
        }
        OutputFormat::JPEG => {
            let (db_data, _, scaled_u8, _) =
                process_scalar_data_pipeline(processed, BitDepth::U8, strategy);
            let shape = db_data.dim();
            let (rows, cols) = shape;

            let (final_cols, final_rows, final_u8, _, scale_x, scale_y, pad_left, pad_top) =
                resize_image_data_with_meta(
                    &scaled_u8,
                    None,
                    cols,
                    rows,
                    target_size,
                    BitDepth::U8,
                    pad,
                )?;

            write_gray_jpeg(output, final_cols, final_rows, &final_u8)?;

            if let Some(meta) = metadata {
                let mut gt_override: Option<[f64; 6]> = None;
                let mut proj_override: Option<String> = None;
                if let Some(mut gt) = meta.geotransform {
                    if scale_x > 0.0 {
                        gt[1] = gt[1] * (cols as f64 / final_cols as f64);
                    }
                    if scale_y > 0.0 {
                        gt[5] = gt[5] * (rows as f64 / final_rows as f64);
                    }
                    gt[0] = gt[0] - (pad_left as f64) * gt[1];
                    gt[3] = gt[3] - (pad_top as f64) * gt[5];
                    write_world_file(output, gt)?;
                    gt_override = Some(gt);
                }
                if let Some(p) = &meta.projection {
                    write_prj_file(output, p)?;
                    proj_override = Some(p.clone());
                }
                create_jpeg_metadata_sidecar_with_overrides(
                    output,
                    meta,
                    operation_label.as_deref(),
                    gt_override,
                    proj_override.as_deref(),
                )?;
            }

            info!("save_processed_image: JPEG saved with metadata sidecar");
        }
    }
    Ok(())
}

pub fn save_processed_multiband_image_sequential(
    processed1: &Array2<f32>,
    processed2: &Array2<f32>,
    output: &Path,
    format: OutputFormat,
    bit_depth: BitDepth,
    target_size: Option<usize>,
    metadata: Option<&crate::io::sentinel1::SafeMetadata>,
    pad: bool,
    strategy: AutoscaleStrategy,
    operation: ProcessingOperation,
    syn_mode: SyntheticRgbMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let operation_label: Option<String> = match operation {
        ProcessingOperation::SingleBand => None,
        ProcessingOperation::MultibandVvVh => Some("multiband_vv_vh".to_string()),
        ProcessingOperation::MultibandHhHv => Some("multiband_hh_hv".to_string()),
        ProcessingOperation::PolarOp(PolarizationOperation::Sum) => Some("sum".to_string()),
        ProcessingOperation::PolarOp(PolarizationOperation::Diff) => Some("difference".to_string()),
        ProcessingOperation::PolarOp(PolarizationOperation::Ratio) => Some("ratio".to_string()),
        ProcessingOperation::PolarOp(PolarizationOperation::NDiff) => {
            Some("normalized_diff".to_string())
        }
        ProcessingOperation::PolarOp(PolarizationOperation::LogRatio) => {
            Some("log_ratio".to_string())
        }
    };
    match format {
        OutputFormat::TIFF => {
            let shape = processed1.dim();
            let (rows, cols) = shape;

            let (db_data, valid_mask, scaled_u8, scaled_u16) =
                process_scalar_data_pipeline(processed1, bit_depth, strategy);

            let (final_cols, final_rows, final_u8, final_u16, scale_x, scale_y, pad_left, pad_top) =
                resize_image_data_with_meta(
                    &scaled_u8,
                    scaled_u16.as_deref(),
                    cols,
                    rows,
                    target_size,
                    bit_depth,
                    pad,
                )?;

            // Compute updated geotransform if metadata available
            let mut gt_override: Option<[f64; 6]> = None;
            let mut proj_override: Option<String> = None;
            if let Some(meta) = metadata {
                if let Some(mut gt) = meta.geotransform {
                    if scale_x > 0.0 {
                        gt[1] = gt[1] * (cols as f64 / final_cols as f64);
                    }
                    if scale_y > 0.0 {
                        gt[5] = gt[5] * (rows as f64 / final_rows as f64);
                    }
                    gt[0] = gt[0] - (pad_left as f64) * gt[1];
                    gt[3] = gt[3] - (pad_top as f64) * gt[5];
                    gt_override = Some(gt);
                }
                if let Some(p) = &meta.projection {
                    proj_override = Some(p.clone());
                }
            }

            match bit_depth {
                BitDepth::U8 => {
                    drop(db_data);
                    drop(valid_mask);

                    let (_, _, scaled_u8, _) =
                        process_scalar_data_pipeline(processed2, bit_depth, strategy);

                    let (_, _, final_u8_band2, _, _sx2, _sy2, _pl2, _pt2) =
                        resize_image_data_with_meta(
                            &scaled_u8,
                            None,
                            cols,
                            rows,
                            target_size,
                            bit_depth,
                            pad,
                        )?;

                    let mut ds = write_tiff_multiband_u8(
                        output,
                        final_cols,
                        final_rows,
                        &final_u8,
                        &final_u8_band2,
                    )?;
                    if let Some(meta) = metadata {
                        embed_tiff_metadata(
                            &mut ds,
                            meta,
                            operation_label.as_deref(),
                            gt_override,
                            proj_override.as_deref(),
                        )?;
                    }
                    info!(
                        "save_processed_multiband_image_sequential: U8 TIFF saved with 2 bands and metadata"
                    );
                }
                BitDepth::U16 => {
                    let final_band1 = final_u16.as_ref().unwrap().clone();
                    drop(db_data);
                    drop(valid_mask);

                    let (_, _, _, scaled_u16) =
                        process_scalar_data_pipeline(processed2, bit_depth, strategy);

                    let (_, _, _, final_u16, _sx2, _sy2, _pl2, _pt2) = resize_image_data_with_meta(
                        &vec![],
                        scaled_u16.as_deref(),
                        cols,
                        rows,
                        target_size,
                        bit_depth,
                        pad,
                    )?;

                    let mut ds = write_tiff_multiband_u16(
                        output,
                        final_cols,
                        final_rows,
                        &final_band1,
                        final_u16.as_ref().unwrap(),
                    )?;
                    if let Some(meta) = metadata {
                        embed_tiff_metadata(
                            &mut ds,
                            meta,
                            operation_label.as_deref(),
                            gt_override,
                            proj_override.as_deref(),
                        )?;
                    }
                    info!(
                        "save_processed_multiband_image_sequential: U16 TIFF saved with 2 bands and metadata"
                    );
                }
            }
        }
        OutputFormat::JPEG => {
            info!("Creating syntetic RGB JPEG from VV | HH (Red) and VH | HV (Green) bands");

            let (db_data, valid_mask, scaled_u8, _) =
                process_scalar_data_pipeline(processed1, BitDepth::U8, strategy);

            // If Tamed for synRGB, recompute band1 U8 using band-specific tamed autoscale
            let input_u8_band1: Vec<u8> = if matches!(strategy, AutoscaleStrategy::Tamed) {
                autoscale_db_image_tamed_synrgb_u8(&db_data, &valid_mask, true)
            } else {
                scaled_u8
            };
            let shape = db_data.dim();
            let (rows, cols) = shape;

            let (final_cols, final_rows, final_u8_band1, _, scale_x, scale_y, pad_left, pad_top) =
                resize_image_data_with_meta(
                    &input_u8_band1,
                    None,
                    cols,
                    rows,
                    target_size,
                    BitDepth::U8,
                    pad,
                )?;

            let (db2, valid2, scaled_u8_b2, _) =
                process_scalar_data_pipeline(processed2, BitDepth::U8, strategy);

            // If Tamed for synRGB, recompute band2 U8 using band-specific tamed autoscale
            let input_u8_band2: Vec<u8> = if matches!(strategy, AutoscaleStrategy::Tamed) {
                autoscale_db_image_tamed_synrgb_u8(&db2, &valid2, false)
            } else {
                scaled_u8_b2
            };

            let (_, _, final_u8_band2, _, _sx2, _sy2, _pl2, _pt2) = resize_image_data_with_meta(
                &input_u8_band2,
                None,
                cols,
                rows,
                target_size,
                BitDepth::U8,
                pad,
            )?;

            let rgb_data = create_synthetic_rgb_by_mode(syn_mode, &final_u8_band1, &final_u8_band2);

            write_rgb_jpeg(output, final_cols, final_rows, &rgb_data)?;

            if let Some(meta) = metadata {
                let mut gt_override: Option<[f64; 6]> = None;
                let mut proj_override: Option<String> = None;
                if let Some(mut gt) = meta.geotransform {
                    if scale_x > 0.0 {
                        gt[1] = gt[1] * (cols as f64 / final_cols as f64);
                    }
                    if scale_y > 0.0 {
                        gt[5] = gt[5] * (rows as f64 / final_rows as f64);
                    }
                    gt[0] = gt[0] - (pad_left as f64) * gt[1];
                    gt[3] = gt[3] - (pad_top as f64) * gt[5];
                    write_world_file(output, gt)?;
                    gt_override = Some(gt);
                }
                if let Some(p) = &meta.projection {
                    write_prj_file(output, p)?;
                    proj_override = Some(p.clone());
                }
                // Attach synthetic_rgb_mode to JPEG sidecar
                create_jpeg_metadata_sidecar_with_overrides_and_extras(
                    output,
                    meta,
                    operation_label.as_deref(),
                    gt_override,
                    proj_override.as_deref(),
                    Some(&[("synthetic_rgb_mode", syn_mode.to_string())]),
                )?;
            }

            info!("Syntetic RGB JPEG saved with metadata sidecar");
        }
    }
    Ok(())
}
