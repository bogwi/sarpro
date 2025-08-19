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

use crate::types::SyntheticRgbMode;

/// Dispatcher for synthetic RGB composition by mode. For now, all modes map to Default.
pub fn create_synthetic_rgb_by_mode(mode: SyntheticRgbMode, band1_data: &[u8], band2_data: &[u8]) -> Vec<u8> {
    match mode {
        SyntheticRgbMode::Default => create_synthetic_rgb(band1_data, band2_data),
        SyntheticRgbMode::RgbRatio => create_synthetic_rgb(band1_data, band2_data),
        SyntheticRgbMode::SarUrban => create_synthetic_rgb(band1_data, band2_data),
        SyntheticRgbMode::Enhanced => create_synthetic_rgb(band1_data, band2_data),
    }
}

/// Placeholder for future implementation of Copernicus "RGB ratio" mode.
pub fn create_synthetic_rgb_rgb_ratio(_band1_data: &[u8], _band2_data: &[u8]) -> Vec<u8> {
    todo!("create_synthetic_rgb_rgb_ratio: to be implemented")
}

/// Placeholder for future implementation of Copernicus "SAR Urban" mode.
pub fn create_synthetic_rgb_urban(_band1_data: &[u8], _band2_data: &[u8]) -> Vec<u8> {
    todo!("create_synthetic_rgb_urban: to be implemented")
}

/// Placeholder for future implementation of Copernicus "Enhanced visualization" mode.
pub fn create_synthetic_rgb_enhanced(_band1_data: &[u8], _band2_data: &[u8]) -> Vec<u8> {
    todo!("create_synthetic_rgb_enhanced: to be implemented")
}
