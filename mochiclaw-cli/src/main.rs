//! Mochiclaw CLI - Entry point

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Start { config } => commands::start(config).await,
        Commands::Onboard { config } => commands::onboard(config).await,
        Commands::Login { plugin, config } => commands::login(&plugin, config).await,
        Commands::Version => {
            println!("mochiclaw {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
