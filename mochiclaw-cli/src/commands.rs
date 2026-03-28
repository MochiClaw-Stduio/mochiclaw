//! CLI commands

use anyhow::Result;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

use mochiclaw_core::{AgentLoop, Config, MessageBus, PluginHost, PluginManifest, discover};
use mochiclaw_sdk::channel::{LoginParams, LoginResponse, QrStatusParams, QrStatusResponse};

/// Deserialize a value from MessagePack bytes
fn from_msgpack<'a, T: serde::Deserialize<'a>>(buf: &'a [u8]) -> Option<T> {
    T::deserialize(&mut Deserializer::new(Cursor::new(buf))).ok()
}

/// Serialize a value to MessagePack bytes
fn to_msgpack<T: Serialize>(value: &T) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    value.serialize(&mut Serializer::new(&mut buf)).ok()?;
    Some(buf)
}

pub async fn start(config_path: PathBuf) -> Result<()> {
    let config = Config::from_file(&config_path)?;

    tracing::info!("loaded config from {}", config_path.display());
    tracing::info!("agent model: {}", config.agent.model);

    // Create plugin host
    let plugin_host = Arc::new(tokio::sync::Mutex::new(PluginHost::new()));

    // Discover and load plugins based on features
    for dir in &config.plugins.plugin_dirs {
        let plugin_base_dir = PathBuf::from(&dir.path);
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
            match host.load_discovered(plugin) {
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

    let config = Config {
        agent: mochiclaw_core::config::AgentConfig {
            model: "gpt-4".to_string(),
            max_iterations: 40,
            workspace: ".".to_string(),
        },
        plugins: mochiclaw_core::config::PluginsConfig {
            plugin_dirs: vec![mochiclaw_core::config::PluginDirConfig {
                path: "./plugins".to_string(),
            }],
        },
        channels: std::collections::HashMap::new(),
        models: std::collections::HashMap::from([(
            "gpt-4".to_string(),
            mochiclaw_core::config::ModelConfig {
                model: "gpt-4".to_string(),
                provider: "mochiclaw-openai".to_string(),
                api_base: None,
                api_key: None,
            },
        )]),
    };

    config.save(&config_path)?;
    println!("created default config at {}", config_path.display());
    println!();
    println!("Next steps:");
    println!("1. Build plugins: cd plugins/mochiclaw-weixin && cargo build --release");
    println!("2. Build plugins: cd plugins/mochiclaw-openai && cargo build --release");
    println!("3. Run: mochiclaw start");

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
    plugin_host.load_plugin(plugin_name, &wasm_path, &manifest)?;
    tracing::info!("loaded plugin '{}'", plugin_name);

    // Call login function with empty config
    let resp: LoginResponse = {
        let login_params = LoginParams { config: Vec::new() };
        let params_bytes = to_msgpack(&login_params).unwrap_or_default();
        let output = plugin_host.call(plugin_name, "login", &params_bytes)?;
        match from_msgpack(&output) {
            Some(r) => r,
            None => {
                // Fallback: try to parse as JSON for backwards compatibility
                serde_json::from_str(&String::from_utf8_lossy(&output)).unwrap_or(LoginResponse {
                    status: "error".to_string(),
                    qr_url: None,
                    temp_token: None,
                    token: None,
                    base_url: None,
                    error: Some("failed to parse response".to_string()),
                })
            }
        }
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
                            let params_bytes = to_msgpack(&params).unwrap_or_default();
                            let output =
                                plugin_host.call(plugin_name, "check_login", &params_bytes)?;
                            match from_msgpack(&output) {
                                Some(r) => r,
                                None => {
                                    // Fallback: try to parse as JSON for backwards compatibility
                                    serde_json::from_str(&String::from_utf8_lossy(&output))
                                        .unwrap_or(QrStatusResponse {
                                            status: "error".to_string(),
                                            token: None,
                                            base_url: None,
                                            error: Some("failed to parse response".to_string()),
                                        })
                                }
                            }
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
        use mochiclaw_core::config::ChannelConfig;

        let mut extra = std::collections::HashMap::new();
        extra.insert("token".to_string(), serde_json::json!(token));
        if let Some(base_url) = &resp.base_url {
            extra.insert("base_url".to_string(), serde_json::json!(base_url));
        }

        let channel_config = ChannelConfig {
            enabled: true,
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
    use mochiclaw_core::config::{AgentConfig, ModelConfig, PluginDirConfig, PluginsConfig};

    Ok(Config {
        agent: AgentConfig {
            model: "gpt-4".to_string(),
            max_iterations: 40,
            workspace: ".".to_string(),
        },
        plugins: PluginsConfig {
            plugin_dirs: vec![PluginDirConfig {
                path: "./plugins".to_string(),
            }],
        },
        channels: std::collections::HashMap::new(),
        models: std::collections::HashMap::from([(
            "gpt-4".to_string(),
            ModelConfig {
                model: "gpt-4".to_string(),
                provider: "mochiclaw-openai".to_string(),
                api_base: None,
                api_key: None,
            },
        )]),
    })
}
