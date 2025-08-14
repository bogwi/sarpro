use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Write a world file next to the raster image using the provided geotransform.
/// The world file stores the transform in pixel-center convention.
pub fn write_world_file(
    output_image: &Path,
    geotransform: [f64; 6],
) -> Result<(), Box<dyn std::error::Error>> {
    let ext = output_image
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let world_ext = match ext.as_str() {
        "jpg" | "jpeg" => "jgw",
        "png" => "pgw",
        "tif" | "tiff" => "tfw",
        other if !other.is_empty() => {
            // Fallback: use first letter + "w"
            let first = other.chars().next().unwrap_or('w');
            let mut s = String::new();
            s.push(first);
            s.push_str("w");
            Box::leak(s.into_boxed_str())
        }
        _ => "wld",
    };

    let world_path = output_image.with_extension(world_ext);

    // Convert GDAL geotransform to world file parameters:
    // A: pixel size in X, D: rotation about Y, B: rotation about X, E: pixel size Y
    // C, F: center of upper-left pixel
    let a = geotransform[1];
    let d = geotransform[4];
    let b = geotransform[2];
    let e = geotransform[5];
    let c = geotransform[0] + 0.5 * a + 0.5 * b;
    let f = geotransform[3] + 0.5 * d + 0.5 * e;

    let mut file = File::create(world_path)?;
    // One value per line, high precision
    writeln!(file, "{:.12}", a)?;
    writeln!(file, "{:.12}", d)?;
    writeln!(file, "{:.12}", b)?;
    writeln!(file, "{:.12}", e)?;
    writeln!(file, "{:.12}", c)?;
    writeln!(file, "{:.12}", f)?;

    Ok(())
}

/// Write a .prj file with the provided projection (WKT or EPSG:XXXX)
pub fn write_prj_file(
    output_image: &Path,
    projection: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let prj_path = output_image.with_extension("prj");
    std::fs::write(prj_path, projection.as_bytes())?;
    Ok(())
}
