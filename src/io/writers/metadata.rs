use gdal::Dataset;
use gdal::Metadata;
use serde_json;
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

use crate::io::sentinel1::SafeMetadata;

/// Metadata format for different output types
#[derive(Debug, Clone, Copy)]
pub enum MetadataFormat {
    /// TIFF metadata (embedded in file)
    Tiff,
    /// JSON metadata (sidecar file)
    Json,
}

/// Extract all metadata fields from SafeMetadata into a HashMap
pub fn extract_metadata_fields(
    meta: &SafeMetadata,
    operation: Option<&str>,
) -> HashMap<String, String> {
    let mut metadata = HashMap::new();

    // Basic product information
    metadata.insert("INSTRUMENT".to_string(), meta.instrument.clone());
    metadata.insert("PLATFORM".to_string(), meta.platform.clone());
    metadata.insert(
        "ACQUISITION_START".to_string(),
        meta.acquisition_start.clone(),
    );
    metadata.insert(
        "ACQUISITION_STOP".to_string(),
        meta.acquisition_stop.clone(),
    );
    metadata.insert("ORBIT_NUMBER".to_string(), meta.orbit_number.to_string());

    // Handle polarization field based on operation
    let polarization_field = if let Some(op) = operation {
        match op {
            "sum" => {
                if meta.polarizations.contains(&"VV".to_string())
                    && meta.polarizations.contains(&"VH".to_string())
                {
                    "SUM(VV, VH)".to_string()
                } else if meta.polarizations.contains(&"HH".to_string())
                    && meta.polarizations.contains(&"HV".to_string())
                {
                    "SUM(HH, HV)".to_string()
                } else {
                    meta.polarizations.join(",")
                }
            }
            "difference" => {
                if meta.polarizations.contains(&"VV".to_string())
                    && meta.polarizations.contains(&"VH".to_string())
                {
                    "DIFF(VV, VH)".to_string()
                } else if meta.polarizations.contains(&"HH".to_string())
                    && meta.polarizations.contains(&"HV".to_string())
                {
                    "DIFF(HH, HV)".to_string()
                } else {
                    meta.polarizations.join(",")
                }
            }
            "ratio" => {
                if meta.polarizations.contains(&"VV".to_string())
                    && meta.polarizations.contains(&"VH".to_string())
                {
                    "RATIO(VV, VH)".to_string()
                } else if meta.polarizations.contains(&"HH".to_string())
                    && meta.polarizations.contains(&"HV".to_string())
                {
                    "RATIO(HH, HV)".to_string()
                } else {
                    meta.polarizations.join(",")
                }
            }
            "normalized_diff" => {
                if meta.polarizations.contains(&"VV".to_string())
                    && meta.polarizations.contains(&"VH".to_string())
                {
                    "NORM_DIFF(VV, VH)".to_string()
                } else if meta.polarizations.contains(&"HH".to_string())
                    && meta.polarizations.contains(&"HV".to_string())
                {
                    "NORM_DIFF(HH, HV)".to_string()
                } else {
                    meta.polarizations.join(",")
                }
            }
            "log_ratio" => {
                if meta.polarizations.contains(&"VV".to_string())
                    && meta.polarizations.contains(&"VH".to_string())
                {
                    "LOG_RATIO(VV, VH)".to_string()
                } else if meta.polarizations.contains(&"HH".to_string())
                    && meta.polarizations.contains(&"HV".to_string())
                {
                    "LOG_RATIO(HH, HV)".to_string()
                } else {
                    meta.polarizations.join(",")
                }
            }
            "multiband_vv_vh" => "MULTIBAND(VV, VH)".to_string(),
            "multiband_hh_hv" => "MULTIBAND(HH, HV)".to_string(),
            _ => meta.polarizations.join(","),
        }
    } else {
        meta.polarizations.join(",")
    };
    metadata.insert("POLARIZATIONS".to_string(), polarization_field);

    metadata.insert("PRODUCT_TYPE".to_string(), meta.product_type.clone());

    // SAR parameters
    if let Some(rate) = meta.range_sampling_rate {
        metadata.insert("RANGE_SAMPLING_RATE".to_string(), rate.to_string());
    }
    if let Some(freq) = meta.radar_frequency {
        metadata.insert("RADAR_FREQUENCY".to_string(), freq.to_string());
    }
    if let Some(prf) = meta.prf {
        metadata.insert("PRF".to_string(), prf.to_string());
    }
    if let Some(pulse_len) = meta.tx_pulse_length {
        metadata.insert("TX_PULSE_LENGTH".to_string(), pulse_len.to_string());
    }
    if let Some(ramp_rate) = meta.tx_pulse_ramp_rate {
        metadata.insert("TX_PULSE_RAMP_RATE".to_string(), ramp_rate.to_string());
    }
    if let Some(vel) = meta.velocity {
        metadata.insert("VELOCITY".to_string(), vel.to_string());
    }
    if let Some(slant_range) = meta.slant_range_near {
        metadata.insert("SLANT_RANGE_NEAR".to_string(), slant_range.to_string());
    }

    // Georeferencing information
    if let Some(pixel_spacing_range) = meta.pixel_spacing_range {
        metadata.insert(
            "PIXEL_SPACING_RANGE".to_string(),
            pixel_spacing_range.to_string(),
        );
    }
    if let Some(pixel_spacing_azimuth) = meta.pixel_spacing_azimuth {
        metadata.insert(
            "PIXEL_SPACING_AZIMUTH".to_string(),
            pixel_spacing_azimuth.to_string(),
        );
    }

    // Acquisition details
    if let Some(mode) = &meta.instrument_mode {
        metadata.insert("INSTRUMENT_MODE".to_string(), mode.clone());
    }
    if let Some(pass_dir) = &meta.pass_direction {
        metadata.insert("PASS_DIRECTION".to_string(), pass_dir.clone());
    }
    if let Some(data_take_id) = &meta.data_take_id {
        metadata.insert("DATA_TAKE_ID".to_string(), data_take_id.clone());
    }
    if let Some(product_id) = &meta.product_id {
        metadata.insert("PRODUCT_ID".to_string(), product_id.clone());
    }

    // Processing parameters
    if let Some(level) = &meta.processing_level {
        metadata.insert("PROCESSING_LEVEL".to_string(), level.clone());
    }
    if let Some(multilook) = meta.multilook_factor {
        metadata.insert("MULTILOOK_FACTOR".to_string(), multilook.to_string());
    }
    if let Some(cal_type) = &meta.calibration_type {
        metadata.insert("CALIBRATION_TYPE".to_string(), cal_type.clone());
    }
    if let Some(noise) = meta.noise_estimate {
        metadata.insert("NOISE_ESTIMATE".to_string(), noise.to_string());
    }
    if let Some(center) = &meta.processing_center {
        metadata.insert("PROCESSING_CENTER".to_string(), center.clone());
    }
    if let Some(version) = &meta.software_version {
        metadata.insert("SOFTWARE_VERSION".to_string(), version.clone());
    }

    // Image characteristics
    if let Some(data_type) = &meta.pixel_data_type {
        metadata.insert("PIXEL_DATA_TYPE".to_string(), data_type.clone());
    }
    if let Some(bits) = meta.bits_per_sample {
        metadata.insert("BITS_PER_SAMPLE".to_string(), bits.to_string());
    }
    if let Some(sample_format) = &meta.sample_format {
        metadata.insert("SAMPLE_FORMAT".to_string(), sample_format.clone());
    }

    // Additional SAR-specific metadata
    if let Some(incidence) = meta.incidence_angle {
        metadata.insert("INCIDENCE_ANGLE".to_string(), incidence.to_string());
    }
    if let Some(look_angle) = meta.look_angle {
        metadata.insert("LOOK_ANGLE".to_string(), look_angle.to_string());
    }
    if let Some(doppler) = meta.doppler_centroid {
        metadata.insert("DOPPLER_CENTROID".to_string(), doppler.to_string());
    }
    if let Some(radiometric) = &meta.radiometric_calibration {
        metadata.insert("RADIOMETRIC_CALIBRATION".to_string(), radiometric.clone());
    }
    if let Some(geometric) = &meta.geometric_calibration {
        metadata.insert("GEOMETRIC_CALIBRATION".to_string(), geometric.clone());
    }

    // Conversion provenance
    metadata.insert("CONVERSION_TOOL".to_string(), meta.conversion_tool.clone());
    metadata.insert(
        "CONVERSION_VERSION".to_string(),
        meta.conversion_version.clone(),
    );
    metadata.insert(
        "CONVERSION_TIMESTAMP".to_string(),
        meta.conversion_timestamp.clone(),
    );

    metadata
}

/// Convert metadata HashMap to JSON format
pub fn convert_metadata_to_json(
    metadata: &HashMap<String, String>,
) -> HashMap<String, serde_json::Value> {
    let mut json_metadata = HashMap::new();

    for (key, value) in metadata {
        // Convert key to lowercase for JSON format
        let json_key = key.to_lowercase();

        // Try to parse as number first, then fall back to string
        if let Ok(num) = value.parse::<f64>() {
            if let Some(json_num) = serde_json::Number::from_f64(num) {
                json_metadata.insert(json_key, serde_json::Value::Number(json_num));
            } else {
                json_metadata.insert(json_key, serde_json::Value::String(value.clone()));
            }
        } else if let Ok(num) = value.parse::<u64>() {
            json_metadata.insert(
                json_key,
                serde_json::Value::Number(serde_json::Number::from(num)),
            );
        } else {
            json_metadata.insert(json_key, serde_json::Value::String(value.clone()));
        }
    }

    json_metadata
}

/// Handle special JSON fields that need array conversion
pub fn add_special_json_fields(
    json_metadata: &mut HashMap<String, serde_json::Value>,
    meta: &SafeMetadata,
    geotransform_override: Option<[f64; 6]>,
    projection_override: Option<&str>,
) {
    // Note: The processed polarization field (e.g., "SUM(VV, VH)") is preserved as "polarizations"
    // We don't add any additional polarization fields to avoid conflicts
    // That might or might not change in the future.

    // Handle geotransform as array
    if let Some(geotransform) = geotransform_override.or(meta.geotransform) {
        json_metadata.insert(
            "geotransform".to_string(),
            serde_json::Value::Array(
                geotransform
                    .iter()
                    .map(|&v| serde_json::Value::Number(serde_json::Number::from_f64(v).unwrap()))
                    .collect(),
            ),
        );
    }

    // Handle CRS field
    if let Some(crs) = projection_override.or(meta.crs.as_deref()) {
        if !crs.is_empty() {
            json_metadata.insert("crs".to_string(), serde_json::Value::String(crs.to_string()));
        }
    }
}

/// Embed comprehensive metadata into a GeoTIFF dataset
pub fn embed_tiff_metadata(
    ds: &mut Dataset,
    meta: &SafeMetadata,
    operation: Option<&str>,
    geotransform_override: Option<[f64; 6]>,
    projection_override: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Set georeferencing information
    let is_identity = |gt: [f64; 6]| gt[0] == 0.0 && gt[1] == 1.0 && gt[2] == 0.0 && gt[3] == 0.0 && gt[4] == 0.0 && gt[5] == 1.0;

    // Determine which geotransform to use and whether it's valid
    let mut set_gt = false;
    if let Some(gt) = geotransform_override {
        if !is_identity(gt) {
            ds.set_geo_transform(&gt)?;
            set_gt = true;
        }
    } else if let Some(gt) = meta.geotransform {
        if !is_identity(gt) {
            ds.set_geo_transform(&gt)?;
            set_gt = true;
        }
    }

    // Only set projection if we also set a non-identity geotransform
    if set_gt {
        if let Some(projection) = projection_override.or(meta.projection.as_deref()) {
            if !projection.is_empty() {
                ds.set_projection(projection)?;
            }
        }
    }

    // Extract all metadata fields
    let metadata = extract_metadata_fields(meta, operation);

    // Set metadata on the dataset
    for (key, value) in metadata {
        ds.set_metadata_item(&key, &value, "")?;
    }

    Ok(())
}

/// Create a sidecar metadata file for JPEG images
pub fn create_jpeg_metadata_sidecar(
    output_path: &Path,
    meta: &SafeMetadata,
    operation: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Extract all metadata fields
    let metadata = extract_metadata_fields(meta, operation);

    // Convert to JSON format
    let mut json_metadata = convert_metadata_to_json(&metadata);

    // Add special JSON fields
    add_special_json_fields(&mut json_metadata, meta, None, None);

    // Create sidecar file path
    let sidecar_path = output_path.with_extension("json");

    // Write metadata to JSON file
    let json_string = serde_json::to_string_pretty(&json_metadata)?;
    std::fs::write(&sidecar_path, json_string)?;

    info!("Created JPEG metadata sidecar: {:?}", sidecar_path);
    Ok(())
}

/// Create a sidecar metadata file with optional overrides for geotransform/projection
pub fn create_jpeg_metadata_sidecar_with_overrides(
    output_path: &Path,
    meta: &SafeMetadata,
    operation: Option<&str>,
    geotransform_override: Option<[f64; 6]>,
    projection_override: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let metadata = extract_metadata_fields(meta, operation);
    let mut json_metadata = convert_metadata_to_json(&metadata);
    add_special_json_fields(&mut json_metadata, meta, geotransform_override, projection_override);
    let sidecar_path = output_path.with_extension("json");
    let json_string = serde_json::to_string_pretty(&json_metadata)?;
    std::fs::write(&sidecar_path, json_string)?;
    info!("Created JPEG metadata sidecar: {:?}", sidecar_path);
    Ok(())
}

/// Generic metadata handler that can work with any format
pub fn handle_metadata(
    meta: &SafeMetadata,
    format: MetadataFormat,
    output_path: &Path,
    dataset: Option<&mut Dataset>,
) -> Result<(), Box<dyn std::error::Error>> {
    match format {
        MetadataFormat::Tiff => {
            let ds = dataset.ok_or("Dataset required for TIFF metadata")?;
            embed_tiff_metadata(ds, meta, None, None, None)
        }
        MetadataFormat::Json => create_jpeg_metadata_sidecar(output_path, meta, None),
    }
}


