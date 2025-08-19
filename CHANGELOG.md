### Changelog

### [0.2.12] - 2025-08-19 (Unpublished)

- **Added**:
  - Auto target CRS (UTM/UPS) via `--target-crs auto` resolves exactly once per SAFE product during open and is reused for all band loads.
    - Candidate measurement is selected from the SAFE `measurement/` folder (prefers VV/VH, else HH/HV, else first TIFF).
    - Logs: INFO for candidate and resolved EPSG; WARN on failure (falls back to no warp).

- **Changed**:
  - CLI/GUI/API continue to accept `--target-crs auto | none | EPSG:nnnn`; no changes to user workflows.

- **Performance**:
  - Avoids an extra dataset open when using `auto`, reducing I/O latency notably in batch processing.

- **Compatibility**:
  - User-facing behavior is unchanged; existing commands and presets continue to work.

### [0.2.11] - 2025-08-18 (Unpublished)

- **Added**:
  - New autoscale strategy: CLAHE (Contrast Limited Adaptive Histogram Equalization).
    - Implementation in `src/core/processing/autoscale.rs`: `clahe_equalize_normalized` with tile-wise histograms (default 8×8 tiles, 256 bins), contrast clipping (`clip_limit = 2.0`), and bilinear interpolation across tiles. Respects the `valid_mask`.
    - Strategy wiring: `AutoscaleStrategy::Clahe` in `src/types.rs`; pipeline routes through advanced autoscale; CLI exposes `--autoscale clahe`; GUI adds a “CLAHE” option and description.

- **Changed**:
  - CLAHE path normalizes dB values to 0..1 using a robust window `[p01, p99]` prior to local equalization to limit outlier influence.
  - CLI help text now lists `clahe` among autoscale strategies.
  - Adaptive strategy: disabled non-physical local enhancement in dB; now uses only robust percentiles + gamma. For local contrast, use `CLAHE`. GUI description updated accordingly.
  - Tamed for synRGB: implemented band-specific percentile mapping before synRGB LUTs — co-pol lower cut near p02..p05, cross-pol near p05; avoids universal p25. GUI hint updated.
  - Default resample algorithm: `lanczos` for all operations.
  - Default autoscale strategy: `CLAHE`.

- **Performance**:
  - CLAHE is heavier than Standard/Robust percentile stretches but remains near-linear in the number of pixels with small per-tile histograms. Memory use is modest (per-tile 256-bin CDFs). Suitable for interactive quicklooks.

- **Compatibility**:
  - BREAKING (library API): Adding a new enum variant may break exhaustive `match` statements over `AutoscaleStrategy` in downstream code. CLI/GUI remain backward compatible; defaults unchanged.

### [0.2.10] - 2025-08-18 (Unpublished)

- **Changed**:
  - Implemented optimization Step 8 (Synthetic RGB `powf` hot loop → LUT) in `core/processing/synthetic_rgb.rs::create_synthetic_rgb`.
    - Replaced per‑pixel `powf` on 0..255 inputs with precomputed lookup tables:
      - 256‑entry LUT for Red (gamma 0.7)
      - 256‑entry LUT for Green (gamma 0.9)
      - 65,536‑entry LUT for Blue derived from the gamma‑mapped R/G ratio raised to 0.1, preserving the original guard for `band2 == 0` and visual scaling factor
    - Call sites remain unchanged; function signature and outputs are identical.
  - Implemented optimization Padding-row-copies in `core/processing/padding.rs::add_padding_to_square`.
    - Replaced double nested per‑pixel loops with per‑row `copy_from_slice` for both `u8` and `u16` paths.
    - Compute a single destination offset per row; corrected final dimensions log to `max_dim x max_dim`.
  - Implemented optimization dB conversion + mask single pass in `core/processing/pipeline.rs`.
    - `process_scalar_data_inplace`: single pass over contiguous slice (when available) to compute dB and validity mask; fallback to indexed iteration maintained for non‑contiguous cases.
    - `process_scalar_data_pipeline`: removed materialization of `valid_db` vector.
    - Dropped `valid_db` parameter from autoscale functions and updated call sites:
      - `autoscale_db_image_to_bitdepth(db, valid_mask, bit_depth)`
      - `autoscale_db_image(db, valid_mask, bit_depth)`
      - `autoscale_db_image_to_bitdepth_advanced(db, valid_mask, bit_depth, strategy)`
      - `autoscale_db_image_advanced(db, valid_mask, bit_depth, strategy)`

- **Performance**:
  - Eliminates scalar `powf` in the tight pixel loop; the synRGB stage is now memory‑bound on typical scenes.
  - One‑time LUT build per call (~66 KiB) is negligible compared to per‑pixel exponentiation on megapixel inputs.
  - Padding stage now performs contiguous row copies, yielding a minor but free ~2–3× speedup for this step.
  - dB+mask stage avoids an extra scan and reduces bounds checks on contiguous arrays; removes the secondary `valid_db` allocation and its cache traffic.

- **Compatibility**:
  - BREAKING (library API): removed the `valid_db` slice parameter from autoscale helpers listed above. Typical CLI/GUI flows are unaffected; consumers of the library API must stop passing this argument. Visual behavior remains unchanged.

### [0.2.9] - 2025-08-18 (Unpublished)

- **Changed**:
  - Implemented optimization Step 7 (U16 resize path) in `core/processing/resize.rs`.
  - `resize_u16_image` now resizes true 16‑bit data directly using `fast_image_resize` with `PixelType::U16`.
    The crate stores image data in a `Vec<u8>`; we pack the `u16` slice into little‑endian bytes before resize
    and reconstruct `u16` after. This is a real 16‑bit pipeline, not an emulation.
  - Updated log message to reflect that U16 is resized without down‑conversion.

- Why this works:
  - `fast_image_resize` interprets the raw `u8` buffer according to `PixelType`. With `PixelType::U16`, every
    two bytes form one sample. On our little‑endian targets (x86_64/ARM64), `to_le_bytes`/`from_le_bytes` preserves
    numeric values. On a big‑endian target, `to_be_bytes`/`from_be_bytes` would be used instead.

- **Performance**:
  - Removes the previous U16→U8→U16 round‑trip (and its extra passes/allocations), reduces memory traffic, avoids
    quantization losses, and lets the resizer operate on 16‑bit samples (better SIMD/cache behavior). Net effect:
    faster U16 resize with equal or better quality.

- **Compatibility**:
  - No public API changes. Behavior is equivalent, with improved precision on the U16 path because down‑/up‑conversion
    is avoided.

### [0.2.8] - 2025-08-18 (Unpublished)

- **Changed**:
  - Implemented optimization Step 6 (stop cloning large arrays on getters).
  - `SafeReader::{vv_data,vh_data,hh_data,hv_data}` now return borrowed `&Array2<f32>` instead of owned `Array2<f32>`.
  - `SafeReader::data()` now returns `&Array2<f32>`.
  - Updated API/GUI/CLI call sites to use borrowed references and avoid redundant clones; clone only where an owned buffer is explicitly required (e.g., `api::load_polarization` still returns an owned array and performs a single clone at the boundary).

- **Performance**:
  - Eliminates extra multi‑hundred‑MB clones during processing; reduces peak memory and transient allocator pressure.

- **Compatibility**:
  - BREAKING (library API): code calling `SafeReader` getters must adapt from owned returns to borrowed references. Downstream functions like `save_processed_image` and processing ops already accept borrows, so most changes are mechanical.

### [0.2.7] - 2025-08-18 (unpublished)

- **Changed**:
  - Adaptive local contrast enhancement in `core/processing/autoscale.rs::autoscale_db_image_advanced` is now allocation‑free for the 3×3 window.
    - Replaced per‑pixel `Vec` allocation and full sort with a fixed `[f64; 9]` buffer and in‑place insertion sort.
    - Added helpers: `local_median_and_range_3x3` and `insertion_sort_in_place` to compute local median/range without heap allocations.
  - Excess precision and type choices (Step 5): switched GRD processing from `Complex<f64>` to scalar `f32` intensities across the pipeline.
    - Readers now return `Array2<f32>`; GDAL reads in `f32`.
    - Processing ops (`sum/diff/ratio/n-diff/log-ratio`) operate on scalar intensities.
    - Pipeline renamed to `process_scalar_data_*`; save paths and API functions updated to accept `Array2<f32>`.
    - CLI/GUI unchanged for users; internal memory tracing updated.

- **Performance**:
  - Adaptive autoscale with local enhancement runs about ~2× faster on typical scenes and removes allocator pressure (constant small stack buffer), improving overall responsiveness.
  - End‑to‑end improvements on original/full‑resolution processing: ~2× lower memory footprint (drop `Complex`), additional ~2× from `f64`→`f32` where bandwidth‑bound; noticeable wall‑clock gains in I/O‑light runs.

- **Compatibility**:
  - BREAKING (library API): public functions that previously accepted/returned `Array2<Complex<f64>>` now use `Array2<f32>` for GRD intensity processing (e.g., `save_image`, `save_multiband_image`, `process_safe_to_buffer`, `load_polarization`, `load_operation`). This crate is pre‑1.0 and documented as experimental/evolving.
  - Visual/quantitative behavior: unchanged for GRD use‑cases; `f32` precision is standard for SAR intensities and differences are far below speckle/noise and autoscale bin widths.
  - Roadmap alignment: GRD phases (masking, speckle filters, RTC normalization, tiling, time‑series intensity change) are intensity‑based and unaffected. Future SLC/phase workflows can add parallel complex paths without impacting the GRD pipeline.

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


