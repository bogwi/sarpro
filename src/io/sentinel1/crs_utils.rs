use std::path::{Path, PathBuf};
use std::fs;
use tracing::{info, warn};
use gdal::Dataset;
use std::process::Command;
use serde_json::Value;

/// Resolve an automatic target CRS (EPSG:XXXXX) based on the SAFE product's geolocation.
///
/// Strategy:
/// - Locate any measurement TIFF inside the SAFE `measurement/` folder (prefer VV/VH/HH/HV by name).
/// - Run `gdalinfo -json` on that TIFF to obtain WGS84 coordinates (from `wgs84Extent`, `gcps`, or `cornerCoordinates`).
/// - Compute a representative lon/lat (centroid) and map to UTM EPSG:326xx/327xx, with UPS fallback near poles
///   and Norway/Svalbard UTM exceptions.
pub fn resolve_auto_target_crs<P: AsRef<Path>>(safe_dir: P) -> Option<String> {
    let base = safe_dir.as_ref();
    let measurement = base.join("measurement");
    if !measurement.is_dir() {
        warn!("AUTO-CRS: measurement directory not found: {:?}", measurement);
        return None;
    }
    // Find a candidate measurement TIFF (prefer names containing polarization hints)
    let mut candidate: Option<PathBuf> = None;
    if let Ok(entries) = fs::read_dir(&measurement) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext_lc = ext.to_lowercase();
                if ext_lc == "tiff" || ext_lc == "tif" {
                    let name_lc = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    // Skip intermediates
                    if name_lc.contains("_warped.tif") || name_lc.contains("_warped.tiff") {
                        continue;
                    }
                    // Prefer VV/VH then HH/HV, else take the first TIFF
                    if name_lc.contains("vv") || name_lc.contains("vh") {
                        candidate = Some(path.clone());
                        break;
                    } else if name_lc.contains("hh") || name_lc.contains("hv") {
                        candidate = Some(path.clone());
                    } else if candidate.is_none() {
                        candidate = Some(path.clone());
                    }
                }
            }
        }
    }
    let file_path = match candidate {
        Some(p) => p,
        None => {
            warn!("AUTO-CRS: no measurement TIFF found in {:?}", measurement);
            return None;
        }
    };
    info!("AUTO-CRS: candidate measurement: {:?}", file_path.file_name());

    // Preferred: use GDAL GCP API to read lon/lat from GCPs
    let mut lonlat: Option<(f64, f64)> = None;
    match Dataset::open(&file_path) {
        Ok(ds) => {
            let gcp_proj = ds.gcp_projection().unwrap_or_else(|| "".to_string());
            let proj_is_geographic = gcp_proj.contains("GEOGCS") || gcp_proj.contains("WGS 84") || gcp_proj.starts_with("EPSG:4326");
            if proj_is_geographic {
                let gcps = ds.gcps();
                if !gcps.is_empty() {
                    let mut sum_lon = 0.0;
                    let mut sum_lat = 0.0;
                    let mut count = 0.0;
                    for g in gcps {
                        sum_lon += g.x();
                        sum_lat += g.y();
                        count += 1.0;
                    }
                    if count > 0.0 {
                        lonlat = Some((sum_lon / count, sum_lat / count));
                        info!("AUTO-CRS: centroid from GDAL GCPs: lon={:.6}, lat={:.6}", sum_lon / count, sum_lat / count);
                    } else {
                        warn!("AUTO-CRS: GDAL reports zero GCPs");
                    }
                } else {
                    warn!("AUTO-CRS: no GCPs available via GDAL API");
                }
            } else {
                warn!("AUTO-CRS: GCP projection not geographic or empty; proj='{}'", gcp_proj);
            }
        }
        Err(e) => {
            warn!("AUTO-CRS: GDAL open failed for candidate: {}", e);
        }
    }

    // Fallback: gdalinfo -json (parse correct gcps.gcpList)
    if lonlat.is_none() {
        let output = Command::new("gdalinfo")
            .arg("-json")
            .arg(file_path.as_os_str())
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                if let Ok(json_text) = std::str::from_utf8(&out.stdout) {
                    if let Ok(v) = serde_json::from_str::<Value>(json_text) {
                        // Try wgs84Extent
                        if let Some(ext) = v.get("wgs84Extent") {
                            if let Some(coords) = ext.get("coordinates").and_then(|c| c.as_array()).and_then(|arr| arr.get(0)).and_then(|ring| ring.as_array()) {
                                let mut sum_lon = 0.0;
                                let mut sum_lat = 0.0;
                                let mut count = 0.0;
                                for pt in coords {
                                    if let Some(pair) = pt.as_array() {
                                        if pair.len() >= 2 {
                                            if let (Some(lon), Some(lat)) = (pair[0].as_f64(), pair[1].as_f64()) {
                                                sum_lon += lon;
                                                sum_lat += lat;
                                                count += 1.0;
                                            }
                                        }
                                    }
                                }
                                if count > 0.0 {
                                    lonlat = Some((sum_lon / count, sum_lat / count));
                                    info!("AUTO-CRS: centroid from wgs84Extent: lon={:.6}, lat={:.6}", sum_lon / count, sum_lat / count);
                                }
                            }
                        }
                        // GCPs list
                        if lonlat.is_none() {
                            if let Some(gcps_obj) = v.get("gcps").and_then(|g| g.as_object()) {
                                if let Some(list) = gcps_obj.get("gcpList").and_then(|l| l.as_array()) {
                                    let mut sum_lon = 0.0;
                                    let mut sum_lat = 0.0;
                                    let mut count = 0.0;
                                    for gcp in list {
                                        if let (Some(lon), Some(lat)) = (gcp.get("lon").and_then(|x| x.as_f64()), gcp.get("lat").and_then(|x| x.as_f64())) {
                                            sum_lon += lon;
                                            sum_lat += lat;
                                            count += 1.0;
                                        }
                                    }
                                    if count > 0.0 {
                                        lonlat = Some((sum_lon / count, sum_lat / count));
                                        info!("AUTO-CRS: centroid from gdalinfo GCPs: lon={:.6}, lat={:.6}", sum_lon / count, sum_lat / count);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // Map lon/lat to UTM/UPS EPSG string
    let (lon, lat) = match lonlat {
        Some(v) => v,
        None => {
            warn!("AUTO-CRS: could not compute lon/lat from GDAL or gdalinfo JSON");
            return None;
        }
    };
    let epsg = lonlat_to_epsg(lon, lat);
    info!("AUTO-CRS: resolved target CRS = {}", epsg);
    Some(epsg)
}

pub fn lonlat_to_epsg(lon: f64, lat: f64) -> String {
    // Polar UPS fallback
    if lat >= 84.0 {
        return "EPSG:32661".to_string();
    }
    if lat <= -80.0 {
        return "EPSG:32761".to_string();
    }
    // Normalize longitude to [-180, 180)
    let mut lon_norm = lon;
    if lon_norm < -180.0 || lon_norm >= 180.0 {
        lon_norm = ((lon_norm + 180.0) % 360.0 + 360.0) % 360.0 - 180.0;
    }
    // Norway exception: 56<=lat<64 and 3<=lon<12 -> zone 32
    let norway_exception = lat >= 56.0 && lat < 64.0 && lon_norm >= 3.0 && lon_norm < 12.0;
    // Svalbard exceptions: 72<=lat<84 and lon bands map to zones 31,33,35,37
    let svalbard_exception = lat >= 72.0 && lat < 84.0;

    let zone = if norway_exception {
        32
    } else if svalbard_exception {
        if lon_norm >= 0.0 && lon_norm < 9.0 {
            31
        } else if lon_norm >= 9.0 && lon_norm < 21.0 {
            33
        } else if lon_norm >= 21.0 && lon_norm < 33.0 {
            35
        } else if lon_norm >= 33.0 && lon_norm < 42.0 {
            37
        } else {
            // Default zone computation outside special bands within Svalbard range
            (((lon_norm + 180.0) / 6.0).floor() as i32 + 1).clamp(1, 60)
        }
    } else {
        (((lon_norm + 180.0) / 6.0).floor() as i32 + 1).clamp(1, 60)
    } as i32;

    if lat >= 0.0 {
        format!("EPSG:326{:02}", zone)
    } else {
        format!("EPSG:327{:02}", zone)
    }
}

/// Resolve an automatic target CRS (EPSG:XXXXX) based on any raster dataset path
/// including GDAL VSI paths (e.g., /vsicurl/, /vsizip//vsicurl/...).
/// Uses the same centroid-from-GCPs or gdalinfo-json fallback strategy.
pub fn resolve_auto_target_crs_from_dataset_path<P: AsRef<std::path::Path>>(dataset_path: P) -> Option<String> {

    let path_ref = dataset_path.as_ref();
    let mut lonlat: Option<(f64, f64)> = None;

    // Preferred: use GDAL GCP API to read lon/lat from GCPs
    match gdal::Dataset::open(path_ref) {
        Ok(ds) => {
            let gcp_proj = ds.gcp_projection().unwrap_or_else(|| "".to_string());
            let proj_is_geographic = gcp_proj.contains("GEOGCS")
                || gcp_proj.contains("WGS 84")
                || gcp_proj.starts_with("EPSG:4326");
            if proj_is_geographic {
                let gcps = ds.gcps();
                if !gcps.is_empty() {
                    let mut sum_lon = 0.0;
                    let mut sum_lat = 0.0;
                    let mut count = 0.0;
                    for g in gcps {
                        sum_lon += g.x();
                        sum_lat += g.y();
                        count += 1.0;
                    }
                    if count > 0.0 {
                        lonlat = Some((sum_lon / count, sum_lat / count));
                        tracing::info!(
                            "AUTO-CRS (remote): centroid from GDAL GCPs: lon={:.6}, lat={:.6}",
                            sum_lon / count,
                            sum_lat / count
                        );
                    } else {
                        tracing::warn!("AUTO-CRS (remote): GDAL reports zero GCPs");
                    }
                } else {
                    tracing::warn!("AUTO-CRS (remote): no GCPs available via GDAL API");
                }
            } else {
                tracing::warn!(
                    "AUTO-CRS (remote): GCP projection not geographic or empty; proj='{}'",
                    gcp_proj
                );
            }
        }
        Err(e) => tracing::warn!("AUTO-CRS (remote): GDAL open failed for candidate: {}", e),
    }

    // Fallback: gdalinfo -json
    if lonlat.is_none() {
        let output = std::process::Command::new("gdalinfo")
            .arg("-json")
            .arg(path_ref.as_os_str())
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                if let Ok(json_text) = std::str::from_utf8(&out.stdout) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_text) {
                        // Try wgs84Extent
                        if let Some(ext) = v.get("wgs84Extent") {
                            if let Some(coords) = ext
                                .get("coordinates")
                                .and_then(|c| c.as_array())
                                .and_then(|arr| arr.get(0))
                                .and_then(|ring| ring.as_array())
                            {
                                let mut sum_lon = 0.0;
                                let mut sum_lat = 0.0;
                                let mut count = 0.0;
                                for pt in coords {
                                    if let Some(pair) = pt.as_array() {
                                        if pair.len() >= 2 {
                                            if let (Some(lon), Some(lat)) =
                                                (pair[0].as_f64(), pair[1].as_f64())
                                            {
                                                sum_lon += lon;
                                                sum_lat += lat;
                                                count += 1.0;
                                            }
                                        }
                                    }
                                }
                                if count > 0.0 {
                                    lonlat = Some((sum_lon / count, sum_lat / count));
                                    tracing::info!(
                                        "AUTO-CRS (remote): centroid from wgs84Extent: lon={:.6}, lat={:.6}",
                                        sum_lon / count,
                                        sum_lat / count
                                    );
                                }
                            }
                        }
                        // GCPs list
                        if lonlat.is_none() {
                            if let Some(gcps_obj) = v.get("gcps").and_then(|g| g.as_object()) {
                                if let Some(list) = gcps_obj
                                    .get("gcpList")
                                    .and_then(|l| l.as_array())
                                {
                                    let mut sum_lon = 0.0;
                                    let mut sum_lat = 0.0;
                                    let mut count = 0.0;
                                    for gcp in list {
                                        if let (Some(lon), Some(lat)) = (
                                            gcp.get("lon").and_then(|x| x.as_f64()),
                                            gcp.get("lat").and_then(|x| x.as_f64()),
                                        ) {
                                            sum_lon += lon;
                                            sum_lat += lat;
                                            count += 1.0;
                                        }
                                    }
                                    if count > 0.0 {
                                        lonlat = Some((sum_lon / count, sum_lat / count));
                                        tracing::info!(
                                            "AUTO-CRS (remote): centroid from gdalinfo GCPs: lon={:.6}, lat={:.6}",
                                            sum_lon / count,
                                            sum_lat / count
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let (lon, lat) = match lonlat {
        Some(v) => v,
        None => {
            tracing::warn!("AUTO-CRS (remote): could not compute lon/lat from GDAL or gdalinfo JSON");
            return None;
        }
    };
    let epsg = lonlat_to_epsg(lon, lat);
    tracing::info!("AUTO-CRS (remote): resolved target CRS = {}", epsg);
    Some(epsg)
}
