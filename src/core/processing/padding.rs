use tracing::info;

use crate::types::BitDepth;

pub fn add_padding_to_square(
    u8_data: &[u8],
    u16_data: Option<&[u16]>,
    cols: usize,
    rows: usize,
    bit_depth: BitDepth,
) -> Result<(Vec<u8>, Option<Vec<u16>>), Box<dyn std::error::Error>> {
    let max_dim = cols.max(rows);
    let pad_cols = (max_dim - cols) / 2;
    let pad_rows = (max_dim - rows) / 2;

    info!(
        "Adding padding: cols={}, rows={}, pad_cols={}, pad_rows={}",
        cols, rows, pad_cols, pad_rows
    );
    info!("Final dimensions: {}x{}", max_dim, max_dim);

    match bit_depth {
        BitDepth::U8 => {
            let mut padded = vec![0u8; max_dim * max_dim];
            // Copy per row using slice copies to minimize per-pixel indexing
            for row in 0..rows {
                let src_offset = row * cols;
                let dst_offset = (row + pad_rows) * max_dim + pad_cols;
                let src_slice = &u8_data[src_offset..src_offset + cols];
                let dst_slice = &mut padded[dst_offset..dst_offset + cols];
                dst_slice.copy_from_slice(src_slice);
            }
            Ok((padded, None))
        }
        BitDepth::U16 => {
            let u16_data = u16_data.ok_or("U16 data required for U16 bit depth")?;
            let mut padded = vec![0u16; max_dim * max_dim];
            // Copy per row using slice copies to minimize per-pixel indexing
            for row in 0..rows {
                let src_offset = row * cols;
                let dst_offset = (row + pad_rows) * max_dim + pad_cols;
                let src_slice = &u16_data[src_offset..src_offset + cols];
                let dst_slice = &mut padded[dst_offset..dst_offset + cols];
                dst_slice.copy_from_slice(src_slice);
            }
            Ok((vec![], Some(padded)))
        }
    }
}
