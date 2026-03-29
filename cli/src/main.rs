//! Mochiclaw CLI - Entry point

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

mod commands;
mod logging;
mod logging_utils;

const DEFAULT_CONFIG_PATH: &str = "config.toml";

fn get_config_path() -> PathBuf {
    std::env::var("MOCHICLAW_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_CONFIG_PATH))
}

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
        /// Config file path (default: $MOCHICLAW_CONFIG or config.toml)
        #[arg(default_value = None)]
        config: Option<PathBuf>,
    },
    /// Interactive onboarding
    Onboard {
        /// Config file path (default: $MOCHICLAW_CONFIG or config.toml)
        #[arg(default_value = None)]
        config: Option<PathBuf>,
    },
    /// Login to a channel lambda (e.g., weixin)
    Login {
        /// Lambda name (e.g., mochiclaw-weixin)
        lambda: String,
        /// Config file path (default: $MOCHICLAW_CONFIG or config.toml)
        #[arg(default_value = None)]
        config: Option<PathBuf>,
    },
    /// Show version
    Version,
}

fn resolve_config_path(config: Option<PathBuf>) -> PathBuf {
    config.unwrap_or_else(get_config_path)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { config } => {
            let config_path = resolve_config_path(config);
            let config = mochiclaw_config::Config::from_file(&config_path)?;
            let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
            logging::setup_tracing(
                config.runtime.log.dir.as_deref(),
                Some(&config.runtime.log.level),
                config_dir,
            );
            commands::start(config, config_path).await
        }
        Commands::Onboard { config } => commands::onboard(resolve_config_path(config)).await,
        Commands::Login { lambda, config } => {
            let config_path = resolve_config_path(config);
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
            commands::login(&lambda, config, config_path).await
        }
        Commands::Version => {
            println!("mochiclaw {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
