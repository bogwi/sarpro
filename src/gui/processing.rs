use super::logging::GuiLogLayer;
use super::models::{SarproGui, SizeMode};
use crate::gui::models::init_gui_logging;
use crate::io::sentinel1::SafeReader;
use crate::{AutoscaleStrategy, InputFormat, Polarization, PolarizationOperation};
use crate::{BitDepth, OutputFormat};
use num_complex::Complex;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;

/// GUI-specific errors
#[derive(Debug, Error)]
pub enum GuiError {
    #[error("Invalid size parameter: {size}. Must be a positive integer or 'original'")]
    InvalidSize { size: String },

    #[error("Size must be greater than 0, got: {size}")]
    ZeroSize { size: usize },

    #[error(
        "No complete polarization data available for operation: {operation}. Available: {available}"
    )]
    IncompleDataPair {
        operation: String,
        available: String,
    },

    #[error("Error creating output directory: {0}")]
    OutputDirError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SAFE reader error: {0}")]
    Safe(#[from] crate::io::sentinel1::SafeError),
}

impl SarproGui {
    pub fn select_input_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("SAFE files", &["safe"])
            .pick_folder()
        {
            self.input_path = Some(path);
            info!(
                "Selected input file: {:?}",
                self.input_path.as_ref().unwrap()
            );
            trace!("Input path set for single file processing");
        }
    }

    pub fn select_input_directory(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.input_dir_path = Some(path);
            info!(
                "Selected input directory: {:?}",
                self.input_dir_path.as_ref().unwrap()
            );
            trace!("Input directory set for batch processing");
        }
    }

    fn path_without_extension(path: &PathBuf) -> PathBuf {
        if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
            if let Some(index) = file_name.find('.') {
                let prefix = &file_name[..index];
                if let Some(parent) = path.parent() {
                    return parent.join(prefix);
                } else {
                    return PathBuf::from(prefix);
                }
            }
        }
        path.to_path_buf()
    }

    pub fn select_output_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Image files", &["tiff", "jpg", "jpeg"])
            .save_file()
        {
            // Strip any extension from the user-selected path
            // The extension will be controlled by the format setting
            let path_without_extension = Self::path_without_extension(&path);

            self.output_path = Some(path_without_extension);
            self.update_output_path_extension();
            info!(
                "Selected output file: {:?}",
                self.output_path.as_ref().unwrap()
            );
            trace!("Output path configured for single file processing");
        }
    }

    pub fn select_output_directory(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.output_dir_path = Some(path);
            info!(
                "Selected output directory: {:?}",
                self.output_dir_path.as_ref().unwrap()
            );
            trace!("Output directory configured for batch processing");
        }
    }

    /// Update the output path extension based on the current format setting
    pub fn update_output_path_extension(&mut self) {
        if let Some(output_path) = &self.output_path {
            // Get the path without extension
            let path_without_extension = Self::path_without_extension(&output_path);

            // Add the correct extension based on format
            let extension = match self.output_format {
                OutputFormat::TIFF => "tiff",
                OutputFormat::JPEG => "jpg",
            };

            let new_path = path_without_extension.with_extension(extension);
            self.output_path = Some(new_path);
            debug!("Updated output path extension to: {}", extension);
        }
    }

    pub fn get_size_string(&self) -> String {
        match self.size_mode {
            SizeMode::Original => "original".to_string(),
            SizeMode::Predefined(size) => size.to_string(),
            SizeMode::Custom => self.custom_size.clone(),
        }
    }

    pub fn process_single_file(
        &self,
        input: &PathBuf,
        output: &PathBuf,
        format: OutputFormat,
        bit_depth: BitDepth,
        input_format: InputFormat,
        polarization: Polarization,
        autoscale: AutoscaleStrategy,
        size: &str,
        batch_mode: bool,
        pad: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        trace!("Starting single file processing");
        trace!("Input: {:?}, Output: {:?}", input, output);
        trace!(
            "Format: {:?}, Bit depth: {:?}, Polarization: {:?}",
            format, bit_depth, polarization
        );
        trace!(
            "Size: {}, Batch mode: {}, Padding: {}",
            size, batch_mode, pad
        );

        // Autoscale strategy now passed as typed enum directly
        trace!("Autoscale strategy: {:?}", autoscale);

        // Convert polarization enum to string for SafeReader
        let polarization_str = match polarization {
            Polarization::Vv => None, // Default
            Polarization::Vh => Some("vh"),
            Polarization::Hh => Some("hh"),
            Polarization::Hv => Some("hv"),
            // For multiband and operations, we'll load all available pairs and let the processing logic decide
            Polarization::Multiband | Polarization::OP(_) => Some("all_pairs"),
        };
        debug!("Polarization string: {:?}", polarization_str);

        // Parse size parameter
        let target_size = if size == "original" {
            None
        } else {
            let parsed_size = size.parse::<usize>().map_err(|_| {
                format!(
                    "Invalid size parameter: {}. Must be a positive integer or 'original'",
                    size
                )
            })?;

            if parsed_size == 0 {
                return Err(GuiError::ZeroSize { size: parsed_size }.into());
            }

            Some(parsed_size)
        };
        debug!("Target size: {:?}", target_size);

        // Open input based on format
        let reader = if batch_mode {
            match input_format {
                InputFormat::Safe => {
                    trace!("Opening SAFE file in batch mode: {:?}", input);
                    // In batch mode, honor target CRS and resample algorithm similarly to single-file mode
                    let resample = match self.resample_alg.trim().to_lowercase().as_str() {
                        "nearest" => Some(gdal::raster::ResampleAlg::NearestNeighbour),
                        "cubic" => Some(gdal::raster::ResampleAlg::Cubic),
                        "lanczos" => Some(gdal::raster::ResampleAlg::Lanczos),
                        _ => Some(gdal::raster::ResampleAlg::Bilinear),
                    };
                    let trimmed = self.target_crs.trim();
                    let tgt_opt = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
                        None
                    } else {
                        Some(trimmed)
                    };
                    match SafeReader::open_with_warnings_with_options(
                        input,
                        polarization_str,
                        tgt_opt,
                        resample,
                        target_size,
                    )? {
                        Some(reader) => {
                            debug!("Successfully opened SAFE file in batch mode");
                            reader
                        }
                        None => {
                            warn!("Skipping unsupported product type: {:?}", input);
                            return Ok(()); // Exit successfully but skip this file
                        }
                    }
                }
            }
        } else {
            match input_format {
                InputFormat::Safe => {
                    trace!("Opening SAFE file in single mode: {:?}", input);
                    // Map resample algorithm string to GDAL enum
                    let resample = match self.resample_alg.trim().to_lowercase().as_str() {
                        "nearest" => Some(gdal::raster::ResampleAlg::NearestNeighbour),
                        "cubic" => Some(gdal::raster::ResampleAlg::Cubic),
                        _ => Some(gdal::raster::ResampleAlg::Bilinear),
                    };
                    // Use target CRS if provided; treat "none" (case-insensitive) or blank as no reprojection
                    let trimmed = self.target_crs.trim().to_string();
                    let tgt_opt = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
                        None
                    } else {
                        Some(trimmed.as_str())
                    };
                    let reader = SafeReader::open_with_options(
                        input,
                        polarization_str,
                        tgt_opt,
                        resample,
                        target_size,
                    )?;
                    debug!("Successfully opened SAFE file in single mode");
                    reader
                }
            }
        };

        // Refactored polarization handling to reduce repetition
        match polarization {
            Polarization::Vv | Polarization::Vh | Polarization::Hh | Polarization::Hv => {
                let processed = match polarization {
                    Polarization::Vv => reader.vv_data()?,
                    Polarization::Vh => reader.vh_data()?,
                    Polarization::Hh => reader.hh_data()?,
                    Polarization::Hv => reader.hv_data()?,
                    _ => unreachable!(),
                };

                let bytes = processed.len() * std::mem::size_of::<Complex<f64>>();
                info!(
                    "Memory usage (approx): {:.2} MB",
                    bytes as f64 / 1024.0 / 1024.0
                );

                trace!("Saving processed image to: {:?}", output);
                crate::core::processing::save::save_processed_image(
                    &processed,
                    output.as_path(),
                    format,
                    bit_depth,
                    target_size,
                    Some(reader.metadata()),
                    pad,
                    autoscale,
                    crate::types::ProcessingOperation::SingleBand,
                )
            }
            Polarization::Multiband => {
                // Support both VV/VH and HH/HV pairs for multiband
                if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                    // Use VV/VH pair
                    trace!("Processing multiband raster (VV + VH)");
                    let vv_processed = reader.vv_data()?;
                    let vh_processed = reader.vh_data()?;
                    debug!(
                        "VV data loaded, dimensions: {}x{}",
                        vv_processed.nrows(),
                        vv_processed.ncols()
                    );
                    debug!(
                        "VH data loaded, dimensions: {}x{}",
                        vh_processed.nrows(),
                        vh_processed.ncols()
                    );

                    let total_bytes = (vv_processed.len() + vh_processed.len())
                        * std::mem::size_of::<num_complex::Complex<f64>>();
                    info!(
                        "Memory usage (Multiband VV/VH): {:.2} MB",
                        total_bytes as f64 / 1024.0 / 1024.0
                    );

                    trace!("Saving multiband processed image to: {:?}", output);
                    crate::core::processing::save::save_processed_multiband_image_sequential(
                        &vv_processed,
                        &vh_processed,
                        output.as_path(),
                        format,
                        bit_depth,
                        target_size,
                        Some(reader.metadata()),
                        pad,
                        autoscale,
                        crate::types::ProcessingOperation::MultibandVvVh,
                    )
                } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                    // Use HH/HV pair
                    trace!("Processing multiband raster (HH + HV)");
                    let hh_processed = reader.hh_data()?;
                    let hv_processed = reader.hv_data()?;
                    debug!(
                        "HH data loaded, dimensions: {}x{}",
                        hh_processed.nrows(),
                        hh_processed.ncols()
                    );
                    debug!(
                        "HV data loaded, dimensions: {}x{}",
                        hv_processed.nrows(),
                        hv_processed.ncols()
                    );

                    let total_bytes = (hh_processed.len() + hv_processed.len())
                        * std::mem::size_of::<num_complex::Complex<f64>>();
                    info!(
                        "Memory usage (Multiband HH/HV): {:.2} MB",
                        total_bytes as f64 / 1024.0 / 1024.0
                    );

                    trace!("Saving multiband processed image to: {:?}", output);
                    crate::core::processing::save::save_processed_multiband_image_sequential(
                        &hh_processed,
                        &hv_processed,
                        output.as_path(),
                        format,
                        bit_depth,
                        target_size,
                        Some(reader.metadata()),
                        pad,
                        autoscale,
                        crate::types::ProcessingOperation::MultibandHhHv,
                    )
                } else {
                    let available = reader.get_available_polarizations();
                    return Err(GuiError::IncompleDataPair {
                        operation: "multiband".to_string(),
                        available,
                    }
                    .into());
                }
            }
            Polarization::OP(_) => {
                // Handle new polarization operations - support both VV/VH and HH/HV pairs
                let processed = if reader.vv_data().is_ok() && reader.vh_data().is_ok() {
                    // Use VV/VH pair
                    match polarization {
                        Polarization::OP(PolarizationOperation::Sum) => reader.sum_data()?,
                        Polarization::OP(PolarizationOperation::Diff) => {
                            reader.difference_data()?
                        }
                        Polarization::OP(PolarizationOperation::Ratio) => reader.ratio_data()?,
                        Polarization::OP(PolarizationOperation::NDiff) => {
                            reader.normalized_diff_data()?
                        }
                        Polarization::OP(PolarizationOperation::LogRatio) => {
                            reader.log_ratio_data()?
                        }
                        _ => unreachable!(),
                    }
                } else if reader.hh_data().is_ok() && reader.hv_data().is_ok() {
                    // Use HH/HV pair
                    match polarization {
                        Polarization::OP(PolarizationOperation::Sum) => reader.sum_hh_hv_data()?,
                        Polarization::OP(PolarizationOperation::Diff) => {
                            reader.difference_hh_hv_data()?
                        }
                        Polarization::OP(PolarizationOperation::Ratio) => {
                            reader.ratio_hh_hv_data()?
                        }
                        Polarization::OP(PolarizationOperation::NDiff) => {
                            reader.normalized_diff_hh_hv_data()?
                        }
                        Polarization::OP(PolarizationOperation::LogRatio) => {
                            reader.log_ratio_hh_hv_data()?
                        }
                        _ => unreachable!(),
                    }
                } else {
                    let available = reader.get_available_polarizations();
                    return Err(GuiError::IncompleDataPair {
                        operation: polarization.to_string(),
                        available,
                    }
                    .into());
                };

                let bytes = processed.len() * std::mem::size_of::<Complex<f64>>();
                info!(
                    "Memory usage (approx): {:.2} MB",
                    bytes as f64 / 1024.0 / 1024.0
                );

                trace!("Saving processed image to: {:?}", output);
                crate::core::processing::save::save_processed_image(
                    &processed,
                    output.as_path(),
                    format,
                    bit_depth,
                    target_size,
                    Some(reader.metadata()),
                    pad,
                    autoscale,
                    crate::types::ProcessingOperation::PolarOp(match polarization {
                        Polarization::OP(op) => op,
                        _ => unreachable!(),
                    }),
                )
            }
        }
    }

    pub fn process_files(&mut self) {
        if self.is_processing {
            debug!("Processing already in progress, ignoring request");
            return;
        }

        trace!("Starting file processing");
        self.is_processing = true;
        self.processing_start_time = Some(Instant::now());
        self.last_processing_duration = None;

        // Always initialize logging for error messages to appear in GUI
        init_gui_logging();
        info!("Processing started");

        // Clone all necessary parameters for the background thread
        let input_path = self.input_path.clone();
        let input_dir_path = self.input_dir_path.clone();
        let output_path = self.output_path.clone();
        let output_dir_path = self.output_dir_path.clone();
        let output_format = self.output_format;
        let input_format = self.input_format;
        let bit_depth = self.bit_depth;
        let polarization = self.polarization;
        let autoscale = self.autoscale; // Pass the actual autoscale strategy
        let size_mode = self.size_mode; // <-- FIX: clone actual size_mode
        let custom_size = self.custom_size.clone(); // <-- FIX: clone actual custom_size
        let batch_mode = self.batch_mode;
        let pad = self.pad;
        let log_enabled = self.enable_logging;
        let log_messages = self.log_messages.clone();
        let (tx, rx) = std::sync::mpsc::channel();

        debug!("Background processing parameters:");
        debug!("  Batch mode: {}", batch_mode);
        debug!("  Output format: {:?}", output_format);
        debug!("  Bit depth: {:?}", bit_depth);
        debug!("  Polarization: {:?}", polarization);
        debug!("  Autoscale strategy: {:?}", autoscale);
        debug!("  Size mode: {:?}", size_mode);
        debug!("  Padding: {}", pad);

        // Spawn background thread for processing
        let target_crs = self.target_crs.clone();
        let resample_alg = self.resample_alg.clone();
        std::thread::spawn(move || {
            // Always set up tracing subscriber for this thread so error messages appear in GUI
            let subscriber = Registry::default().with(GuiLogLayer::new());
            let _ = tracing::subscriber::set_global_default(subscriber);
            // Ignore error if already set
            // Create a twin SarproGui for calling process_files_inner
            let mut twin_gui = SarproGui {
                input_path,
                input_dir_path,
                output_path,
                output_dir_path,
                output_format,
                input_format,
                bit_depth,
                polarization,
                autoscale, // Use the actual autoscale strategy from GUI
                target_crs,
                resample_alg,
                size_mode,   // <-- FIX: use actual size_mode
                custom_size, // <-- FIX: use actual custom_size
                enable_logging: log_enabled,
                batch_mode,
                pad,
                min_log_level: tracing::Level::INFO, // Default to INFO level
                status_message: String::new(),
                is_processing: true,
                processing_start_time: None,
                last_processing_duration: None,
                log_messages,
                completion_receiver: None, // Initialize the new field
                cpu_usage: 0.0,
                memory_usage_mb: 0.0,
                total_memory_mb: 0.0,
                system_monitor: None,
                last_system_update: None,
            };
            trace!("Background processing thread started");
            let result = twin_gui.process_files_inner();
            let msg = match result {
                Ok(m) => m,
                Err(e) => {
                    error!("Processing cancelled: {}", e);
                    format!("Error: {}", e)
                }
            };
            let _ = tx.send(msg);
        });

        // Store the receiver for completion notification
        self.completion_receiver = Some(rx);
        info!("Processing started in background thread");
    }

    // The actual processing logic, moved from process_files
    pub fn process_files_inner(&mut self) -> Result<String, String> {
        // Determine if we're in batch mode
        let batch_mode = self.batch_mode && self.input_dir_path.is_some();
        debug!(
            "Processing mode: {}",
            if batch_mode { "Batch" } else { "Single file" }
        );

        if batch_mode {
            // Batch processing mode
            if let (Some(input_dir), Some(output_dir)) =
                (&self.input_dir_path, &self.output_dir_path)
            {
                trace!("Starting batch processing");
                trace!("Input directory: {:?}", input_dir);
                trace!("Output directory: {:?}", output_dir);

                // Add processing separator after validation passes
                let separator = crate::gui::logging::LogEntry::new(
                    tracing::Level::INFO,
                    "--- Processing Started ---".to_string(),
                    "gui".to_string(),
                );
                if let Ok(mut logs) = self.log_messages.lock() {
                    logs.push(separator);
                }
                // Create output directory if it doesn't exist
                if let Err(e) = fs::create_dir_all(output_dir) {
                    error!("Error creating output directory: {}", e);
                    return Err(GuiError::OutputDirError(e.to_string()).to_string());
                }
                info!("Starting batch processing from directory: {:?}", input_dir);
                info!("Output directory: {:?}", output_dir);
                let mut processed = 0;
                let mut skipped = 0;
                let mut errors = 0;
                // Process all subdirectories in the input directory
                match fs::read_dir(input_dir) {
                    Ok(entries) => {
                        debug!("Successfully opened input directory for reading");
                        trace!("Scanning directory entries for SAFE folders");
                        for entry in entries {
                            match entry {
                                Ok(entry) => {
                                    let path = entry.path();
                                    if path.is_dir() {
                                        let safe_name = path.file_name().unwrap().to_string_lossy();
                                        let output_name = format!(
                                            "{}.{}",
                                            safe_name,
                                            match self.output_format {
                                                OutputFormat::TIFF => "tiff",
                                                OutputFormat::JPEG => "jpg",
                                            }
                                        );
                                        let output_path = output_dir.join(&output_name);
                                        info!("Processing: {:?} -> {:?}", path, output_path);
                                        trace!("Processing SAFE directory: {}", safe_name);
                                        match self.process_single_file(
                                            &path,
                                            &output_path,
                                            self.output_format,
                                            self.bit_depth,
                                            self.input_format,
                                            self.polarization,
                                            self.autoscale, // Pass the autoscale strategy
                                            self.get_size_string().as_str(),
                                            true,
                                            self.pad,
                                        ) {
                                            Ok(()) => {
                                                info!("Successfully processed: {:?}\n", path);
                                                processed += 1;
                                            }
                                            Err(e) => {
                                                warn!("Error processing {:?}: {}", path, e);
                                                errors += 1;
                                            }
                                        }
                                    } else {
                                        debug!("Skipping non-directory: {:?}", path);
                                        skipped += 1;
                                    }
                                }
                                Err(e) => {
                                    warn!("Error reading directory entry: {}", e);
                                    errors += 1;
                                }
                            }
                        }
                        // Print summary
                        info!("Batch processing complete!");
                        info!("Processed: {}", processed);
                        info!("Skipped: {}", skipped);
                        info!("Errors: {}", errors);
                        Ok(format!(
                            "Batch processing complete! Processed: {}, Skipped: {}, Errors: {}",
                            processed, skipped, errors
                        ))
                    }
                    Err(e) => {
                        error!("Error reading input directory: {}", e);
                        Err(format!("Error reading input directory: {}", e))
                    }
                }
            } else {
                error!("Input and output directories required for batch processing");
                Err("Input and output directories required".to_string())
            }
        } else {
            // Single file mode
            if let (Some(input), Some(output)) = (&self.input_path, &self.output_path) {
                trace!("Starting single file processing");
                trace!("Input file: {:?}", input);
                trace!("Output file: {:?}", output);

                // Add processing separator after validation passes
                let separator = crate::gui::logging::LogEntry::new(
                    tracing::Level::INFO,
                    "--- Processing Started ---".to_string(),
                    "gui".to_string(),
                );
                if let Ok(mut logs) = self.log_messages.lock() {
                    logs.push(separator);
                }
                match self.process_single_file(
                    input,
                    output,
                    self.output_format,
                    self.bit_depth,
                    self.input_format,
                    self.polarization,
                    self.autoscale, // Pass the autoscale strategy
                    self.get_size_string().as_str(),
                    false,
                    self.pad,
                ) {
                    Ok(()) => {
                        info!("Successfully processed: {:?} -> {:?}\n", input, output);
                        Ok(format!(
                            "Successfully processed: {:?} -> {:?}\n",
                            input, output
                        ))
                    }
                    Err(e) => {
                        error!("Error processing file: {}", e);
                        Err(format!("Error processing file: {}", e))
                    }
                }
            } else {
                error!("Input and output files required for single file processing");
                Err("Input and output files required".to_string())
            }
        }
    }
}
