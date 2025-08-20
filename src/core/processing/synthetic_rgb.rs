/// Creates synthetic RGB data from two SAR bands with gamma correction.
/// Returns interleaved RGB bytes.
///
/// Optimization: Avoid per‑pixel `powf` by using precomputed lookup tables (LUTs):
/// - 256‑entry LUT for red (gamma 0.7)
/// - 256‑entry LUT for green (gamma 0.9)
/// - 65,536‑entry LUT for blue derived from (band1, band2) via the same
///   red/green mappings and ratio exponent (gamma 0.1), including the
///   original guard `if band2_val == 0 { blue = 0 }`.
pub fn create_synthetic_rgb(band1_data: &[u8], band2_data: &[u8]) -> Vec<u8> {
    debug_assert_eq!(band1_data.len(), band2_data.len());

    // Constants as in the original implementation
    const GAMMA_R: f32 = 0.7;
    const GAMMA_G: f32 = 0.9;
    const GAMMA_B: f32 = 0.1;
    const SCALE_255: f32 = 255.0;
    const BLUE_SCALE: f32 = 0.24; // preserves previous visual tuning

    // Precompute 256‑entry LUTs for red and green channels
    let mut lut_r = [0u8; 256];
    let mut lut_g = [0u8; 256];
    for v in 0u16..=255 {
        let vf = (v as f32) / SCALE_255;
        let r = (vf.powf(GAMMA_R) * SCALE_255).round().clamp(0.0, 255.0) as u8;
        let g = (vf.powf(GAMMA_G) * SCALE_255).round().clamp(0.0, 255.0) as u8;
        lut_r[v as usize] = r;
        lut_g[v as usize] = g;
    }

    // Precompute a 65,536‑entry LUT for blue based on (raw band1, raw band2)
    // The formula mirrors the original: when band2 == 0 => blue = 0; otherwise
    // ratio = r/g (where r,g are gamma‑mapped u8), blue = clamp(round((ratio^gamma_b * 255) * 0.24), 0..255)
    let mut lut_b = [0u8; 256 * 256];
    for b1 in 0u16..=255 {
        for b2 in 0u16..=255 {
            let idx = (b1 as usize) << 8 | (b2 as usize);
            let blue = if b2 == 0 {
                0u8
            } else {
                let r = lut_r[b1 as usize] as f32;
                let g = lut_g[b2 as usize] as f32;
                // Preserve original behavior when g == 0 after gamma/rounding: ratio=>inf, clamp=>255
                let ratio = r / g; // if g==0 => inf
                (ratio.powf(GAMMA_B) * SCALE_255 * BLUE_SCALE)
                    .clamp(0.0, 255.0)
                    .round() as u8
            };
            lut_b[idx] = blue;
        }
    }

    let mut rgb_data = Vec::with_capacity(band1_data.len() * 3);
    for i in 0..band1_data.len() {
        let v1 = band1_data[i] as usize;
        let v2 = band2_data[i] as usize;
        let r = lut_r[v1];
        let g = lut_g[v2];
        let b = lut_b[(v1 << 8) | v2];

        rgb_data.push(r);
        rgb_data.push(g);
        rgb_data.push(b);
    }

    rgb_data
}

use crate::types::{AutoscaleStrategy, SyntheticRgbMode};

/// Dispatcher for synthetic RGB composition by mode. For now, all modes map to Default.
pub fn create_synthetic_rgb_by_mode(mode: SyntheticRgbMode, band1_data: &[u8], band2_data: &[u8]) -> Vec<u8> {
    match mode {
        SyntheticRgbMode::Default => create_synthetic_rgb(band1_data, band2_data),
        SyntheticRgbMode::RgbRatio => create_synthetic_rgb(band1_data, band2_data),
        SyntheticRgbMode::SarUrban => create_synthetic_rgb(band1_data, band2_data),
        SyntheticRgbMode::Enhanced => create_synthetic_rgb(band1_data, band2_data),
    }
}

/// Create synthetic RGB with additional maritime suppression tuned for Tamed/CLAHE autoscale outputs.
///
/// Heuristics:
/// - Compute a global low-intensity floor from the combined histogram (~p05) and treat pixels
///   where both bands are below this floor (with a small cushion) as water → force RGB=(0,0,0).
/// - Apply a soft floor subtraction before gamma to avoid boosting near-zero values (common on water).
/// - Stabilize the blue ratio by adding epsilon to numerator/denominator and lowering blue gain.
pub fn create_synthetic_rgb_suppressed(band1_data: &[u8], band2_data: &[u8]) -> Vec<u8> {
    debug_assert_eq!(band1_data.len(), band2_data.len());

    // 1) Combined histogram to estimate a robust low floor (around p05)
    let mut histogram = [0u32; 256];
    for &v in band1_data.iter() {
        histogram[v as usize] = histogram[v as usize].saturating_add(1);
    }
    for &v in band2_data.iter() {
        histogram[v as usize] = histogram[v as usize].saturating_add(1);
    }
    let total_count: u32 = (band1_data.len() + band2_data.len()) as u32;
    let target_count: u32 = ((total_count as f64) * 0.05).round() as u32; // p05
    let mut cumulative: u32 = 0;
    let mut floor_value: usize = 0;
    for i in 0..=255 {
        cumulative = cumulative.saturating_add(histogram[i]);
        if cumulative >= target_count {
            floor_value = i;
            break;
        }
    }
    // Add a small cushion and clamp to a reasonable maximum to avoid over-masking
    let floor_with_cushion: u8 = floor_value
        .saturating_add(3) // +3 levels above p05
        .min(40) as u8; // cap ~15% on 8-bit scale

    // 2) Precompute LUTs with soft floor subtraction and slightly >1 gamma to compress lows
    const SCALE_255: f32 = 255.0;
    const GAMMA_R_SUPP: f32 = 1.15; // slightly compress low values
    const GAMMA_G_SUPP: f32 = 1.10;
    let floor = floor_with_cushion as f32;
    let denom = (255.0 - floor).max(1.0);

    let mut lut_r = [0u8; 256];
    let mut lut_g = [0u8; 256];
    for v in 0u16..=255 {
        if (v as u8) <= floor_with_cushion {
            lut_r[v as usize] = 0;
            lut_g[v as usize] = 0;
        } else {
            let shifted = (v as f32 - floor) / denom; // in (0,1]
            let r = (shifted.powf(GAMMA_R_SUPP) * SCALE_255).round().clamp(0.0, 255.0) as u8;
            let g = (shifted.powf(GAMMA_G_SUPP) * SCALE_255).round().clamp(0.0, 255.0) as u8;
            lut_r[v as usize] = r;
            lut_g[v as usize] = g;
        }
    }

    // 3) Precompute blue ratio LUT with epsilon stabilization and reduced gain
    const GAMMA_B: f32 = 0.1;
    const BLUE_SCALE_SUPP: f32 = 0.18; // lower than default 0.24 to further tame blue speckle
    const EPS: f32 = 8.0; // ratio stabilizer in U8 space

    let mut lut_b = [0u8; 256 * 256];
    for b1 in 0u16..=255 {
        for b2 in 0u16..=255 {
            let idx = (b1 as usize) << 8 | (b2 as usize);
            let r = lut_r[b1 as usize] as f32;
            let g = lut_g[b2 as usize] as f32;
            let ratio = (r + EPS) / (g + EPS);
            let blue = (ratio.powf(GAMMA_B) * SCALE_255 * BLUE_SCALE_SUPP)
                .clamp(0.0, 255.0)
                .round() as u8;
            lut_b[idx] = blue;
        }
    }

    // 4) Compose with water short-circuit: if both raw bands are under floor+cushion => RGB=(0,0,0)
    let mut rgb_data = Vec::with_capacity(band1_data.len() * 3);
    for i in 0..band1_data.len() {
        let v1_raw = band1_data[i];
        let v2_raw = band2_data[i];
        if v1_raw <= floor_with_cushion && v2_raw <= floor_with_cushion {
            rgb_data.push(0);
            rgb_data.push(0);
            rgb_data.push(0);
            continue;
        }
        let v1 = v1_raw as usize;
        let v2 = v2_raw as usize;
        let r = lut_r[v1];
        let g = lut_g[v2];
        let b = lut_b[(v1 << 8) | v2];
        rgb_data.push(r);
        rgb_data.push(g);
        rgb_data.push(b);
    }

    rgb_data
}

/// Dispatcher that selects a composition tuned to the autoscale strategy.
/// For Tamed and Clahe we use a suppressed mapping; otherwise we use the default mapping.
pub fn create_synthetic_rgb_by_mode_and_strategy(
    mode: SyntheticRgbMode,
    strategy: AutoscaleStrategy,
    band1_data: &[u8],
    band2_data: &[u8],
) -> Vec<u8> {
    match strategy {
        AutoscaleStrategy::Tamed | AutoscaleStrategy::Clahe => match mode {
            SyntheticRgbMode::Default
            | SyntheticRgbMode::RgbRatio
            | SyntheticRgbMode::SarUrban
            | SyntheticRgbMode::Enhanced => create_synthetic_rgb_suppressed(band1_data, band2_data),
        },
        _ => create_synthetic_rgb_by_mode(mode, band1_data, band2_data),
    }
}

/// Placeholder for future implementation of Copernicus "RGB ratio" inspired mode.
pub fn create_synthetic_rgb_rgb_ratio(_band1_data: &[u8], _band2_data: &[u8]) -> Vec<u8> {
    todo!("create_synthetic_rgb_rgb_ratio: to be implemented")
}

/// Placeholder for future implementation of Copernicus "SAR Urban" inspired mode.
pub fn create_synthetic_rgb_urban(_band1_data: &[u8], _band2_data: &[u8]) -> Vec<u8> {
    todo!("create_synthetic_rgb_urban: to be implemented")
}

/// Placeholder for future implementation of Copernicus "Enhanced visualization" inspired mode.
pub fn create_synthetic_rgb_enhanced(_band1_data: &[u8], _band2_data: &[u8]) -> Vec<u8> {
    todo!("create_synthetic_rgb_enhanced: to be implemented")
}
