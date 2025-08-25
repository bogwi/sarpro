//! High-level, ergonomic library API: process SAFE to files or in-memory buffers,
//! batch helpers for directories, and typed save/load helpers. Prefer using these
//! entrypoints over low-level processing modules when integrating SARPRO.

// Module declarations
pub mod processed_image;
pub mod batch_report;
pub mod iterate_safe_products;
pub mod process_safe_to_buffer;
pub mod process_safe_to_buffer_with_mode;
pub mod process_directory_to_path;
pub mod process_safe_to_path;
pub mod process_safe_with_options;
pub mod save_image;
pub mod save_multiband_image;
pub mod load_polarization;
pub mod load_operation;

// Re-exports
pub use processed_image::ProcessedImage;
pub use batch_report::BatchReport;
pub use iterate_safe_products::iterate_safe_products;
pub use process_safe_to_buffer::process_safe_to_buffer;
pub use process_safe_to_buffer_with_mode::process_safe_to_buffer_with_mode;
pub use process_directory_to_path::process_directory_to_path;
pub use process_safe_to_path::process_safe_to_path;
pub use process_safe_with_options::process_safe_with_options;
pub use save_image::save_image;
pub use save_multiband_image::save_multiband_image;
pub use load_polarization::load_polarization;
pub use load_operation::load_operation;
