
use std::path::Path;
use ndarray::Array2;
use tracing::info;
use tempfile;
use gdal::Dataset;
use gdal::raster::Buffer;
use gdal::raster::ResampleAlg;
use std::process::Command;

use crate::io::gdal::{GdalSarReader, to_gdal_path};

use super::errors::SafeError;
use super::types::SafeMetadata;

/// Load polarization data from a file and update metadata
pub fn load_polarization_data(
    file_path: &Path,
    metadata: &mut SafeMetadata,
) -> Result<Array2<f32>, SafeError> {
    info!("Loading underlying data from: {:?}", file_path);

    // Use GDAL to read GeoTIFF safely for large files
    let gdal_reader = GdalSarReader::open(file_path)
        .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;

    // Extract georeferencing information from GDAL
    metadata.geotransform = Some(gdal_reader.metadata.geotransform);
    metadata.projection = Some(gdal_reader.metadata.projection.clone());
    metadata.crs = Some(gdal_reader.metadata.projection.clone());

    // Read first band into f32 array
    let arr_f32 = gdal_reader
        .read_band(1, Some(ResampleAlg::NearestNeighbour))
        .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;

    // Update metadata dimensions
    let (rows, cols) = (arr_f32.nrows(), arr_f32.ncols());
    metadata.lines = rows;
    metadata.samples = cols;

    Ok(arr_f32)
}

/// Load polarization data with optional warp to target CRS
pub fn load_polarization_data_with_options(
    file_path: &Path,
    metadata: &mut SafeMetadata,
    target_crs: Option<&str>,
    resample_alg: Option<ResampleAlg>,
    target_size: Option<usize>,
) -> Result<Array2<f32>, SafeError> {
    if let Some(dst) = target_crs {
        info!("Warping to target CRS: {}", dst);
        let tmp_in = file_path;
        // Build a dedicated temp output file path outside SAFE tree
        let stem = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("warped");
        // Create a unique temporary file using tempfile crate (auto-cleanup fallback)
        let mut tmp_builder = tempfile::Builder::new();
        let tmp_file = tmp_builder
            .prefix(&format!("{}_", stem))
            .suffix("_warped.vrt")
            .tempfile()
            .map_err(|e| SafeError::Parse(format!("tempfile error: {}", e)))?;
        let tmp_out = tmp_file.path().to_path_buf();
        let resample_str = match resample_alg.unwrap_or(ResampleAlg::Bilinear) {
            ResampleAlg::NearestNeighbour => "near",
            ResampleAlg::Bilinear => "bilinear",
            ResampleAlg::Cubic => "cubic",
            ResampleAlg::Lanczos => "lanczos",
            _ => "bilinear",
        };
        // Determine source CRS from dataset
        // ensure source can be remote via VSI path
        let src_vsi = to_gdal_path(tmp_in);
        let src_ds = Dataset::open(Path::new(src_vsi.as_ref()))
            .map_err(|e| SafeError::Parse(format!("GDAL open error: {}", e)))?;
        let ds_proj = src_ds.projection();
        // Helper: parse EPSG from a WKT string
        let parse_epsg = |wkt: &str| -> Option<String> {
            let key = "AUTHORITY[\"EPSG\",\"";
            if let Some(idx) = wkt.rfind(key) {
                let start = idx + key.len();
                if let Some(end) = wkt[start..].find('"') {
                    let code = &wkt[start..start + end];
                    return Some(format!("EPSG:{}", code));
                }
            }
            None
        };
        // Guard: if dataset already has a projection equal to target, skip warping
        if !ds_proj.is_empty() {
            if let Some(epsg) = parse_epsg(&ds_proj).or_else(|| {
                if ds_proj.starts_with("EPSG:") {
                    Some(ds_proj.clone())
                } else {
                    None
                }
            }) {
                if epsg.eq_ignore_ascii_case(dst) {
                    info!("Input already in target CRS ({}); skipping warp", dst);
                    // Read directly using GDAL reader
                    let gdal_reader = GdalSarReader::open(file_path)
                        .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;
                    // Update metadata
                    metadata.geotransform = Some(gdal_reader.metadata.geotransform);
                    metadata.projection = Some(gdal_reader.metadata.projection.clone());
                    metadata.crs = Some(gdal_reader.metadata.projection.clone());
                    let arr_f32 = gdal_reader
                        .read_band(1, Some(ResampleAlg::NearestNeighbour))
                        .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;
                    let (rows, cols) = (arr_f32.nrows(), arr_f32.ncols());
                    metadata.lines = rows;
                    metadata.samples = cols;
                    return Ok(arr_f32);
                }
            }
        }
        // Choose warp mode: if dataset has no projection, use GCP+tps with s_srs; otherwise, standard warp
        let mut args: Vec<String> = vec![
            "-of".into(),
            "VRT".into(),
            "-overwrite".into(),
            "-r".into(),
            resample_str.into(),
            // Performance knobs
            "-multi".into(),
            "-wo".into(),
            "NUM_THREADS=ALL_CPUS".into(),
            "-wm".into(),
            "512".into(),
            "--config".into(),
            "GDAL_CACHEMAX".into(),
            "512".into(),
        ];
        // If a target image long side is provided, compute output cols/rows preserving aspect
        if let Some(ts) = target_size {
            let (size_x, size_y) = src_ds.raster_size();
            let (orig_cols, orig_rows) = (size_x as usize, size_y as usize);
            let long_side = orig_cols.max(orig_rows);
            let scale = (ts as f64 / long_side as f64).min(1.0);
            let out_cols = ((orig_cols as f64) * scale).round().max(1.0) as usize;
            let out_rows = ((orig_rows as f64) * scale).round().max(1.0) as usize;
            args.push("-ts".into());
            args.push(out_cols.to_string());
            args.push(out_rows.to_string());
        }
        if ds_proj.is_empty() {
            // Use GCPs via thin plate spline to geolocate Sentinel-1 GRD rasters
            args.push("-tps".into());
            // Source SRS from GCP projection (fallback to EPSG:4326)
            let src_gcp_proj = src_ds.gcp_projection().unwrap_or_else(|| "".to_string());
            let src_srs = if src_gcp_proj.trim().is_empty() {
                "EPSG:4326".to_string()
            } else {
                src_gcp_proj
            };
            args.push("-s_srs".into());
            args.push(src_srs);
        }
        args.push("-t_srs".into());
        args.push(dst.to_string());
        // pass VSI path into gdalwarp
        args.push(src_vsi.into_owned());
        args.push(tmp_out.to_str().unwrap().to_string());
        let status = Command::new("gdalwarp")
            .args(args.iter().map(|s| s.as_str()))
            .status()
            .map_err(|e| SafeError::Parse(format!("gdalwarp exec error: {}", e)))?;
        if !status.success() {
            // Best-effort cleanup
            let _ = std::fs::remove_file(&tmp_out);
            return Err(SafeError::Parse("gdalwarp failed".to_string()));
        }
        let ds: Dataset = Dataset::open(&tmp_out)
            .map_err(|e| SafeError::Parse(format!("GDAL open warped error: {}", e)))?;

        // Update metadata from warped dataset
        if let Ok(gt) = ds.geo_transform() {
            metadata.geotransform = Some(gt);
        }
        let proj = ds.projection();
        if !proj.is_empty() {
            metadata.projection = Some(proj.clone());
            metadata.crs = Some(proj);
        }

        // Read first band; if we requested -ts, ds size is already the target
        let (size_x, size_y) = ds.raster_size();
        let band = ds
            .rasterband(1)
            .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;
        let buf: Buffer<f32> = band
            .read_as((0, 0), (size_x, size_y), (size_x, size_y), None)
            .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;
        let data_vec = buf.data().to_vec();
        let data = Array2::from_shape_vec((size_y as usize, size_x as usize), data_vec)
            .map_err(|_| SafeError::Parse("warped array shape error".to_string()))?;
        // Update dims
        metadata.lines = size_y as usize;
        metadata.samples = size_x as usize;
        // Clean up temporary VRT file (small)
        let _ = std::fs::remove_file(&tmp_out);
        return Ok(data);
    }
    // Fallback: no warp. If target_size is provided, downsample on read.
    if let Some(ts) = target_size {
        info!("Reading at target size (long side): {}", ts);
        // Use GDAL reader to resample band directly
        let gdal_reader = GdalSarReader::open(file_path)
            .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;
        // Update georef metadata
        metadata.geotransform = Some(gdal_reader.metadata.geotransform);
        metadata.projection = Some(gdal_reader.metadata.projection.clone());
        metadata.crs = Some(gdal_reader.metadata.projection.clone());
        // Compute output cols/rows preserving aspect
        let (orig_cols, orig_rows) = (gdal_reader.metadata.size_x, gdal_reader.metadata.size_y);
        let long_side = orig_cols.max(orig_rows);
        let scale = (ts as f64 / long_side as f64).min(1.0);
        let out_cols = ((orig_cols as f64) * scale).round().max(1.0) as usize;
        let out_rows = ((orig_rows as f64) * scale).round().max(1.0) as usize;
        // Choose a high-quality downsampling filter when shrinking aggressively.
        // Respect user-provided resampling if any; otherwise, use Lanczos for mild downscale,
        // and Average for heavy downscale to suppress aliasing/speckle.
        let reduction = (orig_cols.max(orig_rows) as f64 / ts as f64).max(1.0);
        let chosen_alg = match resample_alg {
            Some(alg) => alg,
            None => {
                if reduction >= 4.0 {
                    ResampleAlg::Average
                } else {
                    ResampleAlg::Lanczos
                }
            }
        };
        let arr_f32 = gdal_reader
            .read_band_resampled(1, out_cols, out_rows, Some(chosen_alg))
            .map_err(|e| SafeError::Parse(format!("GDAL error: {}", e)))?;
        metadata.lines = out_rows;
        metadata.samples = out_cols;
        return Ok(arr_f32);
    }
    // No warp, full-resolution read
    load_polarization_data(file_path, metadata)
}


