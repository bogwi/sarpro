#![doc = r#"
SARPRO — a high-performance Sentinel-1 GRD processing toolkit.

This crate provides a typed, ergonomic API for turning Sentinel-1 SAFE (GRD) products
into high-quality GeoTIFFs or JPEGs, with optional resizing, padding, autoscaling, and
polarization operations. It powers both the SARPRO CLI and GUI, and can be embedded in
your own Rust applications.

Stability
---------
The public library API is experimental in initial releases. It is built on top of a
working MVP used by the CLI/GUI and is robust, but may evolve as the crate stabilizes.
Breaking changes can occur.

Requirements
------------
- GDAL development headers and runtime available on your system.
- Rust 2024 edition toolchain.

Add dependency
--------------
```toml
[dependencies]
sarpro = { version = "0.1", features = ["full"] }
```

Quick start: process a SAFE to a file
-------------------------------------
```rust,no_run
use std::path::Path;
use sarpro::{
    process_safe_to_path,
    ProcessingParams,
    AutoscaleStrategy, BitDepthArg, OutputFormat, Polarization, InputFormat,
};

fn main() -> sarpro::Result<()> {
    let params = ProcessingParams {
        format: OutputFormat::TIFF,
        input_format: InputFormat::Safe,
        bit_depth: BitDepthArg::U16,
        polarization: Polarization::Multiband,
        autoscale: AutoscaleStrategy::Tamed,
        target_crs: Some("EPSG:32630".to_string()),
        resample_alg: Some("lanczos".to_string()),
        size: Some(2048),
        pad: true,
    };

    process_safe_to_path(
        Path::new("/data/S1A_example.SAFE"),
        Path::new("/out/product.tiff"),
        &params,
    )
}
```

Process in-memory to `ProcessedImage`
-------------------------------------
```rust,no_run
use std::path::Path;
use sarpro::{
    process_safe_to_buffer,
    AutoscaleStrategy, BitDepth, OutputFormat, Polarization,
};

fn main() -> sarpro::Result<()> {
    let img = process_safe_to_buffer(
        Path::new("/data/S1A_example.SAFE"),
        Polarization::Multiband,
        AutoscaleStrategy::Tamed,
        BitDepth::U8,
        Some(1024),
        true,
        OutputFormat::JPEG,
    )?;

    // Use `img` buffers in your pipeline (TIFF grayscale/multiband or synthetic RGB JPEG)
    // and/or consult its metadata.
    Ok(())
}
```

Typed save helpers (when you already have arrays)
-------------------------------------------------
```rust
use std::path::Path;
use ndarray::Array2;
use num_complex::Complex;
use sarpro::{
    save_image, save_multiband_image,
    AutoscaleStrategy, BitDepth, OutputFormat, ProcessingOperation,
};

fn save_single(processed: &Array2<Complex<f64>>) -> sarpro::Result<()> {
    save_image(
        processed,
        Path::new("/out/single.tiff"),
        OutputFormat::TIFF,
        BitDepth::U16,
        Some(2048),
        None,                  // Optional SAFE metadata if available
        true,
        AutoscaleStrategy::Tamed,
        ProcessingOperation::SingleBand,
    )
}

fn save_dual(vv: &Array2<Complex<f64>>, vh: &Array2<Complex<f64>>) -> sarpro::Result<()> {
    save_multiband_image(
        vv,
        vh,
        Path::new("/out/multiband.tiff"),
        OutputFormat::TIFF,
        BitDepth::U8,
        Some(1024),
        None,
        true,
        AutoscaleStrategy::Tamed,
        ProcessingOperation::MultibandVvVh,
    )
}
```

Batch helpers
-------------
```rust,no_run
use std::path::Path;
use sarpro::{
    process_directory_to_path,
    ProcessingParams, AutoscaleStrategy, BitDepthArg, OutputFormat, Polarization, InputFormat,
};

fn main() -> sarpro::Result<()> {
    let params = ProcessingParams {
        format: OutputFormat::JPEG,
        input_format: InputFormat::Safe,
        bit_depth: BitDepthArg::U8,
        polarization: Polarization::Multiband,
        autoscale: AutoscaleStrategy::Tamed,
        target_crs: Some("EPSG:32630".to_string()),
        resample_alg: Some("lanczos".to_string()),
        size: Some(1024),
        pad: true,
    };

    let report = process_directory_to_path(
        Path::new("/data/safe_root"),
        Path::new("/out"),
        &params,
        true, // continue_on_error
    )?;

    println!("processed={} skipped={} errors={}", report.processed, report.skipped, report.errors);
    Ok(())
}
```

Error handling
--------------
All public functions return `sarpro::Result<T>`; match on `sarpro::Error` to handle specific
cases, e.g. GDAL or SAFE reader errors.

```rust,no_run
use std::path::Path;
use sarpro::{process_safe_to_path, Error, ProcessingParams, AutoscaleStrategy, BitDepthArg, OutputFormat, Polarization, InputFormat};

fn main() {
    let params = ProcessingParams {
        format: OutputFormat::TIFF,
        input_format: InputFormat::Safe,
        bit_depth: BitDepthArg::U8,
        polarization: Polarization::Vv,
        autoscale: AutoscaleStrategy::Tamed,
        target_crs: Some("EPSG:32630".to_string()),
        resample_alg: Some("lanczos".to_string()),
        size: None,
        pad: false,
    };

    match process_safe_to_path(Path::new("/bad/path.SAFE"), Path::new("/out.tiff"), &params) {
        Ok(()) => {}
        Err(Error::Gdal(e)) => eprintln!("GDAL error: {e}"),
        Err(Error::Safe(e)) => eprintln!("SAFE error: {e}"),
        Err(other) => eprintln!("Other error: {other}"),
    }
}
```

Feature flags
-------------
- `gui`: builds the GUI crate module.
- `full`: enables a complete feature set for typical end-to-end workflows.

Useful modules
--------------
- [`api`] — high-level, ergonomic entry points.
- [`types`] — enums and core types (e.g. `AutoscaleStrategy`, `Polarization`, `ProcessingOperation`).
- [`io`] — SAFE and GDAL readers/writers.
- [`error`] — crate-level `Error` and `Result`.
"#]

// Core modules (public)
pub mod api;
pub mod core;
pub mod error;
pub mod io;
pub mod types;

// GUI module (only available with gui feature)
#[cfg(feature = "gui")]
pub mod gui;

// Curated public API surface
// Types
pub use core::params::ProcessingParams;
pub use error::{Error, Result};
pub use types::{
    AutoscaleStrategy, BitDepth, BitDepthArg, InputFormat, OutputFormat, Polarization,
    PolarizationOperation, ProcessingOperation,
};

// Readers
pub use io::gdal::{GdalError, GdalMetadata, GdalSarReader};
pub use io::sentinel1::{ProductType, SafeError, SafeMetadata, SafeReader};

// Selected writer helpers (keep low-level metadata helpers public)
pub use io::writers::metadata::{
    create_jpeg_metadata_sidecar, embed_tiff_metadata, extract_metadata_fields,
};

// High-level API re-exports
pub use api::{
    BatchReport, ProcessedImage, iterate_safe_products, load_operation, load_polarization,
    process_directory_to_path, process_safe_to_buffer, process_safe_to_path,
    process_safe_with_options, save_image, save_multiband_image,
};
