use super::components::*;
use super::models::SarproGui;
use crate::gui::logging::{LogEntry, get_log_buffer};
use eframe::egui;
use egui_extras::install_image_loaders;
use tracing::Level;

fn format_log_entry(entry: &LogEntry) -> egui::RichText {
    // Special handling for separator entries
    if entry.message.starts_with("---") {
        return egui::RichText::new(&entry.message)
            .color(egui::Color32::from_rgb(255, 165, 0)) // Orange for separators
            .monospace()
            .strong();
    }

    // Special handling for CLI commands
    if entry.target == "cli" {
        return egui::RichText::new(&entry.message)
            .color(egui::Color32::from_rgb(100, 255, 100)) // Green for CLI commands
            .monospace()
            .strong();
    }

    let (color, icon) = match entry.level {
        Level::ERROR => (egui::Color32::from_rgb(255, 100, 100), "❌"),
        Level::WARN => (egui::Color32::from_rgb(255, 200, 100), "⚠️"),
        Level::INFO => (egui::Color32::from_rgb(100, 200, 255), "ℹ️"),
        Level::DEBUG => (egui::Color32::from_rgb(150, 150, 150), "🔍"),
        Level::TRACE => (egui::Color32::from_rgb(100, 100, 100), "🔎"),
    };

    let formatted_text = format!(
        "[{}] {} {}: {}",
        entry.timestamp, icon, entry.level, entry.message
    );

    egui::RichText::new(formatted_text).color(color).monospace()
}

impl eframe::App for SarproGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Initialize logging and image loaders on first update
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            crate::gui::models::init_gui_logging();
            install_image_loaders(ctx);
        });

        // Set dark theme colors inspired by the attached image
        let mut style = (*ctx.style()).clone();
        style.visuals.override_text_color = Some(egui::Color32::from_gray(220));
        style.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(40, 40, 40);
        style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(50, 50, 50);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(60, 60, 60);
        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(70, 70, 70);
        style.visuals.panel_fill = egui::Color32::from_rgb(30, 30, 30);
        style.visuals.window_fill = egui::Color32::from_rgb(25, 25, 25);
        style.visuals.faint_bg_color = egui::Color32::from_rgb(45, 45, 45);
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(20, 20, 20);

        ctx.set_style(style);

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.add_space(5.0);
                            ui.add(
                                egui::Image::new(egui::include_image!(
                                    "../assets/sarprogui_icon.png"
                                ))
                                .fit_to_exact_size(egui::Vec2::new(40.0, 40.0)),
                            );
                        });

                        ui.label(
                            egui::RichText::new("SARPRO")
                                .size(42.0)
                                .color(egui::Color32::from_gray(220))
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.label(
                                egui::RichText::new(format!("v{} ", env!("CARGO_PKG_VERSION")))
                                    .size(10.0)
                                    .color(egui::Color32::WHITE),
                            );
                            ui.label(
                                egui::RichText::new("MIT - Apache-2.0 License")
                                    .size(10.0)
                                    .color(egui::Color32::from_gray(150)),
                            );
                            ui.label(
                                egui::RichText::new("dan vi - github.com/bogwi/sarpro")
                                    .size(10.0)
                                    .color(egui::Color32::from_gray(150)),
                            );
                            
                        });
                    });
                    ui.label(
                        egui::RichText::new("SENTINEL-1 GRD PRODUCT IMAGE PROCESSOR")
                            .size(12.0)
                            .color(egui::Color32::from_gray(220))
                            .strong(),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(!self.is_processing, |ui| {
                        if ui
                            .button(
                                egui::RichText::new("Process")
                                    .size(16.0)
                                    .color(egui::Color32::WHITE),
                            )
                            .clicked()
                        {
                            self.process_files();
                        }
                    });
                });
            });
        });

        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            FooterComponent::render(ui, self);
        });

        egui::SidePanel::left("left_panel")
            // .resizable(true)
            .resizable(false)
            .default_width(150.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.add_space(10.0);

                        // Mode selection
                        ModeSelectionComponent::render(ui, self);

                        ui.separator();

                        // File selection based on mode
                        if !self.batch_mode {
                            FileSelectionComponent::render_single_file(ui, self);
                        } else {
                            FileSelectionComponent::render_batch_mode(ui, self);
                        }

                        ui.separator();

                        // Format options
                        FormatOptionsComponent::render(ui, self);

                        ui.separator();

                        // Size options
                        SizeOptionsComponent::render(ui, self);

                        ui.separator();

                        // Options
                        OptionsComponent::render(ui, self);

                        ui.add_space(20.0);
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Handle incoming log messages
            let mut has_new_logs = false;
            let log_buffer = get_log_buffer();
            let mut new_messages = Vec::new();
            if let Ok(mut buf) = log_buffer.lock() {
                if !buf.is_empty() {
                    new_messages.extend(buf.drain(..));
                }
            }
            if !new_messages.is_empty() {
                let mut logs = self.log_messages.lock().unwrap();
                logs.extend(new_messages);
                // Keep only last 1000 messages to prevent memory issues
                let len = logs.len();
                if len > 1000 {
                    logs.drain(0..(len - 1000));
                }
                has_new_logs = true;
            }

            // Request repaint if new logs were received or if processing is active
            if has_new_logs || self.is_processing {
                ctx.request_repaint();
            }

            // Check for completion of background processing
            if let Some(receiver) = &mut self.completion_receiver {
                if let Ok(msg) = receiver.try_recv() {
                    // Calculate processing duration
                    if let Some(start_time) = self.processing_start_time {
                        let duration = start_time.elapsed();
                        self.last_processing_duration = Some(duration);
                        tracing::info!("Processing completed in {:.2?}", duration);
                        tracing::debug!(
                            "Processing statistics: duration={:.2?}, start_time={:?}",
                            duration,
                            start_time
                        );
                    }

                    // Log the completion message instead of setting status
                    tracing::info!("{}", msg);

                    self.is_processing = false;
                    self.processing_start_time = None;
                    self.completion_receiver = None;
                }
            }

            ui.horizontal(|ui| {
                ui.label("Log Output");

                // Log level filter buttons
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.selectable_value(&mut self.min_log_level, tracing::Level::ERROR, "ERROR");
                    ui.selectable_value(&mut self.min_log_level, tracing::Level::WARN, "WARN");
                    ui.selectable_value(&mut self.min_log_level, tracing::Level::INFO, "INFO");
                    ui.selectable_value(&mut self.min_log_level, tracing::Level::DEBUG, "DEBUG");
                    ui.selectable_value(&mut self.min_log_level, tracing::Level::TRACE, "ALL");
                });

                if let Ok(logs) = self.log_messages.lock() {
                    let total_logs = logs.len();
                    let visible_logs = logs
                        .iter()
                        .filter(|entry| {
                            if self.min_log_level == tracing::Level::TRACE {
                                // Show all logs when ALL is selected
                                true
                            } else {
                                // Show only the specific level
                                entry.level == self.min_log_level
                            }
                        })
                        .count();
                    if total_logs > 0 {
                        ui.label(format!("({} visible / {} total)", visible_logs, total_logs));
                    }
                }
            });

            ui.add_space(5.0);

            egui::ScrollArea::vertical()
                .max_height(ui.available_height() - 40.0)
                .show(ui, |ui| {
                    if let Ok(logs) = self.log_messages.lock() {
                        if logs.is_empty() {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    egui::RichText::new("No log messages")
                                        .color(egui::Color32::from_gray(120)),
                                );
                            });
                        } else {
                            for entry in logs.iter() {
                                if self.min_log_level == tracing::Level::TRACE {
                                    // Show all logs when ALL is selected
                                    ui.label(format_log_entry(entry));
                                } else if entry.level == self.min_log_level {
                                    // Show only the specific level
                                    ui.label(format_log_entry(entry));
                                }
                            }
                        }
                    }
                });
        });
    }
}
