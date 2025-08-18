### Changelog

### [0.2.7] - 2025-08-18

- **Changed**:
  - Adaptive local contrast enhancement in `core/processing/autoscale.rs::autoscale_db_image_advanced` is now allocation‑free for the 3×3 window.
    - Replaced per‑pixel `Vec` allocation and full sort with a fixed `[f64; 9]` buffer and in‑place insertion sort.
    - Added helpers: `local_median_and_range_3x3` and `insertion_sort_in_place` to compute local median/range without heap allocations.

- **Performance**:
  - Adaptive autoscale with local enhancement runs about ~2× faster on typical scenes and removes allocator pressure (constant small stack buffer), improving overall responsiveness.

- **Compatibility**:
  - No public API changes. Visual behavior equivalent with improved local contrast stability.

### [0.2.6] - 2025-08-16

- **Updated**:
  - `README.md` for clarity and consistency.

- **Notes**:
  - This is a documentation update only. No functional code changes vs 0.2.4.

### [0.2.5] - 2025-08-16

- **Added**:
  - New public roadmap: `ROADMAP.md` with phased plan and tentative release targets.
  - New long‑form developer guide: `ROADMAP_explained.md` — deep technical explanation of each phase with diagrams and implementation notes.
  - Extended README “Best Usage Practices” with “What SARPRO Can Do Now” — concrete, real‑world tasks supported in v0.2.4 for GRD.

- **Notes**:
  - This is a documentation/roadmap update only. No functional code changes vs 0.2.4.

### [0.2.4] - 2025-08-15

- **Changed**:
  - Replaced autoscale percentile computation with a streaming histogram approach (4096 bins) in `core/processing/autoscale.rs`.
    - Eliminates O(N log N) full-sort over valid pixels; now O(N) two-pass (stats + histogram) with negligible quantization error (≈0.02 dB typical spans).
    - Removes materialization of the `values: Vec<f64>` for all valid pixels; memory usage now ~constant (histogram ≈32 KB).
    - Adds `p10`/`p90` support and wires them into the Adaptive strategy; percentiles now derived from histogram CDF with intra-bin interpolation.
    - Returns `valid_count` from stats to avoid redundant counting.

- **Performance**:
  - End-to-end CPU reduction on large scenes (multi‑hundreds of MP) due to removing sorting and large allocations. Observed ~10% improvement in local tests; larger gains expected on very large inputs.

- **Compatibility**:
  - Public API unchanged; behavior is equivalent with minor percentile estimation differences well below visual significance.

- **Fixed**:
  - Reprojection in batch mode: `--target-crs` and `--resample-alg` were ignored when `--batch` (and in GUI batch). Now both CLI (`src/cli/runner.rs`) and GUI (`src/gui/processing.rs`) honor target CRS and resampling in batch processing by passing them into `SafeReader::open_with_warnings_with_options(...)`.
  - GUI command generation: `generate_cli_command()` now respects the active mode and avoids mixing `--input/--output` with `--input-dir/--output-dir`. Also prefixes the command with `cargo run --release --bin sarpro --` to allow direct copy-paste execution during development.

### [0.2.3] - 2025-08-15 00:21 JST

- **Added**:
  - Exposed Lanczos resampling as a first-class option in both CLI (`--resample-alg lanczos`) and GUI. Recommended for moderate downscales when maximum sharpness is desired.

- **Changed**:
  - Improved visual quality for native (no-warp) downsamples: automatically choose a higher-quality GDAL resampler when shrinking aggressively (Average for ≥4× reduction; otherwise Lanczos) unless the user explicitly sets `--resample-alg`.

- **Performance**:
  - Maintains the fast pipeline introduced in 0.2.1–0.2.2 while restoring visually clean native outputs; no measurable regression in end-to-end timing.

- **Release notes**:
  - 0.2.1 and 0.2.2 remain marked as unreleased internal optimization milestones. 0.2.3 rolls up user-visible improvements and is released.

### [0.2.2] - 2025-08-14 (unreleased)

- **Changed**:
  - Reprojection now writes a lightweight VRT instead of a temporary GeoTIFF. We invoke `gdalwarp` with `-of VRT` and open the VRT directly, eliminating large intermediate files and further reducing disk I/O and latency.
  - Kept single-pass resampling by continuing to pass `-ts <cols> <rows>` when a target size is provided.
  - Tuned warp performance flags: `-multi -wo NUM_THREADS=ALL_CPUS -wm 512 --config GDAL_CACHEMAX 512`.

- **Performance**:
  - Noticeably lower I/O during reprojection; faster end-to-end with `--target-crs` due to removing the large temporary GeoTIFF and reading via VRT.

- **Compatibility**:
  - No API changes. Behavior identical except for improved performance and reduced temporary disk usage.

### [0.2.1] - 2025-08-14 (unreleased)

- **Added**:
  - Downsampled reading path for SAFE measurements: the reader now supports supplying a target long-side size to read bands directly at the requested output resolution.
  - New `GdalSarReader::read_band_resampled(out_cols, out_rows, alg)` to leverage GDAL’s resampling during read.
  - `SafeReader::open_with_options(..., target_size: Option<usize>)` and `open_with_warnings_with_options` to plumb target size from CLI/GUI/Library.

- **Changed**:
  - When reprojection is requested, `gdalwarp` is invoked with `-ts <cols> <rows>` derived from the target size, producing the final resolution in a single step (no intermediate full-res files, no double resampling).
  - Non-warp reads call `read_band_resampled` to avoid loading full-resolution arrays when the output is smaller.
  - `resize_image_data_with_meta` now early-returns when the current long side already matches the requested target size to prevent redundant resizes.

- **Performance**:
  - End-to-end speedups up to ~10x for small target sizes (e.g., 512), primarily by cutting disk I/O and memory traffic and avoiding extra resampling passes.
  - Lower peak memory usage when processing very large scenes.

- **Compatibility**:
  - API remains source-compatible; added optional `target_size` parameter to existing open functions without breaking previous call sites. CLI/GUI and Library routes forward `--size`/`size` to the reader.

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


