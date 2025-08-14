use gdal::Dataset;
use gdal::DriverManager;
use gdal::raster::{Buffer, ColorInterpretation};
use std::path::Path;

pub fn write_tiff_u8(
    output: &Path,
    cols: usize,
    rows: usize,
    data: &[u8],
) -> Result<Dataset, Box<dyn std::error::Error>> {
    let driver = DriverManager::get_driver_by_name("GTiff")?;
    let ds = driver.create_with_band_type::<u8, _>(output, cols, rows, 1)?;
    let mut buf = Buffer::new((cols, rows), data.to_vec());
    let mut band = ds.rasterband(1)?;
    band.write((0, 0), (cols, rows), &mut buf)?;
    Ok(ds)
}

pub fn write_tiff_u16(
    output: &Path,
    cols: usize,
    rows: usize,
    data: &[u16],
) -> Result<Dataset, Box<dyn std::error::Error>> {
    let driver = DriverManager::get_driver_by_name("GTiff")?;
    let ds = driver.create_with_band_type::<u16, _>(output, cols, rows, 1)?;
    let mut buf = Buffer::new((cols, rows), data.to_vec());
    let mut band = ds.rasterband(1)?;
    band.write((0, 0), (cols, rows), &mut buf)?;
    Ok(ds)
}

pub fn write_tiff_multiband_u8(
    output: &Path,
    cols: usize,
    rows: usize,
    band1: &[u8],
    band2: &[u8],
) -> Result<Dataset, Box<dyn std::error::Error>> {
    let driver = DriverManager::get_driver_by_name("GTiff")?;
    let ds = driver.create_with_band_type::<u8, _>(output, cols, rows, 2)?;

    let mut band1_handle = ds.rasterband(1)?;
    band1_handle.set_color_interpretation(ColorInterpretation::GrayIndex)?;
    let mut buf1 = Buffer::new((cols, rows), band1.to_vec());
    band1_handle.write((0, 0), (cols, rows), &mut buf1)?;

    let mut band2_handle = ds.rasterband(2)?;
    band2_handle.set_color_interpretation(ColorInterpretation::GrayIndex)?;
    let mut buf2 = Buffer::new((cols, rows), band2.to_vec());
    band2_handle.write((0, 0), (cols, rows), &mut buf2)?;

    Ok(ds)
}

pub fn write_tiff_multiband_u16(
    output: &Path,
    cols: usize,
    rows: usize,
    band1: &[u16],
    band2: &[u16],
) -> Result<Dataset, Box<dyn std::error::Error>> {
    let driver = DriverManager::get_driver_by_name("GTiff")?;
    let ds = driver.create_with_band_type::<u16, _>(output, cols, rows, 2)?;

    let mut band1_handle = ds.rasterband(1)?;
    band1_handle.set_color_interpretation(ColorInterpretation::GrayIndex)?;
    let mut buf1 = Buffer::new((cols, rows), band1.to_vec());
    band1_handle.write((0, 0), (cols, rows), &mut buf1)?;

    let mut band2_handle = ds.rasterband(2)?;
    band2_handle.set_color_interpretation(ColorInterpretation::GrayIndex)?;
    let mut buf2 = Buffer::new((cols, rows), band2.to_vec());
    band2_handle.write((0, 0), (cols, rows), &mut buf2)?;

    Ok(ds)
}
