//! Mochiclaw CLI - Entry point

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

mod commands;
mod logging;
mod logging_utils;

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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { config } => {
            let config_path = config;
            let config = mochiclaw_config::Config::from_file(&config_path)?;
            let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
            logging::setup_tracing(
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
            logging::setup_tracing(
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
