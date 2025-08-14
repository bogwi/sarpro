/// Creates synthetic RGB data from two SAR bands with gamma correction.
/// Returns interleaved RGB bytes.
pub fn create_synthetic_rgb(band1_data: &[u8], band2_data: &[u8]) -> Vec<u8> {
    let mut rgb_data = Vec::with_capacity(band1_data.len() * 3);

    for i in 0..band1_data.len() {
        let band1_val = band1_data[i];
        let band2_val = band2_data[i];

        let gamma_r = 0.7;
        let r = ((band1_val as f32 / 255.0).powf(gamma_r) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8;

        let gamma_g = 0.9;
        let g = ((band2_val as f32 / 255.0).powf(gamma_g) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8;

        let b = if band2_val > 0 {
            let gamma_b = 0.1;
            let ratio = r as f32 / g as f32;
            let blue = (ratio.powf(gamma_b) * 255.0 * 0.24)
                .clamp(0.0, 255.0)
                .round() as u8;
            blue
        } else {
            0
        };

        rgb_data.push(r);
        rgb_data.push(g);
        rgb_data.push(b);
    }

    rgb_data
}
