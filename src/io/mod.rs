//! I/O layer for reading SAFE products and GDAL-backed rasters.
//! Provides the `sentinel1` SAFE reader, `gdal` adapters, and `writers`
//! for TIFF/JPEG outputs and metadata embedding/sidecars.
pub mod sentinel1;
pub use sentinel1::{ProductType, SafeError, SafeMetadata, SafeReader};

pub mod gdal;
pub use gdal::{GdalError, GdalMetadata, GdalSarReader};

pub mod writers;
