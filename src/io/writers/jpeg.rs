use jpeg_encoder::{ColorType, Encoder};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub fn write_gray_jpeg(
    output: &Path,
    cols: usize,
    rows: usize,
    data: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(output)?;
    let mut writer = BufWriter::new(file);
    let encoder = Encoder::new(&mut writer, 100);
    encoder.encode(data, cols as u16, rows as u16, ColorType::Luma)?;
    Ok(())
}

pub fn write_rgb_jpeg(
    output: &Path,
    cols: usize,
    rows: usize,
    rgb_data: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(output)?;
    let mut writer = BufWriter::new(file);
    let encoder = Encoder::new(&mut writer, 100);
    encoder.encode(rgb_data, cols as u16, rows as u16, ColorType::Rgb)?;
    Ok(())
}


