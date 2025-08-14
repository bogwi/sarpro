use super::models::{SarproGui, SizeMode};
use crate::{BitDepth, OutputFormat};
use crate::{AutoscaleStrategy, Polarization, PolarizationOperation};
use eframe::egui::{Align, Color32, ComboBox, Frame, Layout, RichText, Ui};

const COMPONENT_HEIGHT: f32 = 80.0;
const COMPONENT_WIDTH: f32 = 120.0;

pub struct ModeSelectionComponent;

impl ModeSelectionComponent {
    pub fn render(ui: &mut Ui, app: &mut SarproGui) {
        ui.heading("Processing Mode");

        Frame::NONE.inner_margin(0.0).show(ui, |ui| {
            ui.set_min_height(COMPONENT_HEIGHT * 0.6); // Set desired height
            ui.set_min_width(COMPONENT_WIDTH);
            ui.horizontal(|ui| {
                ui.radio_value(&mut app.batch_mode, false, "Single File");
                ui.radio_value(&mut app.batch_mode, true, "Batch Processing");
            });

            // Show informational text only when Batch Processing is selected
            if app.batch_mode {
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label("Will skip unsupported products");
                });
            }
        });
    }
}

pub struct FileSelectionComponent;

impl FileSelectionComponent {
    pub fn render_single_file(ui: &mut Ui, app: &mut SarproGui) {
        ui.heading("File Selection");

        Frame::NONE.inner_margin(0.0).show(ui, |ui| {
            ui.set_min_height(COMPONENT_HEIGHT);
            ui.set_min_width(COMPONENT_WIDTH);

            ui.horizontal(|ui| {
                ui.label("Input SAFE Directory:");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Browse").clicked() {
                        app.select_input_file();
                    }
                });
            });

            if let Some(path) = &app.input_path {
                ui.label(
                    RichText::new(path.to_string_lossy()).color(Color32::from_rgb(255, 165, 0)),
                );
            } else {
                ui.label(RichText::new("None selected").color(Color32::from_gray(120)));
            }

            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("Output File:");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Browse").clicked() {
                        app.select_output_file();
                    }
                });
            });

            if let Some(path) = &app.output_path {
                ui.label(
                    RichText::new(path.to_string_lossy()).color(Color32::from_rgb(255, 165, 0)),
                );
            } else {
                ui.label(RichText::new("None selected").color(Color32::from_gray(120)));
            }
        });
    }

    pub fn render_batch_mode(ui: &mut Ui, app: &mut SarproGui) {
        ui.heading("Batch Processing");

        Frame::NONE.inner_margin(0.0).show(ui, |ui| {
            ui.set_min_height(COMPONENT_HEIGHT);
            ui.set_min_width(COMPONENT_WIDTH);

            ui.horizontal(|ui| {
                ui.label("Input Directory:");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Browse").clicked() {
                        app.select_input_directory();
                    }
                });
            });

            if let Some(path) = &app.input_dir_path {
                ui.label(
                    RichText::new(path.to_string_lossy()).color(Color32::from_rgb(255, 165, 0)),
                );
            } else {
                ui.label(RichText::new("None selected").color(Color32::from_gray(120)));
            }

            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("Output Directory:");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Browse").clicked() {
                        app.select_output_directory();
                    }
                });
            });

            if let Some(path) = &app.output_dir_path {
                ui.label(
                    RichText::new(path.to_string_lossy()).color(Color32::from_rgb(255, 165, 0)),
                );
            } else {
                ui.label(RichText::new("None selected").color(Color32::from_gray(120)));
            }
        });
    }
}

pub struct FormatOptionsComponent;

impl FormatOptionsComponent {
    pub fn render(ui: &mut Ui, app: &mut SarproGui) {
        ui.heading("Format Options");

        Frame::NONE.inner_margin(0.0).show(ui, |ui| {
            ui.set_min_height(COMPONENT_HEIGHT);
            ui.set_min_width(COMPONENT_WIDTH);

            ui.horizontal(|ui| {
                ui.label("Image Format:");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let old_format = app.output_format;
                    ComboBox::from_id_salt("output_format")
                        .selected_text(format!("{:?}", app.output_format))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut app.output_format, OutputFormat::TIFF, "TIFF");
                            ui.selectable_value(&mut app.output_format, OutputFormat::JPEG, "JPEG");
                        });

                    // Update output path extension if format changed
                    if app.output_format != old_format {
                        app.update_output_path_extension();
                        if let Some(path) = &app.output_path {
                            tracing::debug!(
                                "Output format changed to {:?}, updated path: {:?}",
                                app.output_format,
                                path
                            );
                        }
                    }
                });
            });

            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("Bit Depth:");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ComboBox::from_id_salt("bit_depth")
                        .selected_text(format!("{:?}", app.bit_depth))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut app.bit_depth, BitDepth::U8, "8-bit");
                            // Disable 16-bit option when JPEG format is selected
                            if app.output_format == OutputFormat::JPEG {
                                ui.add_enabled_ui(false, |ui| {
                                    ui.selectable_value(&mut app.bit_depth, BitDepth::U16, "16-bit (not available for JPEG)");
                                });
                            } else {
                                ui.selectable_value(&mut app.bit_depth, BitDepth::U16, "16-bit");
                            }
                        });
                });
            });

            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("Polarization:");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ComboBox::from_id_salt("polarization")
                        .selected_text(format!("{:?}", app.polarization).to_uppercase())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut app.polarization, Polarization::Vv, "VV");
                            ui.selectable_value(&mut app.polarization, Polarization::Vh, "VH");
                            ui.selectable_value(&mut app.polarization, Polarization::Hh, "HH");
                            ui.selectable_value(&mut app.polarization, Polarization::Hv, "HV");
                            // Disable Multiband option when JPEG format is selected
                            if app.output_format == OutputFormat::JPEG {
                                // ui.add_enabled_ui(false, |ui| {
                                    ui.selectable_value(
                                        &mut app.polarization,
                                        Polarization::Multiband,
                                        "Multiband (synthetic RGB)",
                                    );
                                // });
                            } else {
                                ui.selectable_value(
                                    &mut app.polarization,
                                    Polarization::Multiband,
                                    "Multiband",
                                );
                            }
                            ui.separator();
                            ui.selectable_value(
                                &mut app.polarization,
                                Polarization::OP(PolarizationOperation::Sum),
                                "Sum",
                            );
                            ui.selectable_value(
                                &mut app.polarization,
                                Polarization::OP(PolarizationOperation::Diff),
                                "Diff",
                            );
                            ui.selectable_value(
                                &mut app.polarization,
                                Polarization::OP(PolarizationOperation::Ratio),
                                "Ratio",
                            );
                            ui.selectable_value(
                                &mut app.polarization,
                                Polarization::OP(PolarizationOperation::NDiff),
                                "Normalized Diff",
                            );
                            ui.selectable_value(
                                &mut app.polarization,
                                Polarization::OP(PolarizationOperation::LogRatio),
                                "Log Ratio",
                            );
                        });
                });
            });

            ui.add_space(10.0);

            let polarization_info = match app.polarization {
                Polarization::Vv => {
                    "Looks for VV (Vertical-Vertical) polarization. The output is grayscale."
                },
                Polarization::Vh => {
                    "Looks for VH (Vertical-Horizontal) polarization. The output is grayscale."
                },
                Polarization::Hh => {
                    "Looks for HH (Horizontal-Horizontal) polarization. The output is grayscale."
                },
                Polarization::Hv => {
                    "Looks for HV (Horizontal-Vertical) polarization. The output is grayscale."
                },
                Polarization::Multiband => {
                    if app.output_format == OutputFormat::JPEG {
                        "Multiband (synthetic RGB). The output is R=VV|HH, G=VH|HV, B=VV|HH/VH|HV with gamma correction for each channel. Use Tamed autoscale strategy for maximum contrast."
                    } else {
                        "Multiband. The output is grayscale. Use with Tamed autoscale strategy to isolate the ground from the water."
                    }
                },
                Polarization::OP(PolarizationOperation::Sum) => {
                    "Sum (band1 + band2). The output is grayscale."
                },
                Polarization::OP(PolarizationOperation::Diff) => {
                    "Diff (band1 - band2). The output is grayscale."
                },
                Polarization::OP(PolarizationOperation::Ratio) => {
                    "Ratio (band1 / band2). The output is grayscale."
                },
                Polarization::OP(PolarizationOperation::NDiff) => {
                    "Normalized Diff (band1 - band2) / (band1 + band2). The output is grayscale. Try with Tamed autoscale strategy for maximum contrast."
                },
                Polarization::OP(PolarizationOperation::LogRatio) => {
                    "Log Ratio (log(band1 / band2)). The output is grayscale. Try with Tamed autoscale strategy for maximum contrast."
                },
            };
            ui.label(
                RichText::new(polarization_info)
                    .color(Color32::from_gray(120))
                    .size(11.0)
            );

            // ui.label(
            //     RichText::new("Required polarization(s) must be present in the GRD product, or processing will be skipped. E.g., selecting VV or Multiband on HH-only data, or any OP on single-band data.")
            //         .color(Color32::from_gray(120))
            //         .size(11.0)
            // );
        });
    }
}

pub struct SizeOptionsComponent;

impl SizeOptionsComponent {
    pub fn render(ui: &mut Ui, app: &mut SarproGui) {
        ui.heading("Size Options");

        Frame::NONE.inner_margin(0.0).show(ui, |ui| {
            ui.set_min_height(COMPONENT_HEIGHT * 0.6);
            ui.set_min_width(COMPONENT_WIDTH);

            ui.horizontal(|ui| {
                ui.label("Size Mode:");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ComboBox::from_id_salt("size_mode")
                        .selected_text(match app.size_mode {
                            SizeMode::Original => "Original".to_string(),
                            SizeMode::Predefined(size) => format!("{}", size),
                            SizeMode::Custom => "Custom".to_string(),
                        })
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(
                                    &mut app.size_mode,
                                    SizeMode::Original,
                                    "Original",
                                )
                                .clicked()
                            {
                                app.custom_size.clear();
                            }
                            for &size in &[512, 1024, 2048] {
                                if ui
                                    .selectable_value(
                                        &mut app.size_mode,
                                        SizeMode::Predefined(size),
                                        &size.to_string(),
                                    )
                                    .clicked()
                                {
                                    app.custom_size.clear();
                                }
                            }
                            if ui
                                .selectable_value(&mut app.size_mode, SizeMode::Custom, "Custom")
                                .clicked()
                            {
                                // Do nothing, keep custom_size
                            }
                        });
                });
            });

            if matches!(app.size_mode, SizeMode::Custom) {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label("Custom Size (pixels):");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let response = ui.text_edit_singleline(&mut app.custom_size);
                        if let Some(text) = response.changed().then(|| app.custom_size.clone()) {
                            app.custom_size = text.chars().filter(|c| c.is_ascii_digit()).collect();
                        }
                    });
                });
            }
        });
    }
}

pub struct OptionsComponent;

impl OptionsComponent {
    pub fn render(ui: &mut Ui, app: &mut SarproGui) {
        ui.heading("Processing Options");

        Frame::NONE.inner_margin(0.0).show(ui, |ui| {
            ui.set_min_height(COMPONENT_HEIGHT);
            ui.set_min_width(COMPONENT_WIDTH);

            ui.horizontal(|ui| {
                ui.label("Enable padding:");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.checkbox(&mut app.pad, "");
                });
            });

            ui.add_space(5.0);

            // Add informative text about padding
            ui.label(
                RichText::new("Adds top/bottom padding to make images square based on the longer side. Centers the image and adds zero padding.")
                    .color(Color32::from_gray(120))
                    .size(11.0)
            );

            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("Autoscale:");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ComboBox::from_id_salt("autoscale_strategy")
                        .selected_text(format!("{:?}", app.autoscale))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut app.autoscale, AutoscaleStrategy::Standard, "Standard");
                            ui.selectable_value(&mut app.autoscale, AutoscaleStrategy::Robust, "Robust");
                            ui.selectable_value(&mut app.autoscale, AutoscaleStrategy::Adaptive, "Adaptive");
                            ui.selectable_value(&mut app.autoscale, AutoscaleStrategy::Equalized, "Equalized");
                            ui.selectable_value(&mut app.autoscale, AutoscaleStrategy::Tamed, "Tamed");
                            ui.selectable_value(&mut app.autoscale, AutoscaleStrategy::Default, "Default");
                        });
                });
            });

            ui.add_space(5.0);

            // Add informative text about the selected strategy
            let strategy_info = match app.autoscale {
                AutoscaleStrategy::Standard => {
                    "Standard SAR autoscaling with robust percentiles (2nd/98th). Handles outliers well and adapts to image characteristics (low contrast, heavy-tailed, high dynamic range). Recommended."
                }
                AutoscaleStrategy::Robust => {
                    "Robust statistics approach using IQR-based outlier detection. Handles extreme outliers well with 2.5×IQR threshold for clipping."
                }
                AutoscaleStrategy::Adaptive => {
                    "Adaptive scaling based on image characteristics. Analyzes skewness and tail heaviness to automatically adjust percentiles and gamma correction. Slow. Use default or standard for speed."
                }
                AutoscaleStrategy::Equalized => {
                    "Histogram equalization approach using 1st/99th percentiles. Provides maximum enhancement of even the darkest pixels."
                }
                AutoscaleStrategy::Tamed => {
                    "Tamed scaling based on 25th/99th percentiles. Provides maximum contrast enhancement for visualization. Recommended for synRGB and the first to try."
                }
                AutoscaleStrategy::Default => {
                    "Default advanced scaling (same as Adaptive). Automatically analyzes image characteristics and applies optimal scaling parameters. Recommended."
                }
            };

            ui.label(
                RichText::new(strategy_info)
                    .color(Color32::from_gray(120))
                    .size(11.0)
            );

            ui.add_space(12.0);

            // Target CRS option
            ui.horizontal(|ui| {
                ui.label("Target CRS:");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // simple editable text field for EPSG or WKT; default EPSG:32630
                    let response = ui.text_edit_singleline(&mut app.target_crs);
                    if response.changed() && app.target_crs.trim().is_empty() {
                        app.target_crs = "EPSG:32630".to_string();
                    }
                });
            });
            ui.label(
                RichText::new("Map projection to reproject into (e.g., EPSG:4326, EPSG:32633). Default: EPSG:32630.")
                    .color(Color32::from_gray(120))
                    .size(11.0)
            );

            ui.add_space(8.0);

            // Resample algorithm option
            ui.horizontal(|ui| {
                ui.label("Resample:");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ComboBox::from_id_salt("resample_alg")
                        .selected_text(app.resample_alg.to_string())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut app.resample_alg, "nearest".to_string(), "nearest");
                            ui.selectable_value(&mut app.resample_alg, "bilinear".to_string(), "bilinear");
                            ui.selectable_value(&mut app.resample_alg, "cubic".to_string(), "cubic");
                        });
                });
            });
            let resample_info = match app.resample_alg.as_str() {
                "nearest" => "Nearest neighbor resampling. Fastest but least accurate.",
                "bilinear" => "Bilinear resampling. Good balance of speed and accuracy. Default.",
                "cubic" => "Cubic resampling. Best quality but slowest.",
                _ => "Unknown resampling algorithm.",
            };
            ui.label(
                RichText::new(resample_info)
                    .color(Color32::from_gray(120))
                    .size(11.0)
            );
        });
    }
}

pub struct FooterComponent;

impl FooterComponent {
    pub fn render(ui: &mut Ui, app: &mut SarproGui) {
        // Update system statistics
        app.update_system_stats();

        ui.horizontal(|ui| {
            // Left side - Timing and system information
            let status_color = if app.is_processing {
                Color32::from_rgb(255, 165, 0) // Orange for processing
            } else {
                Color32::from_rgb(100, 200, 100) // Green for ready
            };

            let timing_text = if app.is_processing {
                if let Some(start_time) = app.processing_start_time {
                    let elapsed = start_time.elapsed();
                    format!("Processing: {:.2?}", elapsed)
                } else {
                    "Processing...".to_string()
                }
            } else if let Some(duration) = app.last_processing_duration {
                format!("Last run: {:.2?}", duration)
            } else {
                "Ready".to_string()
            };

            ui.label(RichText::new(timing_text).color(status_color).size(14.0));

            // System monitoring information
            ui.separator();

            // CPU usage
            let cpu_color = if app.cpu_usage > 80.0 {
                Color32::from_rgb(255, 100, 100) // Red for high usage
            } else if app.cpu_usage > 50.0 {
                Color32::from_rgb(255, 165, 0) // Orange for medium usage
            } else {
                Color32::from_rgb(100, 200, 100) // Green for low usage
            };

            ui.label(
                RichText::new(format!("CPU: {:.1}%", app.cpu_usage))
                    .color(cpu_color)
                    .size(12.0),
            );

            ui.separator();

            // Memory usage
            let memory_percent = if app.total_memory_mb > 0.0 {
                (app.memory_usage_mb / app.total_memory_mb) * 100.0
            } else {
                0.0
            };

            let memory_color = if memory_percent > 80.0 {
                Color32::from_rgb(255, 100, 100) // Red for high usage
            } else if memory_percent > 60.0 {
                Color32::from_rgb(255, 165, 0) // Orange for medium usage
            } else {
                Color32::from_rgb(100, 200, 100) // Green for low usage
            };

            ui.label(
                RichText::new(format!(
                    "RAM: {:.1} GB / {:.1} GB ({:.1}%)",
                    app.memory_usage_mb / 1024.0,
                    app.total_memory_mb / 1024.0,
                    memory_percent
                ))
                .color(memory_color)
                .size(12.0),
            );

            // Right side - Buttons
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("To CLI").clicked() {
                    let cli_command = app.generate_cli_command();

                    // Create a special CLI command entry (not a regular log)
                    let cli_entry = crate::gui::logging::LogEntry::new(
                        tracing::Level::INFO,
                        format!("CLI Command: {}", cli_command),
                        "cli".to_string(),
                    );

                    if let Ok(mut logs) = app.log_messages.lock() {
                        logs.push(cli_entry);
                    }
                }

                if ui.button("Save Preset").clicked() {
                    match app.save_preset() {
                        Ok(()) => {
                            // Success is logged in the method
                        }
                        Err(e) => {
                            tracing::error!("Failed to save preset: {}", e);
                        }
                    }
                }

                if ui.button("Load Preset").clicked() {
                    match app.load_preset() {
                        Ok(()) => {
                            // Success is logged in the method
                        }
                        Err(e) => {
                            tracing::error!("Failed to load preset: {}", e);
                        }
                    }
                }

                if ui.button("Save Logs").clicked() {
                    match app.save_logs_to_file() {
                        Ok(()) => {
                            // Success is logged in the method
                        }
                        Err(e) => {
                            tracing::error!("Failed to save logs: {}", e);
                        }
                    }
                }

                if ui.button("Clear").clicked() {
                    if let Ok(mut logs) = app.log_messages.lock() {
                        logs.clear();
                    }
                }

                if ui.button("Reset").clicked() {
                    *app = SarproGui::default();
                }
            });
        });
    }
}
