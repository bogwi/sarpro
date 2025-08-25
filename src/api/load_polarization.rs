//! Load single polarization data from SAFE input

use std::path::Path;
use ndarray::Array2;
use crate::error::{Error, Result};
use crate::io::sentinel1::{SafeMetadata, SafeReader};
use crate::types::Polarization;

/// Load a single polarization's processed intensity array (`Array2<f32>`) and metadata
pub fn load_polarization(
    input: &Path,
    pol: Polarization,
) -> Result<(Array2<f32>, SafeMetadata)> {
    match pol {
        Polarization::Multiband | Polarization::OP(_) => {
            return Err(Error::Processing(
                "load_polarization expects a single polarization (vv/vh/hh/hv)".to_string(),
            ));
        }
        _ => {}
    }

    let reader = SafeReader::open_with_options(input, pol, None, None, None)?;
    let data_ref: &Array2<f32> = match pol {
        Polarization::Vv => reader.vv_data()?,
        Polarization::Vh => reader.vh_data()?,
        Polarization::Hh => reader.hh_data()?,
        Polarization::Hv => reader.hv_data()?,
        _ => unreachable!(),
    };
    Ok((data_ref.clone(), reader.metadata.clone()))
}
