use ndarray::Array2;
use tracing::{debug, info};

use crate::{AutoscaleStrategy, BitDepth};

/// Simple statistics and percentile estimates computed from a streaming histogram.
struct HistogramStats {
    valid_count: usize,
    min_db: f64,
    max_db: f64,
    mean_db: f64,
    std_db: f64,
    median_db: f64,
    p01: f64,
    p02: f64,
    p05: f64,
    p10: f64,
    p25: f64,
    p75: f64,
    p90: f64,
    p95: f64,
    p98: f64,
    p99: f64,
}

#[inline]
fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9_f64
}

/// Compute robust percentiles and basic stats without materializing/sorting all values.
/// Two-pass approach:
///   1) Find min/max and Welford mean/std in a single pass over valid pixels
///   2) Build a fixed-bin histogram over [min,max] and invert CDF for requested percentiles
fn compute_histogram_stats(db: &Array2<f64>, valid_mask: &[bool]) -> HistogramStats {
    // First pass: min/max + Welford mean/std
    let mut count: u64 = 0;
    let mut min_db = f64::INFINITY;
    let mut max_db = f64::NEG_INFINITY;
    let mut mean = 0.0_f64;
    let mut m2 = 0.0_f64; // Sum of squares of differences from the current mean

    for ((i, j), &v) in db.indexed_iter() {
        if valid_mask[i * db.ncols() + j] {
            count += 1;
            if v < min_db { min_db = v; }
            if v > max_db { max_db = v; }

            // Welford's online algorithm
            let delta = v - mean;
            mean += delta / (count as f64);
            let delta2 = v - mean;
            m2 += delta * delta2;
        }
    }

    if count == 0 {
        return HistogramStats {
            valid_count: 0,
            min_db: 0.0,
            max_db: 0.0,
            mean_db: 0.0,
            std_db: 0.0,
            median_db: 0.0,
            p01: 0.0,
            p02: 0.0,
            p05: 0.0,
            p10: 0.0,
            p25: 0.0,
            p75: 0.0,
            p90: 0.0,
            p95: 0.0,
            p98: 0.0,
            p99: 0.0,
        };
    }

    let std_db = if count > 1 { (m2 / (count as f64)).sqrt() } else { 0.0 };

    // Handle degenerate case: all values are equal
    if (max_db - min_db).abs() < f64::EPSILON {
        return HistogramStats {
            valid_count: count as usize,
            min_db,
            max_db,
            mean_db: mean,
            std_db,
            median_db: min_db,
            p01: min_db,
            p02: min_db,
            p05: min_db,
            p10: min_db,
            p25: min_db,
            p75: max_db,
            p90: max_db,
            p95: max_db,
            p98: max_db,
            p99: max_db,
        };
    }

    // Second pass: histogram over [min,max]
    const NUM_BINS: usize = 4096;
    let mut hist: [u64; NUM_BINS] = [0; NUM_BINS];
    let span = max_db - min_db;
    let inv_span = 1.0 / span;

    for ((i, j), &v) in db.indexed_iter() {
        if !valid_mask[i * db.ncols() + j] {
            continue;
        }
        // Map v ∈ [min,max] into bin 0..NUM_BINS-1 (inclusive)
        let t = ((v - min_db) * inv_span).clamp(0.0, 1.0);
        let mut idx = (t * (NUM_BINS as f64)) as usize;
        if idx >= NUM_BINS { idx = NUM_BINS - 1; }
        hist[idx] += 1;
    }

    // Helper to invert CDF and estimate percentile value using linear interpolation within the bin
    let estimate_percentile = |p: f64| -> f64 {
        let n = count; // number of valid samples
        // Match previous behavior roughly: idx = floor(n * p), then clamp to [0, n-1]
        let mut target = (p * (n as f64)).floor() as u64;
        if target >= n { target = n - 1; }

        let mut cumsum: u64 = 0;
        for (b, &h) in hist.iter().enumerate() {
            let next = cumsum + h;
            if target < next {
                let within = target.saturating_sub(cumsum);
                let frac = if h > 0 { (within as f64) / (h as f64) } else { 0.0 };
                let bin_width = span / (NUM_BINS as f64);
                let bin_start = min_db + (b as f64) * bin_width;
                return bin_start + frac * bin_width;
            }
            cumsum = next;
        }
        // Fallback (should not happen): return max
        max_db
    };

    HistogramStats {
        valid_count: count as usize,
        min_db,
        max_db,
        mean_db: mean,
        std_db,
        median_db: estimate_percentile(0.5),
        p01: estimate_percentile(0.01),
        p02: estimate_percentile(0.02),
        p05: estimate_percentile(0.05),
        p10: estimate_percentile(0.10),
        p25: estimate_percentile(0.25),
        p75: estimate_percentile(0.75),
        p90: estimate_percentile(0.90),
        p95: estimate_percentile(0.95),
        p98: estimate_percentile(0.98),
        p99: estimate_percentile(0.99),
    }
}

#[inline]
fn insertion_sort_in_place(values: &mut [f64]) {
    for i in 1..values.len() {
        let key = values[i];
        let mut j = i;
        while j > 0 && values[j - 1] > key {
            values[j] = values[j - 1];
            j -= 1;
        }
        values[j] = key;
    }
}

#[inline]
fn local_median_and_range_3x3(
    db: &Array2<f64>,
    valid_mask: &[bool],
    row: usize,
    col: usize,
) -> Option<(f64, f64)> {
    let rows = db.nrows();
    let cols = db.ncols();

    let mut buf: [f64; 9] = [0.0; 9];
    let mut count: usize = 0;

    let r0 = row.saturating_sub(1);
    let c0 = col.saturating_sub(1);
    let r1 = (row + 1).min(rows - 1);
    let c1 = (col + 1).min(cols - 1);

    for r in r0..=r1 {
        for c in c0..=c1 {
            let idx = r * cols + c;
            if valid_mask[idx] {
                buf[count] = db[(r, c)];
                count += 1;
            }
        }
    }

    if count == 0 {
        return None;
    }

    let slice = &mut buf[..count];
    insertion_sort_in_place(slice);

    let median = slice[count / 2];
    let range = slice[count - 1] - slice[0];
    Some((median, range))
}

/// Contrast Limited Adaptive Histogram Equalization (CLAHE) on a normalized 0..1 image.
/// - Splits the image into `tiles_x` x `tiles_y` tiles
/// - Builds per-tile histograms with `num_bins` bins
/// - Clips each histogram at `clip_limit` (relative multiplier of average count)
/// - Computes CDFs and performs bilinear interpolation of the CDF value for each pixel
fn clahe_equalize_normalized(
    norm: &Array2<f64>,
    valid_mask: &[bool],
    tiles_x: usize,
    tiles_y: usize,
    clip_limit: f64,
    num_bins: usize,
) -> Array2<f64> {

    let rows = norm.nrows();
    let cols = norm.ncols();
    if rows == 0 || cols == 0 || tiles_x == 0 || tiles_y == 0 || num_bins < 2 {
        return norm.clone();
    }

    let tile_h = (rows + tiles_y - 1) / tiles_y;
    let tile_w = (cols + tiles_x - 1) / tiles_x;

    // Precompute per-tile CDFs
    let mut cdfs: Vec<Vec<f64>> = Vec::with_capacity(tiles_x * tiles_y);
    cdfs.resize_with(tiles_x * tiles_y, || vec![0.0; num_bins]);

    let avg_count_per_bin = |tile_rows: usize, tile_cols: usize| -> f64 {
        let tile_pixels = (tile_rows * tile_cols) as f64;
        tile_pixels / (num_bins as f64)
    };

    for ty in 0..tiles_y {
        let r0 = ty * tile_h;
        let r1 = ((ty + 1) * tile_h).min(rows);
        let tile_rows = r1 - r0;
        for tx in 0..tiles_x {
            let c0 = tx * tile_w;
            let c1 = ((tx + 1) * tile_w).min(cols);
            let tile_cols = c1 - c0;

            let mut hist = vec![0u32; num_bins];

            // Build histogram for this tile
            for r in r0..r1 {
                for c in c0..c1 {
                    if valid_mask[r * cols + c] {
                        let v = norm[(r, c)].clamp(0.0, 1.0);
                        let mut bin = (v * (num_bins as f64 - 1.0)).round() as isize;
                        if bin < 0 { bin = 0; }
                        if bin as usize >= num_bins { bin = (num_bins - 1) as isize; }
                        hist[bin as usize] += 1;
                    }
                }
            }

            // Clip histogram
            let avg = avg_count_per_bin(tile_rows, tile_cols);
            let clip_threshold = (clip_limit * avg).max(1.0);
            let mut excess: f64 = 0.0;
            for h in &mut hist {
                if (*h as f64) > clip_threshold {
                    excess += (*h as f64) - clip_threshold;
                    *h = clip_threshold as u32;
                }
            }
            // Redistribute excess uniformly
            let add_per_bin = (excess / num_bins as f64).floor();
            let mut remainder = (excess - add_per_bin * num_bins as f64).round() as usize;
            for h in &mut hist {
                *h = (*h as f64 + add_per_bin) as u32;
            }
            let mut b = 0;
            while remainder > 0 {
                hist[b] += 1;
                b = (b + 1) % num_bins;
                remainder -= 1;
            }

            // Compute CDF normalized to 0..1
            let total: f64 = hist.iter().map(|&x| x as f64).sum::<f64>().max(1.0);
            let mut cdf = vec![0.0f64; num_bins];
            let mut acc = 0.0f64;
            for i in 0..num_bins {
                acc += hist[i] as f64;
                cdf[i] = (acc / total).clamp(0.0, 1.0);
            }
            cdfs[ty * tiles_x + tx] = cdf;
        }
    }

    // Helper to sample bilinearly from neighboring tile CDFs
    let sample_cdf = |r: usize, c: usize, val: f64| -> f64 {
        let rf = r as f64 / tile_h as f64 - 0.5;
        let cf = c as f64 / tile_w as f64 - 0.5;
        let ty = rf.floor().max(0.0) as isize;
        let tx = cf.floor().max(0.0) as isize;
        let dy = rf - ty as f64;
        let dx = cf - tx as f64;

        let ty0 = ty.clamp(0, tiles_y as isize - 1) as usize;
        let tx0 = tx.clamp(0, tiles_x as isize - 1) as usize;
        let ty1 = ((ty + 1).clamp(0, tiles_y as isize - 1)) as usize;
        let tx1 = ((tx + 1).clamp(0, tiles_x as isize - 1)) as usize;

        let bin_pos = (val.clamp(0.0, 1.0) * (num_bins as f64 - 1.0)).round() as usize;

        let cdf00 = cdfs[ty0 * tiles_x + tx0][bin_pos];
        let cdf01 = cdfs[ty0 * tiles_x + tx1][bin_pos];
        let cdf10 = cdfs[ty1 * tiles_x + tx0][bin_pos];
        let cdf11 = cdfs[ty1 * tiles_x + tx1][bin_pos];

        let top = cdf00 * (1.0 - dx) + cdf01 * dx;
        let bottom = cdf10 * (1.0 - dx) + cdf11 * dx;
        top * (1.0 - dy) + bottom * dy
    };

    let mut out = Array2::<f64>::zeros((rows, cols));
    for r in 0..rows {
        for c in 0..cols {
            if valid_mask[r * cols + c] {
                let v = norm[(r, c)];
                out[(r, c)] = sample_cdf(r, c, v);
            } else {
                out[(r, c)] = 0.0;
            }
        }
    }

    out
}

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
    bit_depth: BitDepth,
) -> Vec<u16> {
    // Fast O(N) stats and percentiles
    let stats = compute_histogram_stats(db, valid_mask);

    if stats.valid_count == 0 {
        return vec![0u16; db.len()];
    }

    let min_db = stats.min_db;
    let max_db = stats.max_db;
    let mean_db = stats.mean_db;
    let median_db = stats.median_db;
    let std_db = stats.std_db;
    let p02 = stats.p02;
    let p25 = stats.p25;
    let p75 = stats.p75;
    let p98 = stats.p98;

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
    bit_depth: BitDepth,
    strategy: AutoscaleStrategy, // robust, adaptive, equalized, tamed, CLAHE, default
) -> Vec<u16> {
    let max_val = match bit_depth {
        BitDepth::U8 => 255.0,
        BitDepth::U16 => 65535.0,
    };

    // Fast O(N) stats and percentiles
    let stats = compute_histogram_stats(db, valid_mask);

    if stats.valid_count == 0 {
        return vec![0u16; db.len()];
    }

    let min_db = stats.min_db;
    let max_db = stats.max_db;
    let mean_db = stats.mean_db;
    let median_db = stats.median_db;
    let std_db = stats.std_db;
    let p01 = stats.p01;
    let p05 = stats.p05;
    let p25 = stats.p25;
    let p75 = stats.p75;
    let p95 = stats.p95;
    let p99 = stats.p99;

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

            // Use histogram-estimated percentiles
            let low = if approx_eq(low_pct, 0.10) { stats.p10 }
                      else if approx_eq(low_pct, 0.02) { stats.p02 }
                      else if approx_eq(low_pct, 0.05) { stats.p05 }
                      else if approx_eq(low_pct, 0.25) { stats.p25 }
                      else if approx_eq(low_pct, 0.75) { stats.p75 }
                      else if approx_eq(low_pct, 0.95) { stats.p95 }
                      else if approx_eq(low_pct, 0.99) { stats.p99 }
                      else { stats.p05 };
            let high = if approx_eq(high_pct, 0.90) { stats.p90 }
                       else if approx_eq(high_pct, 0.98) { stats.p98 }
                       else if approx_eq(high_pct, 0.95) { stats.p95 }
                       else if approx_eq(high_pct, 0.75) { stats.p75 }
                       else if approx_eq(high_pct, 0.99) { stats.p99 }
                       else { stats.p95 };
            // Disable local enhancement (non-physical in dB). For local contrast, use CLAHE.
            (low, high, gamma_adj, false)
        }
        AutoscaleStrategy::Equalized => {
            debug!("Equalized SAR scaling");
            // Histogram equalization approach
            (p01, p99, 1.0, false)
        }
        AutoscaleStrategy::Clahe => {
            debug!("CLAHE SAR scaling");
            // Use robust window for initial normalization prior to CLAHE
            (p01, p99, 1.0, false)
        }
        AutoscaleStrategy::Tamed => {
            debug!("Tamed SAR scaling");
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

    // Special path: CLAHE
    if matches!(strategy, AutoscaleStrategy::Clahe) {
        debug!(
            "Applying CLAHE with window [{:.1}, {:.1}] dB over normalized domain",
            low_clip, high_clip
        );

        let rows = db.nrows();
        let cols = db.ncols();

        // Normalize to 0..1 using the chosen window
        let mut norm = Array2::<f64>::zeros((rows, cols));
        for ((i, j), &v) in db.indexed_iter() {
            if valid_mask[i * cols + j] {
                let clipped = v.max(low_clip).min(high_clip);
                let n = (clipped - low_clip) / range;
                norm[(i, j)] = n;
            } else {
                norm[(i, j)] = 0.0;
            }
        }

        let equalized = clahe_equalize_normalized(&norm, valid_mask, 8, 8, 2.0, 256);

        let mut result = Vec::with_capacity(rows * cols);
        let max_val = match bit_depth {
            BitDepth::U8 => 255.0,
            BitDepth::U16 => 65535.0,
        };
        for ((i, j), &n) in equalized.indexed_iter() {
            if valid_mask[i * cols + j] {
                result.push((n.clamp(0.0, 1.0) * max_val) as u16);
            } else {
                result.push(0u16);
            }
        }
        return result;
    }

    // Apply scaling for other strategies
    let mut result = Vec::with_capacity(db.len());

    if use_local_enhancement {
        // Apply local contrast enhancement for better detail visibility
        debug!("Applying local contrast enhancement");
        let cols = db.ncols();
        for ((i, j), &v) in db.indexed_iter() {
            if !valid_mask[i * cols + j] {
                result.push(0u16);
                continue;
            }

            let (local_median, local_range) = match local_median_and_range_3x3(db, valid_mask, i, j) {
                Some((m, r)) => (m, r),
                None => {
                    let clipped = v.max(low_clip).min(high_clip);
                    let normalized = ((clipped - low_clip) / range).powf(gamma);
                    result.push((normalized * max_val).clamp(0.0, max_val) as u16);
                    continue;
                }
            };

            let local_factor = if local_range > 0.0 {
                1.0 + 0.1 * (v - local_median) / local_range
            } else {
                1.0
            };

            let adjusted_v = v * local_factor;
            let clipped = adjusted_v.max(low_clip).min(high_clip);
            let normalized = ((clipped - low_clip) / range).powf(gamma);
            result.push((normalized * max_val).clamp(0.0, max_val) as u16);
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
    bit_depth: BitDepth,
) -> (Vec<u8>, Option<Vec<u16>>) {
    match bit_depth {
        BitDepth::U8 => {
            let v: Vec<u16> = autoscale_db_image(db, valid_mask, BitDepth::U8);
            let u8_data = scale_u16_to_u8(&v);
            debug!("autoscale_db_image_to_bitdepth: U8");
            (u8_data, None)
        }
        BitDepth::U16 => {
            let v: Vec<u16> = autoscale_db_image(db, valid_mask, BitDepth::U16);
            debug!("autoscale_db_image_to_bitdepth: U16");
            (vec![], Some(v))
        }
    }
}

/// Convenience wrapper to return Vec<u8> or Vec<u16> as needed
pub fn autoscale_db_image_to_bitdepth_advanced(
    db: &Array2<f64>,
    valid_mask: &[bool],
    bit_depth: BitDepth,
    strategy: AutoscaleStrategy,
) -> (Vec<u8>, Option<Vec<u16>>) {
    match bit_depth {
        BitDepth::U8 => {
            let v: Vec<u16> =
                autoscale_db_image_advanced(db, valid_mask, BitDepth::U8, strategy);
            let u8_data = scale_u16_to_u8(&v);
            debug!("autoscale_db_image_to_bitdepth: U8");
            (u8_data, None)
        }
        BitDepth::U16 => {
            let v: Vec<u16> =
                autoscale_db_image_advanced(db, valid_mask, BitDepth::U16, strategy);
            debug!("autoscale_db_image_to_bitdepth: U16");
            (vec![], Some(v))
        }
    }
}

/// Band-specific Tamed autoscale for synRGB quicklooks.
/// Co-pol (VV/HH): use a lower cut near p02..p05 to preserve dark water/shadows.
/// Cross-pol (VH/HV): use a slightly higher lower cut to lift signal.
/// Always map to U8 as synRGB expects U8 inputs and applies channel gammas itself.
pub fn autoscale_db_image_tamed_synrgb_u8(
    db: &Array2<f64>,
    valid_mask: &[bool],
    is_copol: bool,
) -> Vec<u8> {
    let stats = compute_histogram_stats(db, valid_mask);
    if stats.valid_count == 0 {
        return vec![0u8; db.len()];
    }

    // Choose band-specific low clip; high clip at p99 to avoid saturating bright targets
    let (low_clip, high_clip) = if is_copol {
        // Co-pol (VV/HH): darker background; cut lower (more conservative)
        (stats.p02.min(stats.p05), stats.p99)
    } else {
        // Cross-pol (VH/HV): generally weaker; raise floor slightly
        (stats.p05, stats.p99)
    };

    let range = (high_clip - low_clip).max(1.0);

    db.indexed_iter()
        .map(|(idx, &v)| {
            if valid_mask[idx.0 * db.ncols() + idx.1] {
                let clipped = v.max(low_clip).min(high_clip);
                let normalized = (clipped - low_clip) / range;
                (normalized * 255.0).clamp(0.0, 255.0) as u8
            } else {
                0u8
            }
        })
        .collect()
}
