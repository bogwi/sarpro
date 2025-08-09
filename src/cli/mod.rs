//! Command Line Interface (CLI) layer for SARPRO.
//!
//! This module defines argument parsing (`args`), error types (`errors`),
//! and the orchestration logic (`runner`) for single-file and batch
//! processing flows. It wires user-provided options to the underlying
//! library functionality exposed via `sarpro::api`.
//!
//! If you are embedding SARPRO into another application, prefer using
//! the high-level `sarpro::api` module instead of calling the CLI code.
pub mod args;
pub mod errors;
pub mod runner;

pub use args::CliArgs;
pub use runner::run;

