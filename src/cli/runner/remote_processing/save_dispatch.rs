use std::path::Path;

use sarpro::core::processing::save::{
    save_processed_image, save_processed_multiband_image_sequential,
};
use sarpro::io::SafeReader;
use sarpro::types::{AutoscaleStrategy, BitDepth, OutputFormat, ProcessingOperation, SyntheticRgbMode};
use sarpro::{Polarization, PolarizationOperation};

use crate::cli::errors::AppError;

pub fn resolve_polarization_and_save_image(
    reader: &SafeReader,
    polarization: Polarization,
    output_path: &Path,
    format: OutputFormat,
    bit_depth: BitDepth,
    target_size: Option<usize>,
    synrgb_mode: SyntheticRgbMode,
    pad: bool,
    autoscale: AutoscaleStrategy,
) -> Result<(), Box<dyn std::error::Error>> {
    if polarization == Polarization::Vv || polarization == Polarization::Vh ||
       polarization == Polarization::Hh || polarization == Polarization::Hv {
        let processed = if polarization == Polarization::Vv {
            reader.vv_data()?
        } else if polarization == Polarization::Vh {
            reader.vh_data()?
        } else if polarization == Polarization::Hh {
            reader.hh_data()?
        } else if polarization == Polarization::Hv {
            reader.hv_data()?
        } else {
            unreachable!()
        };

        save_processed_image(
            &processed,
            output_path,
            format,
            bit_depth,
            target_size,
            Some(reader.metadata()),
            pad,
            autoscale,
            ProcessingOperation::SingleBand,
        )?
    } else if polarization == Polarization::Multiband {
        if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
            let vv_processed = reader.vv_data()?;
            let vh_processed = reader.vh_data()?;
            save_processed_multiband_image_sequential(
                &vv_processed,
                &vh_processed,
                output_path,
                format,
                bit_depth,
                target_size,
                Some(reader.metadata()),
                pad,
                autoscale,
                ProcessingOperation::MultibandVvVh,
                synrgb_mode,
            )?
        } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
            let hh_processed = reader.hh_data()?;
            let hv_processed = reader.hv_data()?;
            save_processed_multiband_image_sequential(
                &hh_processed,
                &hv_processed,
                output_path,
                format,
                bit_depth,
                target_size,
                Some(reader.metadata()),
                pad,
                autoscale,
                ProcessingOperation::MultibandHhHv,
                synrgb_mode,
            )?
        } else {
            return Err(Box::new(AppError::IncompleDataPair {
                operation: "multiband".to_string(),
                available: reader.get_available_polarizations(),
            }));
        }
    } else if let Polarization::OP(op) = polarization {
        let processed = if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
            if op == PolarizationOperation::Sum {
                reader.sum_data()?
            } else if op == PolarizationOperation::Diff {
                reader.difference_data()?
            } else if op == PolarizationOperation::Ratio {
                reader.ratio_data()?
            } else if op == PolarizationOperation::NDiff {
                reader.normalized_diff_data()?
            } else if op == PolarizationOperation::LogRatio {
                reader.log_ratio_data()?
            } else {
                unreachable!()
            }
        } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
            if op == PolarizationOperation::Sum {
                reader.sum_hh_hv_data()?
            } else if op == PolarizationOperation::Diff {
                reader.difference_hh_hv_data()?
            } else if op == PolarizationOperation::Ratio {
                reader.ratio_hh_hv_data()?
            } else if op == PolarizationOperation::NDiff {
                reader.normalized_diff_hh_hv_data()?
            } else if op == PolarizationOperation::LogRatio {
                reader.log_ratio_hh_hv_data()?
            } else {
                unreachable!()
            }
        } else {
            return Err(Box::new(AppError::IncompleDataPair {
                operation: polarization.to_string(),
                available: reader.get_available_polarizations(),
            }));
        };

        save_processed_image(
            &processed,
            output_path,
            format,
            bit_depth,
            target_size,
            Some(reader.metadata()),
            pad,
            autoscale,
            ProcessingOperation::PolarOp(op),
        )?
    }
    Ok(())
}


