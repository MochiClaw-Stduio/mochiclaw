//! CLI commands

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::logging_utils::{cleanup_old_logs, resolve_log_dir};

use mochiclaw_config::{ChannelConfig, Config};
use mochiclaw_core::{
    AgentLoop, AsyncHttpExecutor, ContextBuilder, LambdaHost, MessageBus, discover,
};
use mochiclaw_sdk::lambda::{
    Action, CheckLoginInput, CheckLoginOutput, Effect, LambdaInput, LambdaOutput, LoginInput,
    LoginOutput,
};

pub async fn start(config: Config, config_path: PathBuf) -> Result<()> {
    tracing::info!("loaded config from {}", config_path.display());
    tracing::info!("agent model: {}", config.agent.model);

    // Clean up old log files if max_age_days is configured
    if let Some(max_age_days) = config.runtime.log.max_age_days
        && let Some(log_dir) = resolve_log_dir(config.runtime.log.dir.as_deref(), &config_path)
    {
        cleanup_old_logs(&log_dir, max_age_days);
    }

    // Create lambda host with optional fallback proxy from HTTP_PROXY
    let fallback_proxy = if config.runtime.network.use_system_proxy {
        std::env::var("HTTP_PROXY").ok()
    } else {
        None
    };
    tracing::info!(
        "use_system_proxy={}, fallback_proxy={:?}",
        config.runtime.network.use_system_proxy,
        fallback_proxy
    );
    let mut lambda_host =
        LambdaHost::new().with_http_proxy(fallback_proxy, config.runtime.network.use_system_proxy);

    // Discover and load lambdas based on features
    for dir in &config.runtime.lambda_dirs {
        let lambda_base_dir = PathBuf::from(dir);
        tracing::info!("scanning for lambdas in {}", lambda_base_dir.display());

        let discovered = match discover(&lambda_base_dir) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("failed to scan lambda directory: {}", e);
                continue;
            }
        };

        let host = &mut lambda_host;
        for lambda in discovered {
            // Get per-lambda config if configured
            let lambda_config = config
                .lambdas
                .get(&lambda.manifest.name)
                .cloned()
                .unwrap_or_default();

            match host.load_lambda(
                &lambda.manifest.name,
                &lambda.wasm_path,
                &lambda.manifest,
                &lambda_config,
            ) {
                Ok(()) => {}
                Err(e) => {
                    tracing::warn!("failed to load lambda: {}", e);
                }
            }
        }
    }

    tracing::info!("loaded {} lambdas", lambda_host.lambda_count());

    // Create message bus
    let bus = Arc::new(MessageBus::new());

    // Create agent loop with workspace from config
    let workspace = config.workspace_path(&config_path);

    // Release templates to workspace at startup
    ContextBuilder::new(workspace.clone()).release_templates();

    let agent = AgentLoop::new(bus.clone(), Arc::new(lambda_host), &config, workspace);

    // Run agent
    let _agent_handle = tokio::spawn(async move {
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
    ContextBuilder::new(workspace).release_templates();
    println!("released templates to workspace");

    Ok(())
}

/// Login to a channel lambda using lambda_function
pub async fn login(lambda_name: &str, mut config: Config, config_path: PathBuf) -> Result<()> {
    // Discover lambdas from configured lambda directories
    let mut discovered_lambda = None;
    for dir in &config.runtime.lambda_dirs {
        let lambda_base_dir = PathBuf::from(dir);
        match discover(&lambda_base_dir) {
            Ok(discovered) => {
                if let Some(p) = discovered
                    .into_iter()
                    .find(|p| p.manifest.name == lambda_name)
                {
                    discovered_lambda = Some(p);
                    break;
                }
            }
            Err(e) => {
                tracing::warn!("failed to scan lambda directory {}: {}", dir, e);
            }
        }
    }

    let (manifest, wasm_path) = match discovered_lambda {
        Some(p) => (p.manifest, p.wasm_path),
        None => {
            anyhow::bail!(
                "lambda '{}' not found in configured lambda_dirs: {:?}",
                lambda_name,
                config.runtime.lambda_dirs
            );
        }
    };

    // Load lambda using manifest name
    let mut lambda_host = LambdaHost::new();
    lambda_host.load_lambda(&manifest.name, &wasm_path, &manifest, &Default::default())?;
    tracing::info!("loaded lambda '{}'", manifest.name);

    // Wrap in Arc for HTTP executor
    let lambda_host = Arc::new(lambda_host);

    // Create HTTP executor for this lambda
    let http_executor = AsyncHttpExecutor::new(Arc::clone(&lambda_host));

    // Get lambda config for login
    let lambda_config = config
        .lambdas
        .get(&manifest.name)
        .cloned()
        .unwrap_or_default();

    // Step 1: Call lambda_function with Action::Login
    let mut current_state = Vec::new();
    let login_input = LambdaInput {
        version: 1,
        action: Action::Login,
        state: current_state.clone(),
        payload: rmp_serde::to_vec(&LoginInput {
            config: serde_json::to_vec(&lambda_config)?,
        })?,
        effect_results: Vec::new(),
    };

    let login_output: LambdaOutput =
        lambda_host.call(&manifest.name, "lambda_function", &login_input)?;

    // Use loop mechanism: if effects returned, execute and call again with effect_results
    let login_result = if !login_output.effects.is_empty() {
        // Execute HTTP effect
        let Effect::HttpRequest(effect) = &login_output.effects[0];
        let raw_response = http_executor
            .execute(&manifest.name, effect.clone())
            .await?;

        // Loop: call Login again with effect_results
        current_state = login_output.new_state;
        let login_input = LambdaInput {
            version: 1,
            action: Action::Login,
            state: current_state.clone(),
            payload: Vec::new(),
            effect_results: vec![mochiclaw_sdk::lambda::EffectResult {
                success: true,
                response: Some(raw_response),
                error: None,
            }],
        };

        let login_output: LambdaOutput =
            lambda_host.call(&manifest.name, "lambda_function", &login_input)?;
        rmp_serde::from_slice::<LoginOutput>(&login_output.result)?
    } else {
        // No effects means we got LoginOutput directly (already logged in or error)
        rmp_serde::from_slice::<LoginOutput>(&login_output.result)?
    };

    match login_result.status.as_str() {
        "logged_in" => {
            if let Some(ref token) = login_result.token {
                tracing::info!(
                    "already logged in, token: {}...",
                    &token[..8.min(token.len())]
                );
                save_login_to_config(&mut config, &manifest.name, &login_result, &config_path)?;
            }
        }
        "need_qr" => {
            if let Some(qr_url) = &login_result.qr_url {
                println!();
                println!("========== {} Login ==========", manifest.name);
                println!();
                println!("Scan this QR code with the channel app:");
                println!();
                println!("{}", qr_url);
                println!();
                println!("Waiting for scan...");

                // Poll check_login until confirmed
                if let Some(temp_token) = &login_result.temp_token {
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                        // Call lambda_function with Action::CheckLogin
                        let check_input = LambdaInput {
                            version: 1,
                            action: Action::CheckLogin,
                            state: Vec::new(),
                            payload: rmp_serde::to_vec(&CheckLoginInput {
                                temp_token: temp_token.clone(),
                            })?,
                            effect_results: Vec::new(),
                        };

                        let check_output: LambdaOutput =
                            lambda_host.call(&manifest.name, "lambda_function", &check_input)?;

                        // Use loop mechanism
                        let check_result = if !check_output.effects.is_empty() {
                            // Execute HTTP effect
                            let Effect::HttpRequest(effect) = &check_output.effects[0];
                            let raw_response = http_executor
                                .execute(&manifest.name, effect.clone())
                                .await?;

                            // Loop: call CheckLogin again with effect_results
                            let check_input = LambdaInput {
                                version: 1,
                                action: Action::CheckLogin,
                                state: check_output.new_state,
                                payload: Vec::new(),
                                effect_results: vec![mochiclaw_sdk::lambda::EffectResult {
                                    success: true,
                                    response: Some(raw_response),
                                    error: None,
                                }],
                            };

                            let check_output: LambdaOutput = lambda_host.call(
                                &manifest.name,
                                "lambda_function",
                                &check_input,
                            )?;
                            rmp_serde::from_slice::<CheckLoginOutput>(&check_output.result)?
                        } else {
                            anyhow::bail!("check_login returned no result");
                        };

                        match check_result.status.as_str() {
                            "confirmed" => {
                                println!();
                                println!("Login successful!");

                                // Save token
                                let final_login_result = LoginOutput {
                                    status: "logged_in".to_string(),
                                    qr_url: login_result.qr_url,
                                    temp_token: login_result.temp_token,
                                    token: check_result.token,
                                    base_url: check_result.base_url,
                                    error: None,
                                };
                                save_login_to_config(
                                    &mut config,
                                    lambda_name,
                                    &final_login_result,
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
                                println!("Error: {:?}", check_result.error);
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
            anyhow::bail!("login error: {:?}", login_result.error);
        }
        _ => {
            anyhow::bail!("unknown login status: {}", login_result.status);
        }
    }

    Ok(())
}

fn save_login_to_config(
    config: &mut Config,
    lambda_name: &str,
    resp: &LoginOutput,
    config_path: &Path,
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
            .insert(lambda_name.to_string(), channel_config);
        config.save(config_path)?;
        println!("token saved to config");
    }
    Ok(())
}
