//! Iterator over SAFE product directories

use std::path::Path;
use crate::error::{Error, Result};

/// Return an iterator over immediate subdirectories of `input_dir` (candidate SAFE products)
pub fn iterate_safe_products(input_dir: &Path) -> Result<std::vec::IntoIter<std::path::PathBuf>> {
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(input_dir).map_err(Error::from)? {
        let entry = entry.map_err(Error::from)?;
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        }
    }
    Ok(dirs.into_iter())
}
