use crate::gui::logging::{GuiLogLayer, LogEntry};
use crate::{AutoscaleStrategy, InputFormat, Polarization, PolarizationOperation};
use crate::{BitDepth, OutputFormat};
use crate::types::SyntheticRgbMode;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo;
use tracing::Level;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, Registry};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum SizeMode {
    Original,
    Predefined(usize),
    Custom,
}

static LOGGING_INIT: OnceCell<()> = OnceCell::new();

pub fn init_gui_logging() {
    LOGGING_INIT.get_or_init(|| {
        let gui_layer = GuiLogLayer::new();

        // Creta a filter for eframw TRACE logs.
        let filter = EnvFilter::new("trace")
            .add_directive("eframe=info".parse().unwrap())
            .add_directive("winit=info".parse().unwrap());

        let subscriber = Registry::default().with(gui_layer).with(filter);
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

pub struct SarproGui {
    // Input parameters
    pub input_path: Option<PathBuf>,
    pub input_dir_path: Option<PathBuf>,
    pub output_path: Option<PathBuf>,
    pub output_dir_path: Option<PathBuf>,

    // Format parameters
    pub output_format: OutputFormat,
    pub input_format: InputFormat,
    pub bit_depth: BitDepth,
    pub polarization: Polarization,
    pub autoscale: AutoscaleStrategy,
    pub synrgb_mode: SyntheticRgbMode,

    // Reprojection parameters
    pub target_crs: String,
    pub resample_alg: String,

    // Size parameters
    pub size_mode: SizeMode,
    pub custom_size: String,

    // Options
    pub enable_logging: bool,
    pub batch_mode: bool,
    pub pad: bool,
    pub min_log_level: Level,

    // Status
    pub status_message: String,
    pub is_processing: bool,
    pub processing_start_time: Option<Instant>,
    pub last_processing_duration: Option<Duration>,

    // Log messages for the right panel - now thread-safe
    pub log_messages: Arc<Mutex<Vec<LogEntry>>>,

    // Receiver for completion notification from background processing
    pub completion_receiver: Option<std::sync::mpsc::Receiver<String>>,

    // System monitoring
    pub cpu_usage: f32,
    pub memory_usage_mb: f64,
    pub total_memory_mb: f64,
    pub system_monitor: Option<sysinfo::System>,
    pub last_system_update: Option<Instant>,
}

impl Default for SarproGui {
    fn default() -> Self {
        Self {
            input_path: None,
            input_dir_path: None,
            output_path: None,
            output_dir_path: None,
            output_format: OutputFormat::TIFF,
            input_format: InputFormat::Safe,
            bit_depth: BitDepth::U8,
            polarization: Polarization::Vv,
            autoscale: AutoscaleStrategy::Clahe,
            synrgb_mode: SyntheticRgbMode::Default,
            target_crs: "EPSG:32630".to_string(),
            resample_alg: "lanczos".to_string(),
            size_mode: SizeMode::Original,
            custom_size: String::new(),
            enable_logging: false,
            batch_mode: false,
            pad: false,
            min_log_level: Level::INFO,
            status_message: "Ready".to_string(),
            is_processing: false,
            processing_start_time: None,
            last_processing_duration: None,
            log_messages: Arc::new(Mutex::new(Vec::new())),
            completion_receiver: None,
            cpu_usage: 0.0,
            memory_usage_mb: 0.0,
            total_memory_mb: 0.0,
            system_monitor: None,
            last_system_update: None,
        }
    }
}

impl SarproGui {
    pub fn save_logs_to_file(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Get current logs
        let logs = self
            .log_messages
            .lock()
            .map_err(|e| format!("Failed to lock logs: {}", e))?;

        if logs.is_empty() {
            return Err("No logs to save".into());
        }

        // Filter logs based on current filter level
        let filtered_logs: Vec<&LogEntry> = logs
            .iter()
            .filter(|entry| {
                if self.min_log_level == Level::TRACE {
                    // Show all logs when ALL is selected
                    true
                } else {
                    // Show only the specific level
                    entry.level == self.min_log_level
                }
            })
            .collect();

        if filtered_logs.is_empty() {
            return Err("No logs match the current filter level".into());
        }

        // Open file dialog for save location
        if let Some(save_path) = rfd::FileDialog::new()
            .add_filter("SARPRO Log files", &["sarpolog"])
            .set_file_name("sarpro_log.sarpolog")
            .save_file()
        {
            // Format logs for file
            let mut log_content = String::new();
            log_content.push_str("=== SARPRO Log File ===\n");
            log_content.push_str(&format!("Generated: {}\n", chrono::Utc::now().to_rfc3339()));
            log_content.push_str(&format!(
                "Filter Level: {}\n",
                match self.min_log_level {
                    Level::ERROR => "ERROR",
                    Level::WARN => "WARN",
                    Level::INFO => "INFO",
                    Level::DEBUG => "DEBUG",
                    Level::TRACE => "ALL",
                }
            ));
            log_content.push_str(&format!("Total Logs: {}\n", filtered_logs.len()));
            log_content.push_str("=====================\n\n");

            for entry in &filtered_logs {
                let level_str = match entry.level {
                    Level::ERROR => "ERROR",
                    Level::WARN => "WARN",
                    Level::INFO => "INFO",
                    Level::DEBUG => "DEBUG",
                    Level::TRACE => "TRACE",
                };

                log_content.push_str(&format!(
                    "[{}] {} {}: {}\n",
                    entry.timestamp, level_str, entry.target, entry.message
                ));
            }

            // Write to file
            fs::write(&save_path, log_content)?;

            // Log the save action
            tracing::info!(
                "Filtered logs saved to: {:?} ({} entries)",
                save_path,
                filtered_logs.len()
            );

            Ok(())
        } else {
            Err("No save location selected".into())
        }
    }

    pub fn save_preset(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Create a preset struct with only the configuration fields
        #[derive(Serialize)]
        struct SarproPreset {
            output_format: OutputFormat,
            input_format: InputFormat,
            bit_depth: BitDepth,
            polarization: Polarization,
            autoscale: AutoscaleStrategy,
            synrgb_mode: SyntheticRgbMode,
            target_crs: String,
            resample_alg: String,
            size_mode: SizeMode,
            custom_size: String,
            batch_mode: bool,
            pad: bool,
            min_log_level: String, // Store as string
        }

        let preset = SarproPreset {
            output_format: self.output_format,
            input_format: self.input_format,
            bit_depth: self.bit_depth,
            polarization: self.polarization,
            autoscale: self.autoscale,
            synrgb_mode: self.synrgb_mode,
            target_crs: self.target_crs.clone(),
            resample_alg: self.resample_alg.clone(),
            size_mode: self.size_mode,
            custom_size: self.custom_size.clone(),
            batch_mode: self.batch_mode,
            pad: self.pad,
            min_log_level: format!("{:?}", self.min_log_level),
        };

        // Open file dialog for save location
        if let Some(save_path) = rfd::FileDialog::new()
            .add_filter("SARPRO Preset files", &["sarpro"])
            .set_file_name("sarpro_preset.sarpro")
            .save_file()
        {
            // Create header with metadata
            let mut preset_content = String::new();
            preset_content.push_str("// ==========================================\n");
            preset_content.push_str("// SARPRO Configuration Preset\n");
            preset_content.push_str("// ==========================================\n");
            preset_content.push_str(&format!("// Program: SARPRO - SAR Image Processing Tool\n"));
            preset_content.push_str(&format!("// Version: {}\n", env!("CARGO_PKG_VERSION")));
            preset_content.push_str(&format!(
                "// Generated: {}\n",
                chrono::Utc::now().to_rfc3339()
            ));
            preset_content.push_str(&format!(
                "// Description: Processing configuration preset\n"
            ));
            preset_content.push_str("// Note: Input/Output paths are not included in presets\n");
            preset_content.push_str("// ==========================================\n\n");

            let json = serde_json::to_string_pretty(&preset)?;
            preset_content.push_str(&json);

            fs::write(&save_path, preset_content)?;

            tracing::info!("Preset saved to: {:?}", save_path);
            Ok(())
        } else {
            Err("No save location selected".into())
        }
    }

    pub fn load_preset(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Open file dialog for load location
        if let Some(load_path) = rfd::FileDialog::new()
            .add_filter("SARPRO Preset files", &["sarpro"])
            .pick_file()
        {
            let content = fs::read_to_string(&load_path)?;

            // Extract JSON part by finding the first '{' character
            let json_start = content
                .find('{')
                .ok_or("Invalid preset file: no JSON content found")?;
            let json = &content[json_start..];

            #[derive(Deserialize)]
            struct SarproPreset {
                output_format: OutputFormat,
                input_format: InputFormat,
                bit_depth: BitDepth,
                polarization: Polarization,
                autoscale: AutoscaleStrategy,
                synrgb_mode: SyntheticRgbMode,
                target_crs: String,
                resample_alg: String,
                size_mode: SizeMode,
                custom_size: String,
                batch_mode: bool,
                pad: bool,
                min_log_level: String, // Load as string
            }

            let preset: SarproPreset = serde_json::from_str(&json)?;

            // Parse log level from string
            let min_log_level = match preset.min_log_level.as_str() {
                "ERROR" => Level::ERROR,
                "WARN" => Level::WARN,
                "INFO" => Level::INFO,
                "DEBUG" => Level::DEBUG,
                "TRACE" => Level::TRACE,
                _ => Level::INFO, // Default fallback
            };

            // Apply the preset configuration
            self.output_format = preset.output_format;
            self.input_format = preset.input_format;
            self.bit_depth = preset.bit_depth;
            self.polarization = preset.polarization;
            self.autoscale = preset.autoscale;
            self.synrgb_mode = preset.synrgb_mode;
            self.target_crs = preset.target_crs;
            self.resample_alg = preset.resample_alg;
            self.size_mode = preset.size_mode;
            self.custom_size = preset.custom_size;
            self.batch_mode = preset.batch_mode;
            self.pad = preset.pad;
            self.min_log_level = min_log_level;

            tracing::info!("Preset loaded from: {:?}", load_path);
            Ok(())
        } else {
            Err("No preset file selected".into())
        }
    }

    pub fn generate_cli_command(&self) -> String {
        let mut cmd = String::from("cargo run --release --bin sarpro --");

        // Add input/output paths depending on the mode to avoid mixing single and batch flags
        if self.batch_mode {
            if let Some(input_dir) = &self.input_dir_path {
                cmd.push_str(&format!(" --input-dir {:?}", input_dir));
            }
            if let Some(output_dir) = &self.output_dir_path {
                cmd.push_str(&format!(" --output-dir {:?}", output_dir));
            }
        } else {
            if let Some(input_path) = &self.input_path {
                cmd.push_str(&format!(" --input {:?}", input_path));
            }
            if let Some(output_path) = &self.output_path {
                cmd.push_str(&format!(" --output {:?}", output_path));
            }
        }

        // Add format options
        cmd.push_str(&format!(" --format {:?}", self.output_format).to_lowercase());
        // cmd.push_str(&format!(" --input-format {:?}", self.input_format).to_lowercase());
        cmd.push_str(&format!(" --bit-depth {:?}", self.bit_depth).to_lowercase());

        // Add polarization (convert to CLI format)
        let polarization_cli = match self.polarization {
            Polarization::Vv => "vv",
            Polarization::Vh => "vh",
            Polarization::Hh => "hh",
            Polarization::Hv => "hv",
            Polarization::Multiband => "multiband",
            Polarization::OP(PolarizationOperation::Sum) => "sum",
            Polarization::OP(PolarizationOperation::Diff) => "diff",
            Polarization::OP(PolarizationOperation::Ratio) => "ratio",
            Polarization::OP(PolarizationOperation::NDiff) => "n-diff",
            Polarization::OP(PolarizationOperation::LogRatio) => "log-ratio",
        };
        cmd.push_str(&format!(" --polarization {}", polarization_cli).to_lowercase());

        // Add autoscale strategy
        let autoscale_cli = match self.autoscale {
            AutoscaleStrategy::Standard => "standard",
            AutoscaleStrategy::Robust => "robust",
            AutoscaleStrategy::Adaptive => "adaptive",
            AutoscaleStrategy::Equalized => "equalized",
            AutoscaleStrategy::Clahe => "clahe",
            AutoscaleStrategy::Tamed => "tamed",
            AutoscaleStrategy::Default => "default",
        };
        cmd.push_str(&format!(" --autoscale {}", autoscale_cli));

        // Add synthetic RGB mode when applicable
        if self.output_format == OutputFormat::JPEG && matches!(self.polarization, Polarization::Multiband) {
            let mode_cli = match self.synrgb_mode {
                SyntheticRgbMode::Default => "default",
                SyntheticRgbMode::RgbRatio => "rgb-ratio",
                SyntheticRgbMode::SarUrban => "sar-urban",
                SyntheticRgbMode::Enhanced => "enhanced",
            };
            cmd.push_str(&format!(" --synrgb-mode {}", mode_cli));
        }

        // Add reprojection options
        if !self.target_crs.trim().is_empty() {
            cmd.push_str(&format!(" --target-crs {}", self.target_crs.trim()));
        }
        if !self.resample_alg.trim().is_empty() {
            cmd.push_str(&format!(" --resample-alg {}", self.resample_alg.trim()));
        }

        // Add size parameter
        let size_str = match self.size_mode {
            SizeMode::Original => "original".to_string(),
            SizeMode::Predefined(size) => size.to_string(),
            SizeMode::Custom => self.custom_size.clone(),
        };
        cmd.push_str(&format!(" --size {}", size_str));

        // Add boolean flags
        if self.batch_mode {
            cmd.push_str(" --batch");
        }
        if self.pad {
            cmd.push_str(" --pad");
        }
        // we always want to log
        cmd.push_str(" --log");

        cmd
    }

    /// Update system statistics (CPU and memory usage)
    pub fn update_system_stats(&mut self) {
        // Only update every 2 seconds to avoid excessive system calls
        let now = Instant::now();
        if let Some(last_update) = self.last_system_update {
            if now.duration_since(last_update).as_secs() < 2 {
                return;
            }
        }

        // Initialize system monitor if not already done
        if self.system_monitor.is_none() {
            self.system_monitor = Some(sysinfo::System::new_all());
        }

        if let Some(ref mut sys) = self.system_monitor {
            // Refresh system information
            sys.refresh_all();

            // Get CPU usage (percentage)
            self.cpu_usage = sys.global_cpu_usage();

            // Get memory information
            self.memory_usage_mb = sys.used_memory() as f64 / 1024.0 / 1024.0;
            self.total_memory_mb = sys.total_memory() as f64 / 1024.0 / 1024.0;
        }

        self.last_system_update = Some(now);
    }
}
