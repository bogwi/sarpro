use ndarray::Array2;
use num_complex::Complex;
 

use crate::core::processing::autoscale::{
    autoscale_db_image_to_bitdepth, autoscale_db_image_to_bitdepth_advanced,
};
use crate::types::{AutoscaleStrategy, BitDepth};

pub fn process_complex_data_inplace(
    processed: &Array2<Complex<f64>>,
) -> (Array2<f64>, Vec<bool>) {
    let shape = processed.dim();
    let mut db_data = Array2::<f64>::zeros(shape);
    let mut valid_mask = Vec::with_capacity(shape.0 * shape.1);

    for ((i, j), &complex_val) in processed.indexed_iter() {
        let magnitude = complex_val.re.max(1e-10);
        let db_val = 10.0 * magnitude.log10();
        db_data[[i, j]] = db_val;
        valid_mask.push(db_val > -50.0);
    }

    (db_data, valid_mask)
}

pub fn process_complex_data_pipeline(
    processed: &Array2<Complex<f64>>,
    bit_depth: BitDepth,
    strategy: AutoscaleStrategy,
) -> (Array2<f64>, Vec<bool>, Vec<u8>, Option<Vec<u16>>) {
    let (db_data, valid_mask) = process_complex_data_inplace(processed);

    let valid_db: Vec<f64> = db_data
        .iter()
        .zip(&valid_mask)
        .filter_map(|(&v, &m)| if m { Some(v) } else { None })
        .collect();

    let (scaled_u8, scaled_u16) = match strategy {
        AutoscaleStrategy::Standard => {
            autoscale_db_image_to_bitdepth(&db_data, &valid_mask, &valid_db, bit_depth)
        }
        AutoscaleStrategy::Robust
        | AutoscaleStrategy::Adaptive
        | AutoscaleStrategy::Equalized
        | AutoscaleStrategy::Tamed
        | AutoscaleStrategy::Default => autoscale_db_image_to_bitdepth_advanced(
            &db_data,
            &valid_mask,
            &valid_db,
            bit_depth,
            strategy,
        ),
    };

    (db_data, valid_mask, scaled_u8, scaled_u16)
}


