//! OpenAI Compatible Provider Plugin
//!
//! This plugin implements an LLM provider using the OpenAI Chat Completions API.
//! Uses HttpClient from mochiclaw_sdk for HTTP calls.

use extism_pdk::{FromBytes, ToBytes, Msgpack, *};
use mochiclaw_sdk::host::http::HttpClient;
use mochiclaw_sdk::provider::{ChatRequest, ChatResponse, MessageRole};
use serde::{Deserialize, Serialize};

fn role_to_string(role: &MessageRole) -> &str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
    }
}

#[derive(Debug, Serialize, Deserialize, FromBytes, ToBytes)]
#[encoding(Msgpack)]
struct ChatChunk {
    delta: String,
    done: bool,
}

/// Non-streaming chat completion
#[plugin_fn]
pub fn chat(request: ChatRequest) -> FnResult<ChatResponse> {
    // Build the OpenAI request body
    let openai_messages: Vec<serde_json::Value> = request.messages.iter().map(|m| {
        serde_json::json!({
            "role": role_to_string(&m.role),
            "content": m.content
        })
    }).collect();

    let body = serde_json::json!({
        "model": request.model,
        "messages": openai_messages,
        "max_tokens": request.max_tokens,
        "temperature": request.temperature,
    });

    // Create HTTP request
    let api_key = request.api_key.unwrap_or_default();
    let api_base = request.api_base.unwrap_or_else(|| "https://api.openai.com".to_string());
    // Remove /v1 suffix if present to avoid double path segments
    let base = api_base.trim_end_matches('/').trim_end_matches("/v1");
    let url = format!("{}/v1/chat/completions", base);

    // Make the request using HttpClient
    let response = match HttpClient::post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key).as_str())
        .json(&body)
    {
        Ok(client) => match client.send() {
            Ok(resp) => resp,
            Err(e) => {
                return Ok(ChatResponse {
                    content: String::new(),
                    tool_calls: Vec::new(),
                    error: Some(format!("HTTP request failed: {}", e)),
                });
            }
        },
        Err(e) => {
            return Ok(ChatResponse {
                content: String::new(),
                tool_calls: Vec::new(),
                error: Some(format!("Failed to build request: {}", e)),
            });
        }
    };

    let status = response.status;
    if status != 200 {
        let body_str = String::from_utf8_lossy(response.bytes()).to_string();
        return Ok(ChatResponse {
            content: String::new(),
            tool_calls: Vec::new(),
            error: Some(format!("OpenAI API returned status {}: {}", status, body_str)),
        });
    }

    // Parse the OpenAI response
    let resp_body = response.bytes();
    let resp_str = String::from_utf8_lossy(resp_body).to_string();
    let openai_resp: serde_json::Value = match serde_json::from_str(&resp_str) {
        Ok(v) => v,
        Err(e) => {
            return Ok(ChatResponse {
                content: String::new(),
                tool_calls: Vec::new(),
                error: Some(format!("failed to parse OpenAI response: {}", e)),
            });
        }
    };

    // Extract content from response
    let content = openai_resp["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(ChatResponse {
        content,
        tool_calls: Vec::new(),
        error: None,
    })
}

/// Streaming chat completion
#[plugin_fn]
pub fn chat_stream(request: ChatRequest) -> FnResult<Vec<u8>> {
    // Build the OpenAI request body with streaming
    let openai_messages: Vec<serde_json::Value> = request.messages.iter().map(|m| {
        serde_json::json!({
            "role": role_to_string(&m.role),
            "content": m.content
        })
    }).collect();

    let body = serde_json::json!({
        "model": request.model,
        "messages": openai_messages,
        "max_tokens": request.max_tokens,
        "temperature": request.temperature,
        "stream": true,
    });

    // Create HTTP request
    let api_key = request.api_key.unwrap_or_default();
    let api_base = request.api_base.unwrap_or_else(|| "https://api.openai.com".to_string());
    // Remove /v1 suffix if present to avoid double path segments
    let base = api_base.trim_end_matches('/').trim_end_matches("/v1");
    let url = format!("{}/v1/chat/completions", base);

    // Make the streaming request using HttpClient
    let response = match HttpClient::post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key).as_str())
        .json(&body)
    {
        Ok(client) => match client.send() {
            Ok(resp) => resp,
            Err(e) => {
                let chunk = ChatChunk {
                    delta: format!("HTTP request failed: {}", e),
                    done: true,
                };
                return Ok(rmp_serde::to_vec(&vec![chunk]).unwrap_or_default());
            }
        },
        Err(e) => {
            let chunk = ChatChunk {
                delta: format!("Failed to build request: {}", e),
                done: true,
            };
            return Ok(rmp_serde::to_vec(&vec![chunk]).unwrap_or_default());
        }
    };

    // Parse SSE stream from response body
    let resp_body = response.bytes();
    let body_str = String::from_utf8_lossy(resp_body).to_string();
    let lines: Vec<&str> = body_str.lines().collect();
    let mut chunks: Vec<ChatChunk> = Vec::new();

    for line in lines {
        if line.starts_with("data: ") {
            let data = &line[6..];
            if data == "[DONE]" {
                continue;
            }
            if let Ok(delta) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(content) = delta["choices"][0]["delta"]["content"].as_str() {
                    chunks.push(ChatChunk {
                        delta: content.to_string(),
                        done: false,
                    });
                }
            }
        }
    }

    if let Some(last) = chunks.last_mut() {
        last.done = true;
    }

    Ok(rmp_serde::to_vec(&chunks).unwrap_or_default())
}

/// Get plugin name
#[plugin_fn]
pub fn get_name() -> FnResult<String> {
    Ok("openai".to_string())
}

/// Get supported models - returns * to indicate all models
#[plugin_fn]
pub fn get_models() -> FnResult<String> {
    Ok("*".to_string())
}
