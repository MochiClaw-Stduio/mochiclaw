//! Mochiclaw CLI - Entry point

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

mod commands;

#[derive(Parser)]
#[command(name = "mochiclaw")]
#[command(about = "Mochiclaw - Extensible AI Agent Framework", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the agent
    Start {
        #[arg(default_value = "config.toml")]
        config: PathBuf,
    },
    /// Interactive onboarding
    Onboard {
        #[arg(default_value = "config.toml")]
        config: PathBuf,
    },
    /// Login to a channel plugin (e.g., weixin)
    Login {
        /// Plugin name (e.g., mochiclaw-weixin)
        plugin: String,
        /// Config file path
        #[arg(default_value = "config.toml")]
        config: PathBuf,
    },
    /// Show version
    Version,
}

/// Build log level filter: RUST_LOG env > config value > default "info"
fn build_log_filter(config_log_level: Option<&str>) -> EnvFilter {
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
fn setup_tracing(log_dir: Option<&str>, log_level: Option<&str>, config_dir: &Path) {
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
        let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_path, "mochiclaw.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        // Leak the guard to keep file logging alive for the duration of the program
        Box::leak(Box::new(guard));

        subscriber.with(tracing_subscriber::fmt::layer().with_writer(non_blocking).with_ansi(false)).init();
    } else {
        subscriber.init();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { config } => {
            let config_path = config;
            let config = mochiclaw_config::Config::from_file(&config_path)?;
            let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
            setup_tracing(
                config.runtime.log.dir.as_deref(),
                Some(&config.runtime.log.level),
                config_dir,
            );
            commands::start(config, config_path).await
        }
        Commands::Onboard { config } => commands::onboard(config).await,
        Commands::Login { plugin, config } => {
            let config_path = config;
            let config = if config_path.exists() {
                mochiclaw_config::Config::from_file(&config_path)?
            } else {
                mochiclaw_config::Config::default_for_onboarding()
            };
            let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
            setup_tracing(
                config.runtime.log.dir.as_deref(),
                Some(&config.runtime.log.level),
                config_dir,
            );
            commands::login(&plugin, config, config_path).await
        }
        Commands::Version => {
            println!("mochiclaw {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
