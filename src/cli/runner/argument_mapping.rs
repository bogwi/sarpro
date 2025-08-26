use sarpro::core::params::ProcessingOptions;


use crate::cli::args::CliArgs;

pub fn build_processing_options_from_args(args: &CliArgs) -> ProcessingOptions {
    // Map size string to Option<usize>
    let size_opt = if args.size == "original" {
        None
    } else {
        args.size.parse::<usize>().ok().filter(|v| *v > 0)
    };

    let mut opts = ProcessingOptions::default();
    opts.format = args.format;
    opts.input_format = args.input_format;
    opts.bit_depth = args.bit_depth;
    opts.polarization = args.polarization;
    opts.autoscale = args.autoscale;
    opts.synrgb_mode = args.synrgb_mode;
    opts.size = size_opt;
    opts.pad = args.pad;
    opts.target_crs = args.target_crs.clone();
    opts.resample_alg = args.resample_alg.clone();

    // Phase 1 flags: set STAC/IO hints; functionality comes in later steps
    opts.stac.in_path = args.stac_item.as_ref().map(|s| std::path::PathBuf::from(s));
    opts.io.remote = args.safe_zip_url.is_some()
        || args.vv_url.is_some()
        || args.vh_url.is_some()
        || args.hh_url.is_some()
        || args.hv_url.is_some()
        || opts.stac.in_path.is_some()
        || args.input_list.is_some();
    opts.io.cache_dir = args.remote_cache_dir.clone();
    // Derive zip strategy from presence of cache dir
    opts.io.compute_zip_strategy();
    // Map HTTP runtime options
    opts.http.timeout_s = args.http_timeout;
    opts.http.retries = args.http_retries as u8;
    opts.http.backoff_ms = args.http_backoff_ms;

    opts
}
