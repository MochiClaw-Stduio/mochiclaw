//! Agent Loop - core orchestration logic

use crate::bus::MessageBus;
use crate::commands::{CommandRegistry, parse_command};
use crate::config::{ChannelConfig, Config, ModelConfig};
use crate::error::Error;
use crate::session::SessionManager;
use mochiclaw_plugin::PluginHost;
use mochiclaw_sdk::message::InboundMessage;
use mochiclaw_sdk::provider::{ChatRequest, ChatResponse, Message, MessageRole};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::{interval, timeout};

pub struct AgentLoop {
    bus: Arc<MessageBus>,
    plugin_host: Arc<tokio::sync::Mutex<PluginHost>>,
    model_config: ModelConfig,
    max_iterations: usize,
    /// Channel configurations for polling (channel_name -> config)
    channel_configs: HashMap<String, ChannelConfig>,
    /// Per-channel polling state (get_updates_buf only, context_token is handled by plugin)
    poll_state: tokio::sync::Mutex<HashMap<String, String>>,
    /// Session manager for conversation history (uses Mutex for interior mutability)
    sessions: Mutex<SessionManager>,
    /// Command registry for slash commands
    commands: CommandRegistry,
}

#[derive(Debug, Deserialize)]
struct PollResponse {
    messages: Vec<InboundMessage>,
    #[serde(default)]
    get_updates_buf: String,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SendResponse {
    success: bool,
    #[serde(default)]
    error: Option<String>,
}

impl AgentLoop {
    pub fn new(
        bus: Arc<MessageBus>,
        plugin_host: Arc<tokio::sync::Mutex<PluginHost>>,
        config: &Config,
        workspace: PathBuf,
    ) -> Self {
        // Extract channel configs that have tokens (logged in)
        let channel_configs: HashMap<String, ChannelConfig> = config
            .channels
            .iter()
            .filter(|(_, cfg)| cfg.extra.get("token").is_some())
            .map(|(name, cfg)| (name.clone(), cfg.clone()))
            .collect();

        tracing::info!(
            "AgentLoop initializing with {} logged-in channels: {:?}",
            channel_configs.len(),
            channel_configs.keys().collect::<Vec<_>>()
        );

        // Get model config
        let model_config = config
            .models
            .get(&config.agent.model)
            .cloned()
            .unwrap_or_else(|| {
                tracing::warn!(
                    "model '{}' not found in config, using default",
                    config.agent.model
                );
                ModelConfig {
                    model: "gpt-4".to_string(),
                    provider: "mochiclaw-openai".to_string(),
                    api_base: None,
                    api_key: None,
                }
            });

        tracing::info!(
            "Using model '{}' (provider: {})",
            model_config.model,
            model_config.provider
        );

        Self {
            bus,
            plugin_host,
            model_config,
            max_iterations: config.agent.max_iterations,
            channel_configs,
            poll_state: tokio::sync::Mutex::new(HashMap::new()),
            sessions: Mutex::new(SessionManager::new(workspace.join("sessions"))),
            commands: CommandRegistry::new(),
        }
    }

    pub async fn run(&self) -> Result<(), Error> {
        tracing::info!("AgentLoop started");

        let mut poll_interval = interval(Duration::from_secs(2));

        loop {
            // Poll channel plugins for new messages
            self.poll_channels().await;

            // Process any inbound messages from the bus
            match timeout(Duration::from_millis(100), self.bus.recv_inbound()).await {
                Ok(Some(msg)) => {
                    if let Err(e) = self.process_message(msg).await {
                        tracing::error!("failed to process message: {}", e);
                    }
                }
                Ok(None) => {
                    tracing::debug!("inbound channel closed");
                    break;
                }
                Err(_) => {
                    // timeout, continue to poll
                }
            }

            poll_interval.tick().await;
        }

        tracing::info!("AgentLoop stopped");
        Ok(())
    }

    /// Poll all configured channel plugins for new messages
    async fn poll_channels(&self) {
        for (channel_name, config) in &self.channel_configs {
            if !config.enabled {
                continue;
            }

            // Get token from config extra
            let token = match config.extra.get("token").and_then(|v| v.as_str()) {
                Some(t) => t,
                None => {
                    tracing::warn!("no token found for channel {}", channel_name);
                    continue;
                }
            };

            tracing::debug!(
                "polling channel {} with token length {}",
                channel_name,
                token.len()
            );

            let get_updates_buf = {
                let state = self.poll_state.lock().await;
                state.get(channel_name).cloned().unwrap_or_default()
            };

            let poll_params = serde_json::json!({
                "token": token,
                "get_updates_buf": get_updates_buf,
            });

            // Call the channel plugin's poll function
            let result: Result<PollResponse, _> = {
                let host = self.plugin_host.lock().await;
                let output = match host.call(channel_name, "poll", &poll_params.to_string()) {
                    Ok(o) => {
                        tracing::trace!(
                            "poll raw output for {} (len={}): {}",
                            channel_name,
                            o.len(),
                            &o
                        );
                        o
                    }
                    Err(e) => {
                        tracing::warn!("poll call failed for {}: {}", channel_name, e);
                        continue;
                    }
                };
                serde_json::from_str(&output).map_err(|e| {
                    tracing::warn!("failed to parse poll response for {}: {}", channel_name, e);
                    e
                })
            };

            match result {
                Ok(resp) => {
                    let buf_len = resp.get_updates_buf.len();
                    if let Some(ref err) = resp.error {
                        tracing::info!(
                            "poll {}: {} messages (buf={}), debug: {}",
                            channel_name,
                            resp.messages.len(),
                            buf_len,
                            err
                        );
                    } else {
                        tracing::debug!(
                            "poll {}: {} messages (buf={})",
                            channel_name,
                            resp.messages.len(),
                            buf_len
                        );
                    }

                    // Update poll state (just the get_updates_buf cursor)
                    // Note: context_token is now handled internally by the channel plugin
                    if !resp.get_updates_buf.is_empty() {
                        let mut state = self.poll_state.lock().await;
                        state.insert(channel_name.clone(), resp.get_updates_buf);
                    }

                    // Send each message to the bus
                    for mut inbound in resp.messages {
                        // Ensure channel is set correctly (plugin may not set it)
                        inbound.channel = channel_name.clone();

                        tracing::debug!(
                            "received message from {} in chat {}: {}...",
                            inbound.sender_id,
                            inbound.chat_id,
                            inbound.content.chars().take(50).collect::<String>()
                        );

                        if let Err(e) = self.bus.send_inbound(inbound).await {
                            tracing::error!("failed to send inbound: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::debug!("failed to parse poll response for {}: {}", channel_name, e);
                }
            }
        }
    }

    async fn process_message(&self, msg: InboundMessage) -> Result<(), Error> {
        // HIGHEST PRIORITY: Check for slash commands BEFORE any other processing
        if let Some((cmd_name, _args)) = parse_command(&msg.content) {
            match cmd_name.as_str() {
                "help" => {
                    let help_text = self.commands.generate_help();
                    self.send_to_channel(&msg.channel, &msg.chat_id, &help_text)
                        .await?;
                    return Ok(());
                }
                "clear" => {
                    let session_key = msg.session_key();
                    {
                        let mut sessions = self.sessions.lock().unwrap();
                        let session = sessions.get_or_create(&session_key);
                        session.clear();
                        if let Err(e) = sessions.save(&session_key) {
                            tracing::warn!("failed to save session after /clear: {}", e);
                        }
                    }
                    self.send_to_channel(&msg.channel, &msg.chat_id, "Conversation cleared.")
                        .await?;
                    return Ok(());
                }
                _ => {
                    // Try built-in commands from registry
                    if let Some((name, args)) = parse_command(&msg.content) {
                        if let Some(text) =
                            self.commands
                                .execute(&msg.channel, &msg.chat_id, &name, &args)
                        {
                            self.send_to_channel(&msg.channel, &msg.chat_id, &text)
                                .await?;
                            return Ok(());
                        }
                    }
                    // Not a known command - fall through to normal processing (send to LLM)
                }
            }
        }

        tracing::info!("processing message from {}: {}", msg.channel, msg.content);

        // Get or create session for this conversation
        let session_key = msg.session_key();
        let history = {
            let mut sessions = self.sessions.lock().unwrap();
            let session = sessions.get_or_create(&session_key);
            session.add_message("user", &msg.content);
            session.get_history(500)
        };

        // Build chat messages from history
        let chat_messages: Vec<Message> = history
            .into_iter()
            .map(|m| Message {
                role: match m.role.as_str() {
                    "system" => MessageRole::System,
                    "assistant" => MessageRole::Assistant,
                    _ => MessageRole::User,
                },
                content: m.content,
            })
            .collect();

        // Build chat request for provider
        let chat_request = ChatRequest {
            model: self.model_config.model.clone(),
            messages: chat_messages,
            tools: Vec::new(),
            max_tokens: 4096,
            temperature: 0.7,
            api_key: self.model_config.api_key.clone(),
            api_base: self.model_config.api_base.clone(),
        };

        // Call provider plugin
        let response = {
            let host = self.plugin_host.lock().await;
            let request_json = serde_json::to_string(&chat_request)
                .map_err(|e| Error::Plugin(format!("failed to serialize chat request: {}", e)))?;

            let output = host
                .call(&self.model_config.provider, "chat", &request_json)
                .map_err(|e| Error::Plugin(format!("provider call failed: {}", e)))?;

            let resp: ChatResponse = serde_json::from_str(&output)
                .map_err(|e| Error::Plugin(format!("failed to parse chat response: {}", e)))?;

            if let Some(err) = resp.error {
                return Err(Error::Plugin(format!("provider error: {}", err)));
            }

            resp.content
        };

        tracing::info!(
            "got response: {}...",
            response.chars().take(100).collect::<String>()
        );

        // Add assistant response to session and save
        {
            let mut sessions = self.sessions.lock().unwrap();
            let session = sessions.get_or_create(&session_key);
            session.add_message("assistant", &response);
            if let Err(e) = sessions.save(&session_key) {
                tracing::warn!("failed to save session: {}", e);
            }
        }

        self.send_to_channel(&msg.channel, &msg.chat_id, &response)
            .await?;

        Ok(())
    }

    /// Send a text message to a channel
    async fn send_to_channel(
        &self,
        channel_name: &str,
        chat_id: &str,
        content: &str,
    ) -> Result<(), Error> {
        // Get token from channel config
        let token = match self.channel_configs.get(channel_name) {
            Some(cfg) => cfg.extra.get("token").and_then(|v| v.as_str()),
            None => None,
        };

        let token = match token {
            Some(t) => t,
            None => {
                tracing::warn!("no token for channel {}", channel_name);
                return Ok(());
            }
        };

        tracing::info!(
            "sending to channel {} chat_id {}: {}",
            channel_name,
            chat_id,
            content
        );

        // Note: context_token is handled internally by the channel plugin
        let send_params = serde_json::json!({
            "token": token,
            "to_user_id": chat_id,
            "content": content,
        });

        let host = self.plugin_host.lock().await;
        let output = host
            .call(channel_name, "send_text", &send_params.to_string())
            .map_err(|e| Error::Plugin(format!("send_text failed: {}", e)))?;

        tracing::debug!("send_text raw response: {}", output);

        let resp: SendResponse = serde_json::from_str(&output)
            .map_err(|e| Error::Plugin(format!("invalid send_text response: {}", e)))?;

        if !resp.success {
            tracing::warn!("send_text failed: {:?}", resp.error);
        } else {
            tracing::info!("message sent successfully to {}", chat_id);
            tracing::debug!("sent message to {}", chat_id);
        }

        Ok(())
    }
}
