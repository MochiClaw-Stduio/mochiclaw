//! Channel poller - spawns independent async tasks to poll channels for messages

use crate::bus::MessageBus;
use crate::error::Error;
use crate::history_store::HistoryStore;
use crate::http_executor::AsyncHttpExecutor;
use crate::lambda_loop::lambda_call_typed;
use mochiclaw_config::ChannelConfig;
use mochiclaw_lambda::LambdaHost;
use mochiclaw_sdk::lambda::{Action, DigestOutput, PreparePollInput};
use std::sync::Arc;
use std::time::Duration;

/// Spawn an independent async task that polls a single channel
pub fn spawn_channel_poller(
    channel_name: String,
    config: ChannelConfig,
    bus: Arc<MessageBus>,
    lambda_host: Arc<LambdaHost>,
    http_executor: Arc<AsyncHttpExecutor>,
    history_store: Arc<HistoryStore>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));

        loop {
            interval.tick().await;

            // Use unified lambda_loop to handle the entire prepare_poll flow
            // execution_id 使用 channel_name，状态由 history_store 管理
            let input = PreparePollInput {
                token: config.token.clone().unwrap_or_default(),
            };

            let result: Result<DigestOutput, Error> = lambda_call_typed(
                &lambda_host,
                Arc::clone(&http_executor),
                Arc::clone(&history_store),
                &channel_name, // execution_id = channel_name
                &channel_name, // lambda_name = channel_name
                Action::PreparePoll,
                &input,
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
