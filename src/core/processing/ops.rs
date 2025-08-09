use ndarray::{Array2, Zip};
use num_complex::Complex;

/// Element-wise sum: a + b
pub fn sum_arrays(a: &Array2<Complex<f64>>, b: &Array2<Complex<f64>>) -> Array2<Complex<f64>> {
    a + b
}

/// Element-wise difference: a - b
pub fn difference_arrays(a: &Array2<Complex<f64>>, b: &Array2<Complex<f64>>) -> Array2<Complex<f64>> {
    a - b
}

/// Element-wise ratio: a / b with zero handling
pub fn ratio_arrays(a: &Array2<Complex<f64>>, b: &Array2<Complex<f64>>) -> Array2<Complex<f64>> {
    let mut result = Array2::zeros(a.dim());
    Zip::from(a).and(b).and(&mut result).for_each(|a_val, b_val, res| {
        if b_val.norm() > 1e-10 {
            *res = *a_val / *b_val;
        } else {
            *res = Complex::new(0.0, 0.0);
        }
    });
    result
}

/// Normalized difference: (a - b) / (a + b) with zero handling
pub fn normalized_diff_arrays(a: &Array2<Complex<f64>>, b: &Array2<Complex<f64>>) -> Array2<Complex<f64>> {
    let mut result = Array2::zeros(a.dim());
    Zip::from(a).and(b).and(&mut result).for_each(|a_val, b_val, res| {
        let sum = *a_val + *b_val;
        if sum.norm() > 1e-10 {
            *res = (*a_val - *b_val) / sum;
        } else {
            *res = Complex::new(0.0, 0.0);
        }
    });
    result
}

/// Log ratio in dB of magnitudes: 10 * log10(|a/b|)
/// Returned as Complex where real holds dB and imaginary is 0
pub fn log_ratio_arrays(a: &Array2<Complex<f64>>, b: &Array2<Complex<f64>>) -> Array2<Complex<f64>> {
    let mut result = Array2::zeros(a.dim());
    Zip::from(a).and(b).and(&mut result).for_each(|a_val, b_val, res| {
        if b_val.norm() > 1e-10 {
            let ratio = *a_val / *b_val;
            let log_ratio = 10.0 * ratio.norm().log10();
            *res = Complex::new(log_ratio, 0.0);
        } else {
            *res = Complex::new(0.0, 0.0);
        }
    });
    result
}


