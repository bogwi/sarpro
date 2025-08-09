use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image};
use tracing::{warn, info};
use crate::types::BitDepth;
use crate::core::processing::padding::add_padding_to_square;

pub fn calculate_resize_dimensions(
    original_cols: usize,
    original_rows: usize,
    target_size: usize,
) -> (usize, usize) {
    let short_side = original_rows.min(original_cols);
    let long_side = original_rows.max(original_cols);

    if target_size > long_side {
        warn!(
            "Target size {} is larger than original long side {}. Keeping original dimensions {}x{}",
            target_size, long_side, original_cols, original_rows
        );
        return (original_cols, original_rows);
    }

    let scale_factor = target_size as f64 / long_side as f64;
    let new_short_side = (short_side as f64 * scale_factor).round() as usize;

    if original_cols > original_rows {
        (target_size, new_short_side)
    } else {
        (new_short_side, target_size)
    }
}

pub fn resize_u8_image(
    data: &[u8],
    original_cols: usize,
    original_rows: usize,
    target_cols: usize,
    target_rows: usize,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let resize_options =
        ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    let mut resizer = Resizer::new();

    let src_image = Image::from_vec_u8(
        original_cols as u32,
        original_rows as u32,
        data.to_vec(),
        PixelType::U8,
    )?;
    let mut dst_image = Image::new(target_cols as u32, target_rows as u32, PixelType::U8);
    resizer.resize(&src_image, &mut dst_image, &resize_options)?;

    Ok(dst_image.into_vec())
}

pub fn resize_u16_image(
    data: &[u16],
    original_cols: usize,
    original_rows: usize,
    target_cols: usize,
    target_rows: usize,
) -> Result<Vec<u16>, Box<dyn std::error::Error>> {
    use crate::core::processing::autoscale::scale_u16_to_u8;
    let u8_data = scale_u16_to_u8(data);
    let resized_u8 = resize_u8_image(
        &u8_data,
        original_cols,
        original_rows,
        target_cols,
        target_rows,
    )?;
    let resized_u16: Vec<u16> = resized_u8.into_iter().map(|x| (x as u16) << 8).collect();
    Ok(resized_u16)
}

pub fn resize_image_data(
    u8_data: &[u8],
    u16_data: Option<&[u16]>,
    original_cols: usize,
    original_rows: usize,
    target_size: Option<usize>,
    bit_depth: BitDepth,
    pad: bool,
) -> Result<(usize, usize, Vec<u8>, Option<Vec<u16>>), Box<dyn std::error::Error>> {
    if let Some(size) = target_size {
        info!("Resizing image to {} (long side)", size);

        let (new_cols, new_rows) = calculate_resize_dimensions(original_cols, original_rows, size);

        info!(
            "Original size: {}x{}, New size: {}x{}",
            original_cols, original_rows, new_cols, new_rows
        );

        let (resized_u8, resized_u16) = match bit_depth {
            BitDepth::U8 => {
                let resized_u8 =
                    resize_u8_image(u8_data, original_cols, original_rows, new_cols, new_rows)?;
                (resized_u8, None)
            }
            BitDepth::U16 => {
                info!("Resizing U16 image to U8: currently all scaling is done to 8 bit");
                let u16_data = u16_data.ok_or("U16 data required for U16 bit depth")?;
                let resized_u16 =
                    resize_u16_image(u16_data, original_cols, original_rows, new_cols, new_rows)?;
                (vec![], Some(resized_u16))
            }
        };

        if pad {
            let (padded_u8, padded_u16) = add_padding_to_square(
                &resized_u8,
                resized_u16.as_deref(),
                new_cols,
                new_rows,
                bit_depth,
            )?;
            let final_dim = new_cols.max(new_rows);
            Ok((final_dim, final_dim, padded_u8, padded_u16))
        } else {
            Ok((new_cols, new_rows, resized_u8, resized_u16))
        }
    } else {
        if pad {
            let (padded_u8, padded_u16) =
                add_padding_to_square(u8_data, u16_data, original_cols, original_rows, bit_depth)?;
            let final_dim = original_cols.max(original_rows);
            Ok((final_dim, final_dim, padded_u8, padded_u16))
        } else {
            match bit_depth {
                BitDepth::U8 => Ok((original_cols, original_rows, u8_data.to_vec(), None)),
                BitDepth::U16 => {
                    let u16_data = u16_data.ok_or("U16 data required for U16 bit depth")?;
                    Ok((original_cols, original_rows, vec![], Some(u16_data.to_vec())))
                }
            }
        }
    }
}




