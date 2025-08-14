### Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and this project adheres to Semantic Versioning.

### [0.2.0] - Unreleased

- **Added**:
  - Map reprojection to any target CRS via GDAL. New CLI options: `--target_crs` (e.g., `EPSG:4326`, `EPSG:32633` or `none` to disable) and `--resample_alg` (`nearest`, `bilinear`, `cubic`).
  - Library API additions: `ProcessingParams` now includes `target_crs` and `resample_alg` fields; `SafeReader::open_with_options` for programmatic reprojection and resampling.
  - Resampling support (GDAL `ResampleAlg`): nearest-neighbour, bilinear, cubic.
  - World file and projection sidecars for JPEG outputs: `.jgw/.wld` and `.prj` written alongside images (see `src/io/writers/worldfile.rs`).
  - Metadata enhancements: JSON sidecar now includes `geotransform` and `crs`; TIFFs embed geotransform/projection when available with safe overrides.
  - New dependency: `tempfile` for safe ephemeral files during warps.

- **Changed**:
  - GDAL reader improvements: fallback to `gcp_projection()` when dataset projection is empty; avoid forcing horizontal image flip; more robust EPSG parsing and projection propagation.
  - Resize pipeline returns scale and padding metadata to correctly update geotransform after resizing and padding.
  - CLI wiring for reprojection/resampling; GUI updated to expose Target CRS and Resampling options.
  - Updated README with v0.2.0 capabilities and revised performance notes.

- **Fixed**:
  - Safeguards to skip previously warped intermediate files; improved handling of identity geotransforms before setting projection.

- **Removed**:
  - Replaced example asset filenames in `README.md` and `src/assets/` to reflect new outputs.

### [0.1.1] - 2025-08-13

- **Added**:
  - Roadmap section in `README.md`.

- **Changed**:
  - Default Cargo features now include `gui`.
  - GUI defaults to dark theme (`egui::ThemePreference::Dark`).

### [0.1.0] - 2025-08-09

- **Initial release**:
  - CLI, GUI, and library APIs for Sentinel-1 GRD processing.
  - Polarizations: VV, VH, HH, HV; multiband and common operations.
  - Autoscaling strategies: standard, robust, adaptive, equalized, tamed.
  - Output formats: GeoTIFF (u8/u16) and JPEG (grayscale, synthetic RGB).
  - Batch processing, logging, metadata extraction and sidecars.


