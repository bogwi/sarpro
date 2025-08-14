use ndarray::Array2;
use tracing::{debug, info};

use crate::{AutoscaleStrategy, BitDepth};

/// Scale U16 to U8, used for resizing U16 images to U8
pub fn scale_u16_to_u8(data: &[u16]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }
    let min = *data.iter().min().unwrap() as f32;
    let max = *data.iter().max().unwrap() as f32;

    // Avoid division by zero if min == max
    let scale = if max > min { 255.0 / (max - min) } else { 1.0 };

    data.iter()
        .map(|&x| {
            let val = ((x as f32 - min) * scale).round();
            val.clamp(0.0, 255.0) as u8
        })
        .collect()
}

/// Autoscale a dB image to the desired bit depth using SAR-specific techniques
/// Uses robust statistics and adaptive contrast enhancement for better SAR image quality
pub fn autoscale_db_image(
    db: &Array2<f64>,
    valid_mask: &[bool],
    _valid_db: &[f64], // Kept for API compatibility
    bit_depth: BitDepth,
) -> Vec<u16> {
    use std::f64;

    // Collect valid values and compute robust statistics
    let mut values = Vec::new();
    for ((i, j), &v) in db.indexed_iter() {
        if valid_mask[i * db.ncols() + j] {
            values.push(v);
        }
    }

    if values.is_empty() {
        return vec![0u16; db.len()];
    }

    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = values.len();

    let min_db = values[0];
    let max_db = values[n - 1];
    let mean_db = values.iter().sum::<f64>() / n as f64;
    let median_db = values[n / 2];
    let std_db = (values.iter().map(|&v| (v - mean_db).powi(2)).sum::<f64>() / n as f64).sqrt();

    // Use more robust percentiles for SAR data
    let p02 = values[(n as f64 * 0.02) as usize];
    let _p05 = values[(n as f64 * 0.05) as usize];
    let p25 = values[(n as f64 * 0.25) as usize];
    let p75 = values[(n as f64 * 0.75) as usize];
    let _p95 = values[(n as f64 * 0.95) as usize];
    let p98 = values[(n as f64 * 0.98) as usize];

    let max_val = match bit_depth {
        BitDepth::U8 => 255.0,
        BitDepth::U16 => 65535.0,
    };

    let dynamic_range = max_db - min_db;
    let iqr = p75 - p25;

    info!(
        "SAR autoscale: range={:.1}dB, mean={:.1}dB, std={:.1}dB, IQR={:.1}dB",
        dynamic_range, mean_db, std_db, iqr
    );

    // SAR-specific clipping strategy
    let (low_clip, high_clip, gamma) = if dynamic_range < 15.0 {
        // Very low contrast - use median-based range
        debug!("Low contrast SAR - using median-based range");
        let range = 20.0f64.max(dynamic_range * 0.8);
        (median_db - range / 2.0, median_db + range / 2.0, 1.1)
    } else if iqr < 5.0 {
        // Heavy-tailed distribution - use IQR-based robust range
        debug!("Heavy-tailed SAR - using IQR-based range");
        let outlier_factor = 2.5;
        (p25 - outlier_factor * iqr, p75 + outlier_factor * iqr, 1.0)
    } else if dynamic_range > 40.0 {
        // High dynamic range - use adaptive clipping
        debug!("High dynamic range SAR - adaptive clipping");
        let clip_low = p02.max(min_db + 0.02 * dynamic_range);
        let clip_high = p98.min(max_db - 0.02 * dynamic_range);
        (clip_low, clip_high, 0.9) // Slight gamma compression
    } else {
        // Normal SAR - use 2nd/98th percentiles
        debug!("Normal SAR - using 2nd/98th percentiles");
        (p02, p98, 1.0)
    };

    // Ensure valid range
    let low_clip = low_clip.max(min_db);
    let high_clip = high_clip.min(max_db);
    let range = (high_clip - low_clip).max(1.0);

    debug!(
        "SAR autoscale: clipping to [{:.1}, {:.1}] dB, gamma={:.2}",
        low_clip, high_clip, gamma
    );

    // Apply scaling with gamma correction
    db.indexed_iter()
        .map(|(idx, &v)| {
            if valid_mask[idx.0 * db.ncols() + idx.1] {
                let clipped = v.max(low_clip).min(high_clip);
                let normalized = ((clipped - low_clip) / range).powf(gamma);
                (normalized * max_val).clamp(0.0, max_val) as u16
            } else {
                0u16
            }
        })
        .collect()
}

/// Advanced SAR autoscaling with local contrast enhancement and speckle handling
/// This provides multiple strategies for different SAR image characteristics
pub fn autoscale_db_image_advanced(
    db: &Array2<f64>,
    valid_mask: &[bool],
    _valid_db: &[f64], // Kept for API compatibility
    bit_depth: BitDepth,
    strategy: AutoscaleStrategy, // robust, adaptive, equalized, tamed, default
) -> Vec<u16> {
    use std::f64;

    let max_val = match bit_depth {
        BitDepth::U8 => 255.0,
        BitDepth::U16 => 65535.0,
    };

    // Collect valid values and compute comprehensive statistics
    let mut values = Vec::new();
    for ((i, j), &v) in db.indexed_iter() {
        if valid_mask[i * db.ncols() + j] {
            values.push(v);
        }
    }

    if values.is_empty() {
        return vec![0u16; db.len()];
    }

    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = values.len();

    let min_db = values[0];
    let max_db = values[n - 1];
    let mean_db = values.iter().sum::<f64>() / n as f64;
    let median_db = values[n / 2];
    let std_db = (values.iter().map(|&v| (v - mean_db).powi(2)).sum::<f64>() / n as f64).sqrt();

    let p01 = values[(n as f64 * 0.01) as usize];
    let p05 = values[(n as f64 * 0.05) as usize];
    let p25 = values[(n as f64 * 0.25) as usize];
    let p75 = values[(n as f64 * 0.75) as usize];
    let p95 = values[(n as f64 * 0.95) as usize];
    let p99 = values[(n as f64 * 0.99) as usize];

    let dynamic_range = max_db - min_db;
    let iqr = p75 - p25;

    info!(
        "Advanced SAR stats: range={:.1}dB, mean={:.1}dB, std={:.1}dB, IQR={:.1}dB",
        dynamic_range, mean_db, std_db, iqr
    );

    // Choose scaling parameters based on strategy and image characteristics
    let (low_clip, high_clip, gamma, use_local_enhancement) = match strategy {
        AutoscaleStrategy::Robust => {
            debug!("Robust SAR scaling");
            // Robust statistics approach - handles outliers well
            let outlier_threshold = 2.5 * iqr;
            let low = (p25 - outlier_threshold).max(p01).max(min_db);
            let high = (p75 + outlier_threshold).min(p99).min(max_db);
            (low, high, 1.0, false)
        }
        AutoscaleStrategy::Adaptive => {
            debug!("Adaptive SAR scaling");
            // Adaptive based on image characteristics
            let skew_factor = (mean_db - median_db) / std_db.abs().max(1.0);
            let tail_heaviness = (p99 - p95) / (p95 - p75).max(1.0);

            let (low_pct, high_pct, gamma_adj) = if skew_factor.abs() > 0.5 {
                // Skewed distribution - adjust percentiles
                if skew_factor > 0.0 {
                    (0.02, 0.98, 0.9) // Positive skew
                } else {
                    (0.05, 0.95, 1.1) // Negative skew
                }
            } else if tail_heaviness > 2.0 {
                // Heavy tails - use more conservative clipping
                (0.10, 0.90, 0.8)
            } else {
                // Normal distribution
                (0.05, 0.95, 1.0)
            };

            let low_idx = ((n as f64 * low_pct) as usize).min(n - 1);
            let high_idx = ((n as f64 * high_pct) as usize).min(n - 1);
            let low = values[low_idx];
            let high = values[high_idx];
            (low, high, gamma_adj, true)
        }
        AutoscaleStrategy::Equalized => {
            debug!("Equalized SAR scaling");
            // Histogram equalization approach
            (p01, p99, 1.0, false)
        }
        AutoscaleStrategy::Tamed => {
            debug!("Equalized SAR scaling");
            // Use with p25 and p99 for synRGB only
            (p25, p99, 1.0, false)
        }
        AutoscaleStrategy::Standard => {
            debug!("Standard SAR scaling");
            (p05, p95, 1.0, false)
        }
        AutoscaleStrategy::Default => {
            debug!("Default SAR scaling");
            (p05, p95, 1.0, false)
        }
    };

    let range = (high_clip - low_clip).max(1.0);

    debug!(
        "Advanced SAR scaling: strategy={}, range=[{:.1}, {:.1}] dB, gamma={:.2}, local_enh={}",
        strategy, low_clip, high_clip, gamma, use_local_enhancement
    );

    // Apply scaling
    let mut result = Vec::with_capacity(db.len());

    if use_local_enhancement {
        // Apply local contrast enhancement for better detail visibility
        debug!("Applying local contrast enhancement");
        let window_size = 3; // 3x3 window for local enhancement
        let half_window = window_size / 2;

        for ((i, j), &v) in db.indexed_iter() {
            if !valid_mask[i * db.ncols() + j] {
                result.push(0u16);
                continue;
            }

            // Local contrast enhancement
            let mut local_values = Vec::new();
            for di in -(half_window as isize)..=(half_window as isize) {
                for dj in -(half_window as isize)..=(half_window as isize) {
                    let ni = i as isize + di;
                    let nj = j as isize + dj;
                    if ni >= 0 && ni < db.nrows() as isize && nj >= 0 && nj < db.ncols() as isize {
                        let idx = (ni as usize) * db.ncols() + (nj as usize);
                        if valid_mask[idx] {
                            local_values.push(db[(ni as usize, nj as usize)]);
                        }
                    }
                }
            }

            if !local_values.is_empty() {
                local_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let local_median = local_values[local_values.len() / 2];
                let local_range = local_values.last().unwrap() - local_values.first().unwrap();

                // Adjust based on local statistics
                let local_factor = if local_range > 0.0 {
                    1.0 + 0.1 * (v - local_median) / local_range
                } else {
                    1.0
                };

                let adjusted_v = v * local_factor;
                let clipped = adjusted_v.max(low_clip).min(high_clip);
                let normalized = ((clipped - low_clip) / range).powf(gamma);
                result.push((normalized * max_val).clamp(0.0, max_val) as u16);
            } else {
                let clipped = v.max(low_clip).min(high_clip);
                let normalized = ((clipped - low_clip) / range).powf(gamma);
                result.push((normalized * max_val).clamp(0.0, max_val) as u16);
            }
        }
    } else {
        // Standard scaling
        debug!("Applying scaling without local enhancement");
        for ((i, j), &v) in db.indexed_iter() {
            if valid_mask[i * db.ncols() + j] {
                let clipped = v.max(low_clip).min(high_clip);
                let normalized = ((clipped - low_clip) / range).powf(gamma);
                result.push((normalized * max_val).clamp(0.0, max_val) as u16);
            } else {
                result.push(0u16);
            }
        }
    }

    result
}

/// Convenience wrapper to return Vec<u8> or Vec<u16> as needed
pub fn autoscale_db_image_to_bitdepth(
    db: &Array2<f64>,
    valid_mask: &[bool],
    valid_db: &[f64],
    bit_depth: BitDepth,
) -> (Vec<u8>, Option<Vec<u16>>) {
    match bit_depth {
        BitDepth::U8 => {
            let v: Vec<u16> = autoscale_db_image(db, valid_mask, valid_db, BitDepth::U8);
            let u8_data = scale_u16_to_u8(&v);
            debug!("autoscale_db_image_to_bitdepth: U8");
            (u8_data, None)
        }
        BitDepth::U16 => {
            let v: Vec<u16> = autoscale_db_image(db, valid_mask, valid_db, BitDepth::U16);
            debug!("autoscale_db_image_to_bitdepth: U16");
            (vec![], Some(v))
        }
    }
}

/// Convenience wrapper to return Vec<u8> or Vec<u16> as needed
pub fn autoscale_db_image_to_bitdepth_advanced(
    db: &Array2<f64>,
    valid_mask: &[bool],
    valid_db: &[f64],
    bit_depth: BitDepth,
    strategy: AutoscaleStrategy,
) -> (Vec<u8>, Option<Vec<u16>>) {
    match bit_depth {
        BitDepth::U8 => {
            let v: Vec<u16> =
                autoscale_db_image_advanced(db, valid_mask, valid_db, BitDepth::U8, strategy);
            let u8_data = scale_u16_to_u8(&v);
            debug!("autoscale_db_image_to_bitdepth: U8");
            (u8_data, None)
        }
        BitDepth::U16 => {
            let v: Vec<u16> =
                autoscale_db_image_advanced(db, valid_mask, valid_db, BitDepth::U16, strategy);
            debug!("autoscale_db_image_to_bitdepth: U16");
            (vec![], Some(v))
        }
    }
}
