use std::fs;
use std::path::Path;
use quick_xml::Reader;
use quick_xml::events::Event;

use super::errors::SafeError;
use super::types::SafeMetadata;

pub fn parse_annotation_files(
    annotation_path: &Path,
    mut meta: SafeMetadata,
) -> Result<SafeMetadata, SafeError> {
    // Find and parse annotation XML files
    for entry in fs::read_dir(annotation_path)? {
        let path = entry?.path();
        if path.extension().map(|e| e == "xml").unwrap_or(false) {
            meta = parse_annotation_xml(&path, meta)?;
        }
    }
    Ok(meta)
}

pub fn parse_annotation_xml(
    path: &Path,
    mut meta: SafeMetadata,
) -> Result<SafeMetadata, SafeError> {
    let mut reader = Reader::from_file(path)?;
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut curr = String::new();
    let mut in_product_info = false;
    let mut in_downlink_info = false;
    let mut in_orbit_state = false;
    let mut in_image_annotation = false;
    let mut _in_geolocation_grid = false;
    let mut in_ads_header = false;
    let mut _in_quality_info = false;
    let mut _in_general_annotation = false;
    let mut in_downlink_values = false;
    let mut downlink_fields = 0;
    let mut state_vectors: Vec<(f64, f64, f64)> = Vec::new();
    let mut current_vector: (f64, f64, f64) = (0.0, 0.0, 0.0);

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(ref e) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                curr = tag.clone();
                match tag.as_str() {
                    "adsHeader" => in_ads_header = true,
                    "qualityInformation" => _in_quality_info = true,
                    "generalAnnotation" => _in_general_annotation = true,
                    "productInformation" => in_product_info = true,
                    "downlinkInformation" if downlink_fields == 0 => in_downlink_info = true,
                    "downlinkValues" => in_downlink_values = true,
                    "orbitStateVector" => in_orbit_state = true,
                    "imageAnnotation" => in_image_annotation = true,
                    "geolocationGrid" => _in_geolocation_grid = true,
                    _ => {}
                }
            }
            Event::End(ref e) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag.as_str() {
                    "adsHeader" => in_ads_header = false,
                    "qualityInformation" => _in_quality_info = false,
                    "generalAnnotation" => _in_general_annotation = false,
                    "productInformation" => in_product_info = false,
                    "downlinkInformation" if in_downlink_info => {
                        in_downlink_info = false;
                        downlink_fields += 1;
                    }
                    "downlinkValues" => in_downlink_values = false,
                    "orbitStateVector" => {
                        in_orbit_state = false;
                        state_vectors.push(current_vector);
                        current_vector = (0.0, 0.0, 0.0);
                    }
                    "imageAnnotation" => in_image_annotation = false,
                    "geolocationGrid" => _in_geolocation_grid = false,
                    _ => {}
                }
            }
            Event::Text(e) => {
                let txt = e.unescape().unwrap();
                match curr.as_str() {
                    // ADS Header information
                    "missionId" if in_ads_header => meta.platform = txt.to_string(),
                    "productType" if in_ads_header => meta.product_type = txt.to_string(),
                    "polarisation" if in_ads_header => meta.polarizations.push(txt.to_string()),
                    "mode" if in_ads_header => meta.instrument_mode = Some(txt.to_string()),
                    "startTime" if in_ads_header => meta.acquisition_start = txt.to_string(),
                    "stopTime" if in_ads_header => meta.acquisition_stop = txt.to_string(),
                    "absoluteOrbitNumber" if in_ads_header => {
                        meta.orbit_number = txt.parse().unwrap_or(0)
                    }
                    "missionDataTakeId" if in_ads_header => {
                        meta.data_take_id = Some(txt.to_string())
                    }

                    // Product information
                    "pass" if in_product_info => meta.pass_direction = Some(txt.to_string()),
                    "rangeSamplingRate" if in_product_info => {
                        meta.range_sampling_rate = txt.parse().ok()
                    }
                    "radarFrequency" if in_product_info => {
                        meta.radar_frequency = txt.parse().ok()
                    }
                    "azimuthSteeringRate" if in_product_info => {
                        // Additional SAR parameter
                    }

                    // Downlink information
                    "prf" if in_downlink_info && meta.prf.is_none() => {
                        meta.prf = txt.parse().ok()
                    }

                    // Downlink values
                    "txPulseLength" if in_downlink_values && meta.tx_pulse_length.is_none() => {
                        meta.tx_pulse_length = txt.parse().ok()
                    }
                    "txPulseRampRate"
                        if in_downlink_values && meta.tx_pulse_ramp_rate.is_none() =>
                    {
                        meta.tx_pulse_ramp_rate = txt.parse().ok()
                    }

                    // Image annotation for slant range and pixel spacing
                    "slantRangeTime"
                        if in_image_annotation && meta.slant_range_near.is_none() =>
                    {
                        let srt = txt.parse::<f64>().unwrap_or(0.0);
                        meta.slant_range_near = Some(srt * 299_792_458.0 / 2.0);
                    }
                    "rangePixelSpacing" if in_image_annotation => {
                        meta.pixel_spacing_range = txt.parse().ok()
                    }
                    "azimuthPixelSpacing" if in_image_annotation => {
                        meta.pixel_spacing_azimuth = txt.parse().ok()
                    }

                    // Orbit state vectors
                    "vx" if in_orbit_state => current_vector.0 = txt.parse().unwrap_or(0.0),
                    "vy" if in_orbit_state => current_vector.1 = txt.parse().unwrap_or(0.0),
                    "vz" if in_orbit_state => current_vector.2 = txt.parse().unwrap_or(0.0),

                    // Image dimensions
                    "lines" => meta.lines = txt.parse().unwrap_or(0),
                    "samplesPerLine" => meta.samples = txt.parse().unwrap_or(0),
                    "numberOfSamples" => meta.samples = txt.parse().unwrap_or(0),

                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    // Compute velocity from state vectors
    if !state_vectors.is_empty() {
        let (vx, vy, vz) = state_vectors[state_vectors.len() / 2];
        meta.velocity = Some((vx.powi(2) + vy.powi(2) + vz.powi(2)).sqrt());
    }

    Ok(meta)
}
