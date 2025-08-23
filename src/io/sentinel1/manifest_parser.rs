use std::path::Path;
use quick_xml::Reader;
use quick_xml::events::Event;
use chrono;

use super::errors::SafeError;
use super::types::SafeMetadata;
use super::annotation_parser::{parse_annotation_files};

/// Parse comprehensive metadata from manifest.safe and annotation files
pub fn parse_comprehensive_metadata(base_path: &Path) -> Result<SafeMetadata, SafeError> {
    // Initialize metadata with conversion provenance
    let mut meta = SafeMetadata {
        instrument: String::new(),
        platform: String::new(),
        acquisition_start: String::new(),
        acquisition_stop: String::new(),
        orbit_number: 0,
        polarizations: Vec::new(),
        lines: 0,
        samples: 0,
        product_type: String::new(),
        range_sampling_rate: None,
        radar_frequency: None,
        prf: None,
        tx_pulse_length: None,
        tx_pulse_ramp_rate: None,
        velocity: None,
        slant_range_near: None,
        geotransform: None,
        projection: None,
        crs: None,
        pixel_spacing_range: None,
        pixel_spacing_azimuth: None,
        instrument_mode: None,
        pass_direction: None,
        data_take_id: None,
        product_id: None,
        processing_level: None,
        multilook_factor: None,
        calibration_type: None,
        noise_estimate: None,
        processing_center: None,
        software_version: None,
        pixel_data_type: None,
        bits_per_sample: None,
        sample_format: None,
        incidence_angle: None,
        look_angle: None,
        doppler_centroid: None,
        radiometric_calibration: None,
        geometric_calibration: None,
        conversion_tool: "SARPRO".to_string(),
        conversion_version: env!("CARGO_PKG_VERSION").to_string(),
        conversion_timestamp: chrono::Utc::now().to_rfc3339(),
    };

    // Parse manifest.safe for product-level metadata
    let manifest_path = base_path.join("manifest.safe");
    if manifest_path.exists() {
        meta = parse_manifest_safe(&manifest_path, meta)?;
    }

    // Parse annotation files for detailed metadata
    let annotation_path = base_path.join("annotation");
    if annotation_path.is_dir() {
        meta = parse_annotation_files(&annotation_path, meta)?;
    }

    Ok(meta)
}

pub fn parse_manifest_safe(path: &Path, mut meta: SafeMetadata) -> Result<SafeMetadata, SafeError> {
    let mut reader = Reader::from_file(path)?;
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut curr = String::new();
    let mut _in_metadata_section = false;
    let mut in_platform_section = false;
    let mut in_acquisition_period = false;
    let mut in_orbit_reference = false;
    let mut _in_general_product_info = false;
    let mut _in_processing = false;
    let mut in_facility = false;
    let mut in_software = false;
    let mut in_standalone_product_info = false;
    let mut in_orbit_properties = false;

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(ref e) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                curr = tag.clone();
                match tag.as_str() {
                    "metadataSection" => _in_metadata_section = true,
                    "platform" => in_platform_section = true,
                    "acquisitionPeriod" => in_acquisition_period = true,
                    "orbitReference" => in_orbit_reference = true,
                    "generalProductInformation" => _in_general_product_info = true,
                    "processing" => _in_processing = true,
                    "facility" => in_facility = true,
                    "software" => in_software = true,
                    "standAloneProductInformation" => in_standalone_product_info = true,
                    "orbitProperties" => in_orbit_properties = true,
                    _ => {}
                }
            }
            Event::End(ref e) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag.as_str() {
                    "metadataSection" => _in_metadata_section = false,
                    "platform" => in_platform_section = false,
                    "acquisitionPeriod" => in_acquisition_period = false,
                    "orbitReference" => in_orbit_reference = false,
                    "generalProductInformation" => _in_general_product_info = false,
                    "processing" => _in_processing = false,
                    "facility" => in_facility = false,
                    "software" => in_software = false,
                    "standAloneProductInformation" => in_standalone_product_info = false,
                    "orbitProperties" => in_orbit_properties = false,
                    _ => {}
                }
            }
            Event::Text(e) => {
                let txt = e.unescape().unwrap();
                match curr.as_str() {
                    // Platform information
                    "familyName" if in_platform_section => meta.platform = txt.to_string(),
                    "instrument" if in_platform_section => meta.instrument = txt.to_string(),
                    "mode" if in_platform_section => {
                        meta.instrument_mode = Some(txt.to_string())
                    }

                    // Acquisition period
                    "startTime" if in_acquisition_period => {
                        meta.acquisition_start = txt.to_string()
                    }
                    "stopTime" if in_acquisition_period => {
                        meta.acquisition_stop = txt.to_string()
                    }

                    // Orbit information
                    "orbitNumber" if in_orbit_reference => {
                        meta.orbit_number = txt.parse().unwrap_or(0)
                    }
                    "pass" if in_orbit_properties => {
                        meta.pass_direction = Some(txt.to_string())
                    }

                    // Product information
                    "productType" if in_standalone_product_info => {
                        meta.product_type = txt.to_string()
                    }
                    "missionDataTakeID" if in_standalone_product_info => {
                        meta.data_take_id = Some(txt.to_string())
                    }
                    "productClass" if in_standalone_product_info => {
                        meta.processing_level = Some(txt.to_string())
                    }
                    "transmitterReceiverPolarisation" if in_standalone_product_info => {
                        meta.polarizations.push(txt.to_string());
                    }

                    // Processing information
                    "name" if in_facility => meta.processing_center = Some(txt.to_string()),
                    "name" if in_software => meta.software_version = Some(txt.to_string()),
                    "version" if in_software => meta.software_version = Some(txt.to_string()),

                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(meta)
}
