//! OpenAI Compatible Provider Lambda
//!
//! This lambda implements an LLM provider using the OpenAI Chat Completions API.
//! Uses the lambda_function architecture: returns HTTP effects for host to execute.

use std::collections::HashMap;

use mochiclaw_sdk::lambda::{Action, ChatInput, Effect, HttpEffect, LambdaInput, LambdaOutput};
use mochiclaw_sdk::provider::{ChatResponse, MessageRole, ToolCall};
use mochiclaw_sdk::tool::Tool;
use mochiclaw_sdk::{FnResult, plugin_fn};

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

            // Parse arguments JSON string into HashMap
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

/// Unified lambda entry point for provider operations
#[plugin_fn]
pub fn lambda_function(params: LambdaInput) -> FnResult<LambdaOutput> {
    match params.action {
        Action::Chat => handle_chat(params),
        // Other actions (channel/login) not supported by provider
        _ => Ok(LambdaOutput {
            effects: vec![],
            result: rmp_serde::to_vec(&ChatResponse {
                content: String::new(),
                tool_calls: Vec::new(),
                error: Some("provider lambda does not support this action".to_string()),
            })
            .unwrap_or_default(),
            new_state: Vec::new(),
        }),
    }
}

/// Unified chat handler with loop:
/// - First call: payload=ChatInput, return effect=[http_request]
/// - Second call: effect_results=[response], return result=ChatResponse
fn handle_chat(params: LambdaInput) -> FnResult<LambdaOutput> {
    // Second call: we have HTTP response, parse it and return result
    if !params.effect_results.is_empty() {
        let response = &params.effect_results[0];
        if !response.success {
            return Ok(LambdaOutput {
                effects: vec![],
                result: rmp_serde::to_vec(&ChatResponse {
                    content: String::new(),
                    tool_calls: Vec::new(),
                    error: response.error.clone(),
                })?,
                new_state: Vec::new(),
            });
        }

        let raw_response = response.response.as_deref().unwrap_or("{}");
        let openai_resp: serde_json::Value = match serde_json::from_str(raw_response) {
            Ok(v) => v,
            Err(e) => {
                return Ok(LambdaOutput {
                    effects: vec![],
                    result: rmp_serde::to_vec(&ChatResponse {
                        content: String::new(),
                        tool_calls: Vec::new(),
                        error: Some(format!("failed to parse OpenAI response: {}", e)),
                    })?,
                    new_state: Vec::new(),
                });
            }
        };

        // Extract content
        let content = openai_resp["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // Extract tool calls
        let tool_calls =
            if let Some(tc) = openai_resp["choices"][0]["message"]["tool_calls"].as_array() {
                parse_tool_calls(tc)
            } else {
                Vec::new()
            };

        return Ok(LambdaOutput {
            effects: vec![],
            result: rmp_serde::to_vec(&ChatResponse {
                content,
                tool_calls,
                error: None,
            })?,
            new_state: Vec::new(),
        });
    }

    // First call: build HTTP effect for OpenAI API
    let input: ChatInput = rmp_serde::from_slice(&params.payload)?;

    // Build OpenAI request body
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

    let effect = HttpEffect {
        method: "POST".to_string(),
        url,
        headers,
        body: Some(body.to_string()),
        timeout_ms: 60000, // 60 second timeout for chat
    };

    Ok(LambdaOutput {
        effects: vec![Effect::HttpRequest(effect)],
        result: Vec::new(),
        new_state: Vec::new(),
    })
}
