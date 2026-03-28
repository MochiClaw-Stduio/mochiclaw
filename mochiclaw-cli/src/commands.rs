//! CLI commands

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;

use mochiclaw_config::{ChannelConfig, Config};
use mochiclaw_core::{AgentLoop, ContextBuilder, MessageBus, PluginHost, PluginManifest, discover};
use mochiclaw_sdk::channel::{LoginParams, LoginResponse, QrStatusParams, QrStatusResponse};

pub async fn start(config_path: PathBuf) -> Result<()> {
    let config = Config::from_file(&config_path)?;

    tracing::info!("loaded config from {}", config_path.display());
    tracing::info!("agent model: {}", config.agent.model);

    // Create plugin host with optional fallback proxy from HTTP_PROXY
    let fallback_proxy = if config.runtime.network.use_system_proxy {
        std::env::var("HTTP_PROXY").ok()
    } else {
        None
    };
    tracing::info!("use_system_proxy={}, fallback_proxy={:?}", config.runtime.network.use_system_proxy, fallback_proxy);
    let plugin_host = Arc::new(tokio::sync::Mutex::new(
        PluginHost::new().with_http_proxy(fallback_proxy, config.runtime.network.use_system_proxy)
    ));

    // Discover and load plugins based on features
    for dir in &config.runtime.plugin_dirs {
        let plugin_base_dir = PathBuf::from(dir);
        tracing::info!("scanning for plugins in {}", plugin_base_dir.display());

        let discovered = match discover(&plugin_base_dir) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("failed to scan plugin directory: {}", e);
                continue;
            }
        };

        let mut host = plugin_host.lock().await;
        for plugin in discovered {
            // Get per-plugin proxy_url if configured
            let proxy_url = config
                .plugins
                .get(&plugin.name)
                .and_then(|p| p.proxy_url.clone());

            match host.load_discovered_with_proxy(plugin, proxy_url) {
                Ok(()) => {}
                Err(e) => {
                    tracing::warn!("failed to load plugin: {}", e);
                }
            }
        }
    }

    tracing::info!("loaded {} plugins", plugin_host.lock().await.plugin_count());

    // Create message bus
    let bus = Arc::new(MessageBus::new());

    // Create agent loop with workspace from config
    let workspace = config.workspace_path(&config_path);

    // Release templates to workspace at startup
    ContextBuilder::new(workspace.clone(), None).release_templates();

    let agent = AgentLoop::new(bus.clone(), plugin_host.clone(), &config, workspace);

    // Run agent
    let agent_handle = tokio::spawn(async move {
        if let Err(e) = agent.run().await {
            tracing::error!("agent error: {}", e);
        }
    });

    // Keep running
    tokio::signal::ctrl_c().await?;

    tracing::info!("shutting down...");
    Ok(())
}

pub async fn onboard(config_path: PathBuf) -> Result<()> {
    println!("Mochiclaw Onboarding");
    println!("===================");
    println!();

    let config = Config::default_for_onboarding();
    config.save(&config_path)?;
    println!("created default config at {}", config_path.display());

    // Release templates to workspace
    let workspace = config.workspace_path(&config_path);
    ContextBuilder::new(workspace, None).release_templates();
    println!("released templates to workspace");

    Ok(())
}

/// Login to a channel plugin
pub async fn login(plugin_name: &str, config_path: PathBuf) -> Result<()> {
    // Load config, create default if not exists
    let mut config = if config_path.exists() {
        Config::from_file(&config_path)?
    } else {
        create_default_config()?
    };

    // Find plugin paths
    let wasm_name = plugin_name.replace("mochiclaw-", "mochiclaw_");
    let wasm_path =
        PathBuf::from("target/wasm32-unknown-unknown/release").join(format!("{}.wasm", wasm_name));
    let manifest_path = PathBuf::from("plugins")
        .join(plugin_name)
        .join("manifest.toml");

    if !wasm_path.exists() {
        anyhow::bail!(
            "plugin '{}' not found at {}. Run 'just build' first",
            plugin_name,
            wasm_path.display()
        );
    }

    // Load manifest (required)
    let manifest = PluginManifest::from_file(&manifest_path)
        .map_err(|e| anyhow::anyhow!("failed to load manifest for '{}': {}", plugin_name, e))?;

    // Load plugin
    let mut plugin_host = PluginHost::new();
    plugin_host.load_plugin(plugin_name, &wasm_path, &manifest, None)?;
    tracing::info!("loaded plugin '{}'", plugin_name);

    // Call login function with empty config
    let resp: LoginResponse = {
        let login_params = LoginParams { config: Vec::new() };
        plugin_host.call(plugin_name, "login", &login_params)?
    };

    match resp.status.as_str() {
        "logged_in" => {
            if let Some(ref token) = resp.token {
                tracing::info!(
                    "already logged in, token: {}...",
                    &token[..8.min(token.len())]
                );
                save_token_to_config(&mut config, plugin_name, &resp, &config_path)?;
            }
        }
        "need_qr" => {
            if let Some(qr_url) = &resp.qr_url {
                println!();
                println!("========== {} Login ==========", plugin_name);
                println!();
                println!("Scan this QR code with the channel app:");
                println!();
                println!("{}", qr_url);
                println!();
                println!("Waiting for scan...");

                // Poll check_login
                if let Some(temp_token) = &resp.temp_token {
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                        let check_resp: QrStatusResponse = {
                            let params = QrStatusParams {
                                temp_token: temp_token.clone(),
                            };
                            plugin_host.call(plugin_name, "check_login", &params)?
                        };

                        match check_resp.status.as_str() {
                            "confirmed" => {
                                println!();
                                println!("Login successful!");

                                // Save token
                                let mut login_resp = resp.clone();
                                login_resp.token = check_resp.token;
                                login_resp.base_url = check_resp.base_url;
                                login_resp.status = "logged_in".to_string();
                                save_token_to_config(
                                    &mut config,
                                    plugin_name,
                                    &login_resp,
                                    &config_path,
                                )?;
                                break;
                            }
                            "scaned" => {
                                println!("QR code scanned, waiting for confirmation...");
                            }
                            "expired" => {
                                println!("QR code expired, please run login again");
                                break;
                            }
                            "error" => {
                                println!("Error: {:?}", check_resp.error);
                                break;
                            }
                            _ => {
                                // still waiting
                            }
                        }
                    }
                }
            }
        }
        "error" => {
            anyhow::bail!("login error: {:?}", resp.error);
        }
        _ => {
            anyhow::bail!("unknown login status: {}", resp.status);
        }
    }

    Ok(())
}

fn save_token_to_config(
    config: &mut Config,
    plugin_name: &str,
    resp: &LoginResponse,
    config_path: &PathBuf,
) -> Result<()> {
    if let Some(token) = &resp.token {
        let mut extra = std::collections::HashMap::new();
        if let Some(base_url) = &resp.base_url {
            extra.insert("base_url".to_string(), serde_json::json!(base_url));
        }

        let channel_config = ChannelConfig {
            enabled: true,
            token: Some(token.clone()),
            extra,
        };
        config
            .channels
            .insert(plugin_name.to_string(), channel_config);
        config.save(config_path)?;
        println!("token saved to config");
    }
    Ok(())
}

fn create_default_config() -> Result<Config> {
    Ok(Config::default_for_onboarding())
}
