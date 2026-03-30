//! OpenAI Compatible Provider Lambda
//!
//! This lambda implements an LLM provider using the OpenAI Chat Completions API.
//! Uses the durable execution model with Context for suspend/resume.

use std::collections::HashMap;

use mochiclaw_sdk::lambda::{ChatInput, Effect, HttpEffect};
use mochiclaw_sdk::provider::{ChatResponse, MessageRole, ToolCall};
use mochiclaw_sdk::tool::Tool;

use mochiclaw_macro::mochi_main;

fn role_to_string(role: &MessageRole) -> &str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
    }
}

/// Convert our Tool format to OpenAI's tools format
fn tools_to_openai(tools: &[Tool]) -> Vec<serde_json::Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect()
}

/// Parse OpenAI tool_calls into our ToolCall format
fn parse_tool_calls(openai_tool_calls: &[serde_json::Value]) -> Vec<ToolCall> {
    openai_tool_calls
        .iter()
        .filter_map(|tc| {
            let id = tc["id"].as_str().unwrap_or("").to_string();
            let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
            let arguments_str = tc["function"]["arguments"].as_str().unwrap_or("{}");

            let arguments: HashMap<String, serde_json::Value> =
                serde_json::from_str(arguments_str).unwrap_or_default();

            if name.is_empty() {
                return None;
            }

            Some(ToolCall {
                id,
                name,
                arguments,
            })
        })
        .collect()
}

// ============================================================================
// Lambda Function Entry Point
// ============================================================================

/// Handler function using durable execution model
fn handle_chat(
    ctx: &mut mochiclaw_sdk::lambda::Context,
    payload: &[u8],
) -> Result<Vec<u8>, mochiclaw_sdk::lambda::SuspendSignal> {
    let input: ChatInput = rmp_serde::from_slice(payload)
        .map_err(|_| {
            mochiclaw_sdk::lambda::SuspendSignal::new("chat", Effect::HttpRequest(HttpEffect {
                method: String::new(),
                url: String::new(),
                headers: HashMap::new(),
                body: Some("failed to parse ChatInput".to_string()),
                timeout_ms: 0,
            }))
        })?;

    // Build OpenAI request
    let openai_messages: Vec<serde_json::Value> = input
        .request
        .messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": role_to_string(&m.role),
                "content": m.content
            })
        })
        .collect();

    let mut body = serde_json::json!({
        "model": input.request.model,
        "messages": openai_messages,
        "max_tokens": input.request.max_tokens,
        "temperature": input.request.temperature,
    });

    if !input.request.tools.is_empty() {
        body["tools"] = serde_json::json!(tools_to_openai(&input.request.tools));
    }

    // Build URL
    let api_key = input.request.api_key.unwrap_or_default();
    let api_base = input
        .request
        .api_base
        .unwrap_or_else(|| "https://api.openai.com".to_string());
    let base = api_base.trim_end_matches('/').trim_end_matches("/v1");
    let url = format!("{}/v1/chat/completions", base);

    // Build headers
    let mut headers = HashMap::new();
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers.insert("Authorization".to_string(), format!("Bearer {}", api_key));

    let req = HttpEffect {
        method: "POST".to_string(),
        url,
        headers,
        body: Some(body.to_string()),
        timeout_ms: 60000,
    };

    // HTTP call - may suspend on first call, returns cached EffectResult on resumption
    let http_result = ctx.http("chat", req)?;

    // If HTTP call failed, return error in result
    if !http_result.success {
        let result = rmp_serde::to_vec(&ChatResponse {
            content: String::new(),
            tool_calls: Vec::new(),
            error: http_result.error,
        });
        return Ok(result.unwrap_or_default());
    }

    // Parse successful response
    let raw_response = http_result.response.as_deref().unwrap_or("{}");
    let openai_resp: serde_json::Value = match serde_json::from_str(raw_response) {
        Ok(v) => v,
        Err(_) => {
            return Ok(rmp_serde::to_vec(&ChatResponse {
                content: String::new(),
                tool_calls: Vec::new(),
                error: Some("failed to parse OpenAI response".to_string()),
            }).unwrap_or_default());
        }
    };

    let content = openai_resp["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let tool_calls =
        if let Some(tc) = openai_resp["choices"][0]["message"]["tool_calls"].as_array() {
            parse_tool_calls(tc)
        } else {
            Vec::new()
        };

    let result = rmp_serde::to_vec(&ChatResponse {
        content,
        tool_calls,
        error: None,
    });
    Ok(result.unwrap_or_default())
}

/// Entry point using #[mochi_main] macro
#[mochi_main]
pub fn main_handler(
    ctx: &mut mochiclaw_sdk::lambda::Context,
    action: mochiclaw_sdk::lambda::Action,
    payload: &[u8],
) -> Result<Vec<u8>, mochiclaw_sdk::lambda::SuspendSignal> {
    match action {
        mochiclaw_sdk::lambda::Action::Chat => handle_chat(ctx, payload),
        _ => Ok(rmp_serde::to_vec(&ChatResponse {
            content: String::new(),
            tool_calls: Vec::new(),
            error: Some("provider lambda does not support this action".to_string()),
        }).unwrap_or_default()),
    }
}
