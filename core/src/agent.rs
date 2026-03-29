//! Agent Loop - core orchestration logic

use crate::bus::MessageBus;
use crate::commands::{CommandRegistry, parse_command};
use crate::context::ContextBuilder;
use crate::error::Error;
use crate::http_executor::AsyncHttpExecutor;
use crate::lambda_loop::lambda_call_typed;
use crate::session::SessionManager;
use mochiclaw_config::{ChannelConfig, Config, ModelConfig};
use mochiclaw_lambda::PluginHost;
use mochiclaw_sdk::lambda::{
    Action, ChatInput, ExecuteToolInput, GetToolsInput, SendInput, SendOutput, SetTypingInput,
};
use mochiclaw_sdk::message::InboundMessage;
use mochiclaw_sdk::provider::{ChatRequest, ChatResponse, Message, MessageRole};
use mochiclaw_sdk::tool::{Tool, ToolExecutionResponse};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct AgentLoop {
    bus: Arc<MessageBus>,
    plugin_host: Arc<PluginHost>,
    http_executor: Arc<AsyncHttpExecutor>,
    model_config: ModelConfig,
    max_iterations: usize,
    /// Channel configurations for polling (channel_name -> config)
    channel_configs: HashMap<String, ChannelConfig>,
    /// Per-channel polling state (MessagePack bytes)
    poll_state: Arc<tokio::sync::Mutex<HashMap<String, Vec<u8>>>>,
    /// Session manager for conversation history (uses Mutex for interior mutability)
    sessions: Arc<Mutex<SessionManager>>,
    /// Command registry for slash commands
    commands: CommandRegistry,
    /// Context builder for system prompts
    context_builder: ContextBuilder,
    /// Available tool definitions from tool plugins
    tool_definitions: Arc<Mutex<Vec<Tool>>>,
    /// Mapping from tool name to plugin name that provides it
    tool_plugin_map: Arc<Mutex<HashMap<String, String>>>,
}

impl Clone for AgentLoop {
    fn clone(&self) -> Self {
        Self {
            bus: Arc::clone(&self.bus),
            plugin_host: Arc::clone(&self.plugin_host),
            http_executor: Arc::clone(&self.http_executor),
            model_config: self.model_config.clone(),
            max_iterations: self.max_iterations,
            channel_configs: self.channel_configs.clone(),
            poll_state: Arc::clone(&self.poll_state),
            sessions: Arc::clone(&self.sessions),
            commands: self.commands.clone(),
            context_builder: self.context_builder.clone(),
            tool_definitions: Arc::clone(&self.tool_definitions),
            tool_plugin_map: Arc::clone(&self.tool_plugin_map),
        }
    }
}

impl AgentLoop {
    pub fn new(
        bus: Arc<MessageBus>,
        plugin_host: Arc<PluginHost>,
        config: &Config,
        workspace: PathBuf,
    ) -> Self {
        // Extract channel configs that have tokens (logged in)
        let channel_configs: HashMap<String, ChannelConfig> = config
            .channels
            .iter()
            .filter(|(_, cfg)| cfg.token.is_some())
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

        let sessions_dir = workspace.join("sessions");

        Self {
            bus,
            plugin_host: Arc::clone(&plugin_host),
            http_executor: Arc::new(AsyncHttpExecutor::new(Arc::clone(&plugin_host))),
            model_config,
            max_iterations: config.agent.max_iterations,
            channel_configs,
            poll_state: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            sessions: Arc::new(Mutex::new(SessionManager::new(sessions_dir))),
            commands: CommandRegistry::new(),
            context_builder: ContextBuilder::new(workspace),
            tool_definitions: Arc::new(Mutex::new(Vec::new())),
            tool_plugin_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Discover and load tool definitions from all loaded tool plugins.
    /// Tool plugins are identified by having features.tool = true in their manifest.
    async fn load_tool_plugins(&self) {
        let tool_plugin_names = self.plugin_host.tool_plugins();

        let mut tool_definitions = Vec::new();
        let mut tool_plugin_map: HashMap<String, String> = HashMap::new();
        let mut loaded_plugin_count = 0;

        for plugin_name in tool_plugin_names {
            // Use lambda_call_typed to call GetTools action
            let tools_result: Result<String, Error> = lambda_call_typed(
                &self.plugin_host,
                Arc::clone(&self.http_executor),
                &plugin_name,
                Action::GetTools,
                &GetToolsInput {},
                &[],
            )
            .await
            .map_err(|e| Error::Plugin(e.to_string()));

            match tools_result {
                Ok(tools_json) => {
                    // Parse the tools JSON
                    match serde_json::from_str::<Vec<Tool>>(&tools_json) {
                        Ok(tools) => {
                            loaded_plugin_count += 1;
                            tracing::info!(
                                "loaded {} tools from plugin '{}'",
                                tools.len(),
                                plugin_name
                            );
                            for tool in tools {
                                tool_plugin_map.insert(tool.name.clone(), plugin_name.clone());
                                tool_definitions.push(tool);
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "failed to parse tools from plugin '{}': {}",
                                plugin_name,
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    // Tool plugin but GetTools failed - this is a real error
                    tracing::warn!(
                        "tool plugin '{}' failed to provide tools (GetTools failed): {}",
                        plugin_name,
                        e
                    );
                }
            }
        }

        // Update the agent state with discovered tools
        *self.tool_definitions.lock().unwrap() = tool_definitions;
        *self.tool_plugin_map.lock().unwrap() = tool_plugin_map;

        if self.tool_definitions.lock().unwrap().is_empty() {
            tracing::info!("no tool plugins loaded");
        } else {
            tracing::info!(
                "total tool definitions: {}, from {} tool plugin(s)",
                self.tool_definitions.lock().unwrap().len(),
                loaded_plugin_count
            );
        }
    }

    pub async fn run(&self) -> Result<(), Error> {
        tracing::info!("AgentLoop started");

        // Initialize tool plugins
        self.load_tool_plugins().await;

        // Spawn independent polling task for each channel
        let mut poller_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
        for (channel_name, config) in &self.channel_configs {
            if !config.enabled {
                continue;
            }
            if config.token.is_none() {
                tracing::warn!("no token for channel {}, skipping", channel_name);
                continue;
            }

            let poller = crate::poller::spawn_channel_poller(
                channel_name.clone(),
                config.clone(),
                Arc::clone(&self.bus),
                Arc::clone(&self.plugin_host),
                Arc::clone(&self.http_executor),
                Arc::clone(&self.poll_state),
            );
            poller_handles.push(poller);
        }

        tracing::info!("spawned {} channel poller tasks", poller_handles.len());

        // Pure consumer loop - only receives from bus and processes messages
        loop {
            match self.bus.recv_inbound().await {
                Some(msg) => {
                    // Spawn processing task for parallelism
                    let agent = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = agent.process_message(msg).await {
                            tracing::error!("failed to process message: {}", e);
                        }
                    });
                }
                None => {
                    tracing::debug!("inbound channel closed");
                    break;
                }
            }
        }

        // Graceful shutdown: abort poller tasks
        tracing::info!(
            "AgentLoop stopping - aborting {} poller tasks",
            poller_handles.len()
        );
        for handle in poller_handles {
            handle.abort();
        }

        tracing::info!("AgentLoop stopped");
        Ok(())
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
                    if let Some((name, args)) = parse_command(&msg.content)
                        && let Some(text) =
                            self.commands
                                .execute(&msg.channel, &msg.chat_id, &name, &args)
                    {
                        self.send_to_channel(&msg.channel, &msg.chat_id, &text)
                            .await?;
                        return Ok(());
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
            session.get_history(500)
        };

        // Build messages with system prompt using ContextBuilder
        let media_ref: Option<&[String]> = if msg.media.is_empty() {
            None
        } else {
            Some(&msg.media)
        };
        let context_messages = self.context_builder.build_messages(
            &history,
            &msg.content,
            media_ref,
            Some(&msg.channel),
            Some(&msg.chat_id),
            "user",
        );

        // Convert to provider messages
        let mut chat_messages: Vec<Message> = context_messages
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

        // Add user message to session history (will be updated with full content later)
        {
            let mut sessions = self.sessions.lock().unwrap();
            let session = sessions.get_or_create(&session_key);
            session.add_message("user", &msg.content);
        }

        // Send typing start indicator before LLM processing
        if let Some(cfg) = self.channel_configs.get(&msg.channel)
            && let Some(token) = &cfg.token
        {
            let typing_input = SetTypingInput {
                token: token.clone(),
                chat_id: msg.chat_id.clone(),
                typing: true,
            };
            let _: Option<mochiclaw_sdk::lambda::SendOutput> = lambda_call_typed(
                &self.plugin_host,
                Arc::clone(&self.http_executor),
                &msg.channel,
                Action::SetTyping,
                &typing_input,
                &[],
            )
            .await
            .ok();
        }

        // Main agent loop - handle tool calls iteratively
        let mut iterations = 0;
        let final_response = loop {
            iterations += 1;
            if iterations > self.max_iterations {
                tracing::warn!(
                    "max iterations ({}) reached for message from {}",
                    self.max_iterations,
                    msg.channel
                );
                break "I apologize, but I reached the maximum number of iterations. Please try again with a simpler request.".to_string();
            }

            // Build chat request for provider with current messages and tools
            let chat_request = ChatRequest {
                model: self.model_config.model.clone(),
                messages: chat_messages.clone(),
                tools: self.tool_definitions.lock().unwrap().clone(),
                max_tokens: 4096,
                temperature: 0.7,
                api_key: self.model_config.api_key.clone(),
                api_base: self.model_config.api_base.clone(),
            };

            // Call provider via unified lambda_loop - handles HTTP call + response parsing internally
            let chat_input = ChatInput {
                request: chat_request.clone(),
            };

            let response: ChatResponse = lambda_call_typed(
                &self.plugin_host,
                Arc::clone(&self.http_executor),
                &self.model_config.provider,
                Action::Chat,
                &chat_input,
                &[],
            )
            .await
            .map_err(|e| Error::Plugin(format!("provider call failed: {}", e)))?;

            if let Some(err) = response.error {
                return Err(Error::Plugin(format!("provider error: {}", err)));
            }

            // Check if there are tool calls to execute
            if response.tool_calls.is_empty() {
                // No tool calls, this is the final response
                break response.content;
            }

            tracing::info!(
                "received {} tool call(s) in iteration {}",
                response.tool_calls.len(),
                iterations
            );

            // Add assistant message with tool calls to chat_messages
            chat_messages.push(Message {
                role: MessageRole::Assistant,
                content: response.content.clone(),
            });

            // Store assistant message with tool calls to session
            {
                let mut sessions = self.sessions.lock().unwrap();
                let session = sessions.get_or_create(&session_key);
                let session_tool_calls: Vec<crate::session::ToolCall> = response
                    .tool_calls
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .collect();
                session.add_message_full(
                    "assistant",
                    &response.content,
                    Some(session_tool_calls),
                    None,
                    None,
                );
            }

            // Execute each tool call and collect results
            for tool_call in &response.tool_calls {
                let tool_name = &tool_call.name;

                // Find which plugin provides this tool
                let plugin_name = match self.tool_plugin_map.lock().unwrap().get(tool_name) {
                    Some(name) => name.clone(),
                    None => {
                        tracing::warn!("no plugin found for tool '{}'", tool_name);
                        // Add error result message
                        chat_messages.push(Message {
                            role: MessageRole::User,  // Tool results use "user" role in OpenAI format
                            content: format!(
                                r#"{{"tool_call_id": "{}", "name": "{}", "content": "Error: tool '{}' not found"}}"#,
                                tool_call.id, tool_name, tool_name
                            ),
                        });
                        continue;
                    }
                };

                // Execute the tool via lambda_call_typed
                tracing::debug!(
                    "executing tool '{}' via plugin '{}'",
                    tool_name,
                    plugin_name
                );

                let tool_input = ExecuteToolInput {
                    name: tool_name.clone(),
                    arguments: tool_call.arguments.clone(),
                };

                let tool_response: ToolExecutionResponse = lambda_call_typed(
                    &self.plugin_host,
                    Arc::clone(&self.http_executor),
                    &plugin_name,
                    Action::ExecuteTool,
                    &tool_input,
                    &[],
                )
                .await
                .map_err(|e| Error::Plugin(format!("tool call failed: {}", e)))?;

                // Format tool result as a message
                let tool_result_content = if let Some(error) = tool_response.error {
                    format!(
                        r#"{{"tool_call_id": "{}", "name": "{}", "content": "Error: {}"}}"#,
                        tool_call.id, tool_name, error
                    )
                } else {
                    format!(
                        r#"{{"tool_call_id": "{}", "name": "{}", "content": {}}}"#,
                        tool_call.id, tool_name, tool_response.result
                    )
                };

                chat_messages.push(Message {
                    role: MessageRole::User, // Tool results use "user" role in OpenAI format
                    content: tool_result_content.clone(),
                });

                // Store tool result to session with tool_call_id and name
                {
                    let mut sessions = self.sessions.lock().unwrap();
                    let session = sessions.get_or_create(&session_key);
                    session.add_message_full(
                        "tool",
                        &tool_result_content,
                        None,
                        Some(tool_call.id.clone()),
                        Some(tool_name.clone()),
                    );
                }

                tracing::debug!("tool '{}' executed successfully", tool_name);
            }
        };

        // Add assistant response to session
        {
            let mut sessions = self.sessions.lock().unwrap();
            let session = sessions.get_or_create(&session_key);
            session.add_message("assistant", &final_response);
            if let Err(e) = sessions.save(&session_key) {
                tracing::warn!("failed to save session: {}", e);
            }
        }

        tracing::info!(
            "got final response: {}...",
            final_response.chars().take(100).collect::<String>()
        );

        self.send_to_channel(&msg.channel, &msg.chat_id, &final_response)
            .await?;

        Ok(())
    }

    /// Send a text message to a channel using lambda_function
    async fn send_to_channel(
        &self,
        channel_name: &str,
        chat_id: &str,
        content: &str,
    ) -> Result<(), Error> {
        // Get token from channel config
        let token = match self.channel_configs.get(channel_name) {
            Some(cfg) => cfg.token.as_deref(),
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

        let input = SendInput {
            token: token.to_string(),
            to_user_id: chat_id.to_string(),
            content: content.to_string(),
        };

        // Use unified lambda_loop to handle all effects
        let _: SendOutput = lambda_call_typed(
            &self.plugin_host,
            Arc::clone(&self.http_executor),
            channel_name,
            Action::FormatSend,
            &input,
            &[], // empty initial state for send
        )
        .await
        .map_err(|e| Error::Plugin(format!("format_send failed: {}", e)))?;

        tracing::info!("message sent successfully to {}", chat_id);
        Ok(())
    }
}
