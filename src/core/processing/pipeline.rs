use ndarray::Array2;

use crate::core::processing::autoscale::{
    autoscale_db_image_to_bitdepth, autoscale_db_image_to_bitdepth_advanced,
};
use crate::types::{AutoscaleStrategy, BitDepth};

pub fn process_scalar_data_inplace(processed: &Array2<f32>) -> (Array2<f64>, Vec<bool>) {
    let (rows, cols) = processed.dim();
    let len = rows * cols;

    // Prefer contiguous slice iteration to minimize bounds checks
    if let Some(src) = processed.as_slice() {
        let mut db_vec: Vec<f64> = Vec::with_capacity(len);
        let mut valid_mask: Vec<bool> = Vec::with_capacity(len);

        // Single pass over source data: compute dB and validity mask
        for &v in src.iter() {
            let magnitude = (v as f64).max(1e-10);
            let db_val = 10.0 * magnitude.log10();
            db_vec.push(db_val);
            valid_mask.push(db_val > -50.0);
        }

        let db_data = Array2::from_shape_vec((rows, cols), db_vec)
            .expect("db_vec length should match (rows*cols)");
        return (db_data, valid_mask);
    }

    // Fallback: indexed iteration
    let mut db_data = Array2::<f64>::zeros((rows, cols));
    let mut valid_mask = Vec::with_capacity(len);
    for ((i, j), &val) in processed.indexed_iter() {
        let magnitude = (val as f64).max(1e-10);
        let db_val = 10.0 * magnitude.log10();
        db_data[[i, j]] = db_val;
        valid_mask.push(db_val > -50.0);
    }
    (db_data, valid_mask)
}

pub fn process_scalar_data_pipeline(
    processed: &Array2<f32>,
    bit_depth: BitDepth,
    strategy: AutoscaleStrategy,
) -> (Array2<f64>, Vec<bool>, Vec<u8>, Option<Vec<u16>>) {
    let (db_data, valid_mask) = process_scalar_data_inplace(processed);

    let (scaled_u8, scaled_u16) = match strategy {
        AutoscaleStrategy::Standard => {
            autoscale_db_image_to_bitdepth(&db_data, &valid_mask, bit_depth)
        }
        AutoscaleStrategy::Robust
        | AutoscaleStrategy::Adaptive
        | AutoscaleStrategy::Equalized
        | AutoscaleStrategy::Clahe
        | AutoscaleStrategy::Tamed
        | AutoscaleStrategy::Default => autoscale_db_image_to_bitdepth_advanced(
            &db_data,
            &valid_mask,
            bit_depth,
            strategy,
        ),
    };

    (db_data, valid_mask, scaled_u8, scaled_u16)
}
