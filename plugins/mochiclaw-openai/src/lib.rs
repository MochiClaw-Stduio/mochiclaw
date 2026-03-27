//! OpenAI Compatible Provider Plugin
//!
//! This plugin implements an LLM provider using the OpenAI Chat Completions API.
//! Uses extism's built-in http::request for HTTP calls.

use extism_pdk::http::{request, HttpResponse};
use extism_pdk::*;
use extism_manifest::HttpRequest;
use mochiclaw_sdk::provider::{ChatRequest, ChatResponse, MessageRole};
use serde::Serialize;

fn role_to_string(role: &MessageRole) -> &str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
    }
}

#[derive(Debug, Serialize)]
struct ChatChunk {
    delta: String,
    done: bool,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

/// Make an HTTP request using extism's built-in http support
fn make_http_request(req: HttpRequest, body: Option<String>) -> Result<HttpResponse, Error> {
    match body {
        Some(b) => request(&req, Some(b.as_str())),
        None => request(&req, Option::<()>::None),
    }
}

/// Non-streaming chat completion
#[plugin_fn]
pub fn chat(request_json: String) -> FnResult<String> {
    let request: ChatRequest = match serde_json::from_str(&request_json) {
        Ok(r) => r,
        Err(e) => {
            let response = ErrorResponse {
                error: format!("invalid request: {}", e),
            };
            return Ok(serde_json::to_string(&response).unwrap_or_default());
        }
    };

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
    let http_req = HttpRequest::new(&url)
        .with_method("POST")
        .with_header("Content-Type", "application/json")
        .with_header("Authorization", format!("Bearer {}", api_key));

    // Make the request
    let response = match make_http_request(http_req, Some(body.to_string())) {
        Ok(resp) => resp,
        Err(e) => {
            let response = ErrorResponse {
                error: format!("HTTP request failed: {}", e),
            };
            return Ok(serde_json::to_string(&response).unwrap_or_default());
        }
    };

    let status = response.status_code();
    if status != 200 {
        let body_str = String::from_utf8_lossy(&response.body()).to_string();
        let response = ErrorResponse {
            error: format!("OpenAI API returned status {}: {}", status, body_str),
        };
        return Ok(serde_json::to_string(&response).unwrap_or_default());
    }

    // Parse the OpenAI response
    let resp_body = response.body();
    let resp_str = String::from_utf8_lossy(&resp_body).to_string();
    let openai_resp: serde_json::Value = match serde_json::from_str(&resp_str) {
        Ok(v) => v,
        Err(e) => {
            let response = ErrorResponse {
                error: format!("failed to parse OpenAI response: {}", e),
            };
            return Ok(serde_json::to_string(&response).unwrap_or_default());
        }
    };

    // Extract content from response
    let content = openai_resp["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let response = ChatResponse {
        content,
        tool_calls: Vec::new(),
        error: None,
    };
    Ok(serde_json::to_string(&response).unwrap_or_default())
}

/// Streaming chat completion
#[plugin_fn]
pub fn chat_stream(request_json: String) -> FnResult<String> {
    let request: ChatRequest = match serde_json::from_str(&request_json) {
        Ok(r) => r,
        Err(e) => {
            let response = ErrorResponse {
                error: format!("invalid request: {}", e),
            };
            return Ok(serde_json::to_string(&response).unwrap_or_default());
        }
    };

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
    let http_req = HttpRequest::new(&url)
        .with_method("POST")
        .with_header("Content-Type", "application/json")
        .with_header("Authorization", format!("Bearer {}", api_key));

    // Make the streaming request
    let response = match make_http_request(http_req, Some(body.to_string())) {
        Ok(resp) => resp,
        Err(e) => {
            let chunk = ChatChunk {
                delta: format!("HTTP request failed: {}", e),
                done: true,
            };
            return Ok(serde_json::to_string(&vec![chunk]).unwrap_or_default());
        }
    };

    // Parse SSE stream from response body
    let resp_body = response.body();
    let body_str = String::from_utf8_lossy(&resp_body).to_string();
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

    Ok(serde_json::to_string(&chunks).unwrap_or_default())
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
