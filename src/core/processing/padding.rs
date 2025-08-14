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
    info!("Final dimensions: {}x{}", cols, rows + pad_rows * 2);

    match bit_depth {
        BitDepth::U8 => {
            let mut padded = vec![0u8; max_dim * max_dim];
            for row in 0..rows {
                for col in 0..cols {
                    let src_idx = row * cols + col;
                    let dst_idx = (row + pad_rows) * max_dim + (col + pad_cols);
                    padded[dst_idx] = u8_data[src_idx];
                }
            }
            Ok((padded, None))
        }
        BitDepth::U16 => {
            let u16_data = u16_data.ok_or("U16 data required for U16 bit depth")?;
            let mut padded = vec![0u16; max_dim * max_dim];
            for row in 0..rows {
                for col in 0..cols {
                    let src_idx = row * cols + col;
                    let dst_idx = (row + pad_rows) * max_dim + (col + pad_cols);
                    padded[dst_idx] = u16_data[src_idx];
                }
            }
            Ok((vec![], Some(padded)))
        }
    }
}
