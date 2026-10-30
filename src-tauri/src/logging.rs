//! `tracing` setup: stdout, a daily-rotated INFO+ log, and a
//! daily-rotated WARN+ log. Per-run files live in `logs/runs/` and are
//! written by `run_log::RunLog`, not by tracing (they need a per-run
//! handle, which tracing-appender does not support directly).

use std::sync::OnceLock;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

static GUARDS: OnceLock<Vec<WorkerGuard>> = OnceLock::new();

pub fn init() {
    let log_dir = std::path::Path::new("logs");
    let _ = std::fs::create_dir_all(log_dir);
    let _ = std::fs::create_dir_all(log_dir.join("runs"));

    let main_appender = tracing_appender::rolling::daily(log_dir, "main.txt");
    let errors_appender = tracing_appender::rolling::daily(log_dir, "errors.txt");

    let (main_writer, g1) = tracing_appender::non_blocking(main_appender);
    let (errors_writer, g2) = tracing_appender::non_blocking(errors_appender);

    // Keep both guards alive for the process lifetime.
    let _ = GUARDS.set(vec![g1, g2]);

    // Console: INFO+ by default (or RUST_LOG override).
    let console_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,script_verifier_lib=debug"));

    // Main file: everything the console sees.
    let main_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,script_verifier_lib=debug"));

    // Errors file: WARN+ only. This is what you grep after a bad run.
    let errors_filter = EnvFilter::new("warn");

    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_target(false);

    let main_layer = tracing_subscriber::fmt::layer()
        .with_writer(main_writer)
        .with_ansi(false)
        .with_target(true)
        .with_filter(main_filter);

    let errors_layer = tracing_subscriber::fmt::layer()
        .with_writer(errors_writer)
        .with_ansi(false)
        .with_target(true)
        .with_filter(errors_filter);

    let _ = tracing_subscriber::registry()
        .with(console_filter)
        .with(stdout_layer)
        .with(main_layer)
        .with(errors_layer)
        .try_init();

    tracing::info!("logging initialised: logs/main.txt.<date>, logs/errors.txt.<date>, logs/runs/*.txt");
}
