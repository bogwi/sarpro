//! Sentinel-1 SAR data processing module
//!
//! This module provides comprehensive support for reading and processing
//! Sentinel-1 Synthetic Aperture Radar (SAR) data in SAFE format.

pub mod errors;
pub mod types;
pub mod reader;
pub mod reader_methods;
pub mod discovery;
pub mod data_loader;
pub mod manifest_parser;
pub mod annotation_parser;

pub mod crs_utils;

// Re-export main types and functions for convenient access
pub use errors::SafeError;
pub use types::{ProductType, TargetCrsArg, SafeMetadata};
pub use reader::SafeReader;
pub use discovery::identify_polarization_files;
pub use data_loader::{load_polarization_data, load_polarization_data_with_options};
pub use manifest_parser::{parse_comprehensive_metadata, parse_manifest_safe};
pub use annotation_parser::{parse_annotation_files, parse_annotation_xml};

pub use crs_utils::{resolve_auto_target_crs, lonlat_to_epsg, resolve_auto_target_crs_from_dataset_path};