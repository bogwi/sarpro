use ndarray::{Array2, Zip};

/// Element-wise sum: a + b (scalar intensity)
pub fn sum_arrays(a: &Array2<f32>, b: &Array2<f32>) -> Array2<f32> { a + b }

/// Element-wise difference: a - b
pub fn difference_arrays(a: &Array2<f32>, b: &Array2<f32>) -> Array2<f32> { a - b }

/// Element-wise ratio: a / b with zero handling
pub fn ratio_arrays(a: &Array2<f32>, b: &Array2<f32>) -> Array2<f32> {
    let mut result = Array2::zeros(a.dim());
    Zip::from(a)
        .and(b)
        .and(&mut result)
        .for_each(|&a_val, &b_val, res| {
            if b_val.abs() > 1e-10 { *res = a_val / b_val; } else { *res = 0.0; }
        });
    result
}

/// Normalized difference: (a - b) / (a + b) with zero handling
pub fn normalized_diff_arrays(a: &Array2<f32>, b: &Array2<f32>) -> Array2<f32> {
    let mut result = Array2::zeros(a.dim());
    Zip::from(a)
        .and(b)
        .and(&mut result)
        .for_each(|&a_val, &b_val, res| {
            let denom = a_val + b_val;
            if denom.abs() > 1e-10 { *res = (a_val - b_val) / denom; } else { *res = 0.0; }
        });
    result
}

/// Log ratio proxy: output linear magnitude ratio a/b; dB conversion occurs downstream
pub fn log_ratio_arrays(a: &Array2<f32>, b: &Array2<f32>) -> Array2<f32> {
    let mut result = Array2::zeros(a.dim());
    Zip::from(a)
        .and(b)
        .and(&mut result)
        .for_each(|&a_val, &b_val, res| {
            if b_val.abs() > 1e-10 { *res = a_val / b_val; } else { *res = 0.0; }
        });
    result
}
