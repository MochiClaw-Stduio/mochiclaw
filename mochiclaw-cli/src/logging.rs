//! Logging setup utilities

use std::path::{Path, PathBuf};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

/// Build log level filter: RUST_LOG env > config value > default "info"
pub fn build_log_filter(config_log_level: Option<&str>) -> EnvFilter {
    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        // RUST_LOG env var takes highest priority
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(rust_log))
    } else if let Some(level) = config_log_level {
        EnvFilter::new(level)
    } else {
        EnvFilter::new("info")
    }
}

/// Set up tracing with stdout and optional file output
pub fn setup_tracing(log_dir: Option<&str>, log_level: Option<&str>, config_dir: &Path) {
    let env_filter = build_log_filter(log_level);

    let base = Registry::default().with(env_filter);

    // Always add stdout layer
    let subscriber = base.with(tracing_subscriber::fmt::layer());

    // Add file layer if log_dir is configured
    if let Some(dir) = log_dir {
        // Resolve log_dir relative to config file location
        let log_path = PathBuf::from(dir);
        let log_path = if log_path.is_absolute() {
            log_path
        } else {
            config_dir.join(log_path)
        };

        // Create rolling file appender (daily rotation)
        // Filename format: mochiclaw.2026-03-28.log
        let file_appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("mochiclaw")
            .filename_suffix("log")
            .build(&log_path)
            .expect("failed to create rolling file appender");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        // Leak the guard to keep file logging alive for the duration of the program
        Box::leak(Box::new(guard));

        subscriber.with(tracing_subscriber::fmt::layer().with_writer(non_blocking).with_ansi(false)).init();
    } else {
        subscriber.init();
    }
}