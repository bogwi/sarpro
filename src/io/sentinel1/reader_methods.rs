use ndarray::Array2;
use tracing::info;

use super::errors::SafeError;
use super::types::SafeMetadata;
use super::reader::SafeReader;

impl SafeReader {
    /// Access parsed metadata
    pub fn metadata(&self) -> &SafeMetadata {
        &self.metadata
    }

    /// Retrieve the image data as a borrowed array (returns VV data if available, otherwise VH)
    pub fn data(&self) -> Result<&Array2<f32>, SafeError> {
        if let Some(ref arr) = self.vv_data {
            Ok(arr)
        } else if let Some(ref arr) = self.vh_data {
            Ok(arr)
        } else {
            Err(SafeError::MissingField("no polarization data available"))
        }
    }

    /// Get VV data (borrowed reference)
    pub fn vv_data(&self) -> Result<&Array2<f32>, SafeError> {
        let arr = self
            .vv_data
            .as_ref()
            .ok_or(SafeError::MissingField("vv_data"))?;
        Ok(arr)
    }

    /// Get VH data (borrowed reference)
    pub fn vh_data(&self) -> Result<&Array2<f32>, SafeError> {
        let arr = self
            .vh_data
            .as_ref()
            .ok_or(SafeError::MissingField("vh_data"))?;
        Ok(arr)
    }

    /// Get HH data (borrowed reference)
    pub fn hh_data(&self) -> Result<&Array2<f32>, SafeError> {
        let arr = self
            .hh_data
            .as_ref()
            .ok_or(SafeError::MissingField("hh_data"))?;
        Ok(arr)
    }

    /// Get HV data (borrowed reference)
    pub fn hv_data(&self) -> Result<&Array2<f32>, SafeError> {
        let arr = self
            .hv_data
            .as_ref()
            .ok_or(SafeError::MissingField("hv_data"))?;
        Ok(arr)
    }

    /// Get sum of VV and VH data (vv + vh)
    pub fn sum_data(&self) -> Result<Array2<f32>, SafeError> {
        info!("Summing VV and VH data");
        let vv = self.vv_data()?;
        let vh = self.vh_data()?;
        Ok(crate::core::processing::ops::sum_arrays(vv, vh))
    }

    /// Get difference of VV and VH data (vv - vh)
    pub fn difference_data(&self) -> Result<Array2<f32>, SafeError> {
        info!("Differencing VV and VH data");
        let vv = self.vv_data()?;
        let vh = self.vh_data()?;
        Ok(crate::core::processing::ops::difference_arrays(vv, vh))
    }

    /// Get ratio of VV and VH data (vv / vh, with zero handling)
    pub fn ratio_data(&self) -> Result<Array2<f32>, SafeError> {
        info!("Calculating ratio of VV and VH data");
        let vv = self.vv_data()?;
        let vh = self.vh_data()?;
        Ok(crate::core::processing::ops::ratio_arrays(vv, vh))
    }

    /// Get normalized difference of VV and VH data ((vv - vh) / (vv + vh))
    pub fn normalized_diff_data(&self) -> Result<Array2<f32>, SafeError> {
        info!("Calculating normalized difference of VV and VH data");
        let vv = self.vv_data()?;
        let vh = self.vh_data()?;
        Ok(crate::core::processing::ops::normalized_diff_arrays(
            vv, vh,
        ))
    }

    /// Get log ratio of VV and VH data (10 * log10(vv / vh))
    pub fn log_ratio_data(&self) -> Result<Array2<f32>, SafeError> {
        info!("Calculating log ratio of VV and VH data");
        let vv = self.vv_data()?;
        let vh = self.vh_data()?;
        Ok(crate::core::processing::ops::log_ratio_arrays(vv, vh))
    }

    // HH/HV pair operations
    /// Get sum of HH and HV data (hh + hv)
    pub fn sum_hh_hv_data(&self) -> Result<Array2<f32>, SafeError> {
        info!("Summing HH and HV data");
        let hh = self.hh_data()?;
        let hv = self.hv_data()?;
        Ok(crate::core::processing::ops::sum_arrays(hh, hv))
    }

    /// Get difference of HH and HV data (hh - hv)
    pub fn difference_hh_hv_data(&self) -> Result<Array2<f32>, SafeError> {
        info!("Differencing HH and HV data");
        let hh = self.hh_data()?;
        let hv = self.hv_data()?;
        Ok(crate::core::processing::ops::difference_arrays(hh, hv))
    }

    /// Get ratio of HH and HV data (hh / hv, with zero handling)
    pub fn ratio_hh_hv_data(&self) -> Result<Array2<f32>, SafeError> {
        info!("Calculating ratio of HH and HV data");
        let hh = self.hh_data()?;
        let hv = self.hv_data()?;
        Ok(crate::core::processing::ops::ratio_arrays(hh, hv))
    }

    /// Get normalized difference of HH and HV data ((hh - hv) / (hh + hv))
    pub fn normalized_diff_hh_hv_data(&self) -> Result<Array2<f32>, SafeError> {
        info!("Calculating normalized difference of HH and HV data");
        let hh = self.hh_data()?;
        let hv = self.hv_data()?;
        Ok(crate::core::processing::ops::normalized_diff_arrays(
            hh, hv,
        ))
    }

    /// Get log ratio of HH and HV data (10 * log10(hh / hv))
    pub fn log_ratio_hh_hv_data(&self) -> Result<Array2<f32>, SafeError> {
        info!("Calculating log ratio of HH and HV data");
        let hh = self.hh_data()?;
        let hv = self.hv_data()?;
        Ok(crate::core::processing::ops::log_ratio_arrays(hh, hv))
    }

    /// Get a string representation of available polarizations for error reporting
    pub fn get_available_polarizations(&self) -> String {
        let mut available = Vec::new();

        if self.vv_data.is_some() {
            available.push("VV");
        }
        if self.vh_data.is_some() {
            available.push("VH");
        }
        if self.hh_data.is_some() {
            available.push("HH");
        }
        if self.hv_data.is_some() {
            available.push("HV");
        }

        if available.is_empty() {
            "none".to_string()
        } else {
            available.join(", ")
        }
    }
}
