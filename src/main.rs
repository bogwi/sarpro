//! SARPRO CLI entrypoint.
//!
//! Provides a thin wrapper over the `cli` module: parse args, dispatch to
//! single-file or batch processing, and exit with appropriate status.
//! For programmatic use, prefer the library API (`sarpro::api`).

use clap::Parser;

mod cli;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = cli::CliArgs::parse();
    cli::run(args)
}
