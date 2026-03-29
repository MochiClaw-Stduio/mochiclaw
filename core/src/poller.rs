//! Channel poller - spawns independent async tasks to poll channels for messages

use crate::bus::MessageBus;
use crate::error::Error;
use crate::http_executor::AsyncHttpExecutor;
use crate::lambda_loop::lambda_call_typed;
use mochiclaw_config::ChannelConfig;
use mochiclaw_lambda::PluginHost;
use mochiclaw_sdk::lambda::{Action, DigestOutput, PreparePollInput};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Spawn an independent async task that polls a single channel
pub fn spawn_channel_poller(
    channel_name: String,
    config: ChannelConfig,
    bus: Arc<MessageBus>,
    plugin_host: Arc<PluginHost>,
    http_executor: Arc<AsyncHttpExecutor>,
    poll_state: Arc<tokio::sync::Mutex<HashMap<String, Vec<u8>>>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));

        loop {
            interval.tick().await;

            // Get current state
            let state = {
                let s = poll_state.lock().await;
                s.get(&channel_name).cloned().unwrap_or_default()
            };

            // Use unified lambda_loop to handle the entire prepare_poll flow
            let input = PreparePollInput {
                token: config.token.clone().unwrap_or_default(),
            };

            let result: Result<DigestOutput, Error> = lambda_call_typed(
                &plugin_host,
                Arc::clone(&http_executor),
                &channel_name,
                Action::PreparePoll,
                &input,
                &state,
            )
            .await;

            match result {
                Ok(digest) => {
                    // Send messages to bus
                    for mut msg in digest.messages {
                        msg.channel = channel_name.clone();
                        if let Err(e) = bus.send_inbound(msg).await {
                            tracing::error!("failed to send inbound: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("prepare_poll loop failed for {}: {}", channel_name, e);
                }
            }
        }
    })
}
