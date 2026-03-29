//! Weixin API Client
//!
//! HTTP client and API methods for WeChat iLink HTTP API.
//! Uses the lambda_function architecture with unified loop for all operations.

use base64::Engine;
use mochiclaw_sdk::lambda::{
    Action, CheckLoginInput, CheckLoginOutput, DigestOutput, Effect, HttpEffect, LambdaInput,
    LambdaOutput, LoginInput, LoginOutput, PreparePollInput, SendInput, SendOutput, SetTypingInput,
};
use mochiclaw_sdk::{FnResult, info, plugin_fn};
use std::collections::HashMap;

use crate::constants::*;
use crate::messages::{
    build_send_message, parse_get_config_response, parse_messages, parse_qr_status,
};
use crate::session::{
    cache_context_token, cache_typing_ticket, get_route_tag, get_typing_ticket, pop_context_token,
    set_route_tag,
};
use crate::types::*;

// ============================================================================
// HTTP Helpers (for use in lambda handlers)
// ============================================================================

fn make_http_effect(
    method: &str,
    url: &str,
    token: &str,
    route_tag: &str,
    body: Option<String>,
    timeout_ms: u32,
) -> HttpEffect {
    let headers = make_headers(token, route_tag);
    HttpEffect {
        method: method.to_string(),
        url: url.to_string(),
        headers,
        body,
        timeout_ms,
    }
}

pub fn make_headers(token: &str, route_tag: &str) -> HashMap<String, String> {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use mochiclaw_sdk::host::rand_u32;

    let mut headers = HashMap::new();
    let uint32 = rand_u32();
    let uin_b64 = BASE64.encode(uint32.to_string().as_bytes());
    headers.insert("X-WECHAT-UIN".to_string(), uin_b64);
    headers.insert("Content-Type".to_string(), "application/json".to_string());
    headers.insert(
        "AuthorizationType".to_string(),
        "ilink_bot_token".to_string(),
    );
    if !token.is_empty() {
        headers.insert("Authorization".to_string(), format!("Bearer {}", token));
    }
    if !route_tag.is_empty() {
        headers.insert("SKRouteTag".to_string(), route_tag.to_string());
    }
    headers
}

// ============================================================================
// Lambda Function Entry Point
// ============================================================================

/// Unified lambda entry point for all channel operations
#[plugin_fn]
pub fn lambda_function(params: LambdaInput) -> FnResult<LambdaOutput> {
    match params.action {
        Action::PreparePoll => handle_prepare_poll(params),
        Action::FormatSend => handle_format_send(params),
        Action::SetTyping => handle_set_typing(params),
        Action::Login => handle_login(params),
        Action::CheckLogin => handle_check_login(params),
        _ => Ok(LambdaOutput {
            effects: vec![],
            result: Vec::new(),
            new_state: Vec::new(),
        }),
    }
}

// ============================================================================
// Channel Handlers
// ============================================================================

fn handle_prepare_poll(params: LambdaInput) -> FnResult<LambdaOutput> {
    let route_tag = get_route_tag();

    // If we have effect_results, means we just got the getupdates response (second iteration)
    if !params.effect_results.is_empty() {
        let raw_response = match params.effect_results.first() {
            Some(r) if r.success => r.response.as_deref().unwrap_or("{}"),
            _ => "{}",
        };

        let parsed = parse_messages(raw_response);

        // Cache context_tokens from messages
        for msg in &parsed.messages {
            if let Some(token) = msg.metadata.get("context_token")
                && !token.is_empty()
            {
                cache_context_token(&msg.sender_id, token);
            }
        }

        return Ok(LambdaOutput {
            effects: vec![],
            result: rmp_serde::to_vec(&DigestOutput {
                messages: parsed.messages,
            })?,
            new_state: parsed.get_updates_buf.into_bytes(),
        });
    }

    // First call: get input and state, return HTTP effect
    let input: PreparePollInput = rmp_serde::from_slice(&params.payload)?;
    let state: String = rmp_serde::from_slice(&params.state).unwrap_or_default();

    let body = serde_json::json!({
        "get_updates_buf": state,
        "base_info": { "channel_version": CHANNEL_VERSION },
        "timeout_ms": 35000
    });

    let effect = make_http_effect(
        "POST",
        "https://ilinkai.weixin.qq.com/ilink/bot/getupdates",
        &input.token,
        &route_tag,
        Some(body.to_string()),
        35000,
    );

    Ok(LambdaOutput {
        effects: vec![Effect::HttpRequest(effect)],
        result: Vec::new(),
        new_state: state.into_bytes(),
    })
}

/// Unified set_typing handler:
/// - First call: check if ticket cached, if yes send sendtyping directly
/// - If no cached ticket: call getconfig to fetch ticket, then send sendtyping
fn handle_set_typing(params: LambdaInput) -> FnResult<LambdaOutput> {
    let route_tag = get_route_tag();

    // Second call: we have getconfig response, parse and cache ticket
    if params.effect_results.len() == 1 {
        let config_response = &params.effect_results[0];
        if !config_response.success {
            return Ok(LambdaOutput {
                effects: vec![],
                result: Vec::new(),
                new_state: Vec::new(),
            });
        }

        let raw_response = config_response.response.as_deref().unwrap_or("{}");
        let typing_ticket = match parse_get_config_response(raw_response) {
            Ok(ticket) => {
                let input: SetTypingInput = rmp_serde::from_slice(&params.payload)?;
                cache_typing_ticket(&input.chat_id, &ticket);
                ticket
            }
            Err(_) => {
                return Ok(LambdaOutput {
                    effects: vec![],
                    result: Vec::new(),
                    new_state: Vec::new(),
                });
            }
        };

        let input: SetTypingInput = rmp_serde::from_slice(&params.payload)?;
        let status = if input.typing { 1 } else { 2 };
        let body = serde_json::json!({
            "ilink_user_id": input.chat_id,
            "typing_ticket": typing_ticket,
            "status": status,
            "base_info": { "channel_version": CHANNEL_VERSION }
        });

        info!(
            "set_typing: sending {} for {} (fetched ticket)",
            if input.typing { "start" } else { "stop" },
            input.chat_id
        );

        return Ok(LambdaOutput {
            effects: vec![Effect::HttpRequest(make_http_effect(
                "POST",
                "https://ilinkai.weixin.qq.com/ilink/bot/sendtyping",
                &input.token,
                &route_tag,
                Some(body.to_string()),
                5000,
            ))],
            result: Vec::new(),
            new_state: Vec::new(),
        });
    }

    // First call: check if we have cached ticket
    let input: SetTypingInput = rmp_serde::from_slice(&params.payload)?;
    let status = if input.typing { 1 } else { 2 };

    info!(
        "set_typing: {} for {}",
        if input.typing { "start" } else { "stop" },
        input.chat_id
    );

    // Try to get cached typing ticket
    if let Some(ticket) = get_typing_ticket(&input.chat_id) {
        let body = serde_json::json!({
            "ilink_user_id": input.chat_id,
            "typing_ticket": ticket,
            "status": status,
            "base_info": { "channel_version": CHANNEL_VERSION }
        });

        return Ok(LambdaOutput {
            effects: vec![Effect::HttpRequest(make_http_effect(
                "POST",
                "https://ilinkai.weixin.qq.com/ilink/bot/sendtyping",
                &input.token,
                &route_tag,
                Some(body.to_string()),
                5000,
            ))],
            result: Vec::new(),
            new_state: Vec::new(),
        });
    }

    // No cached ticket, need to fetch via getconfig first
    let body = serde_json::json!({
        "ilink_user_id": input.chat_id,
        "context_token": "",
        "base_info": { "channel_version": CHANNEL_VERSION }
    });

    Ok(LambdaOutput {
        effects: vec![Effect::HttpRequest(make_http_effect(
            "POST",
            "https://ilinkai.weixin.qq.com/ilink/bot/getconfig",
            &input.token,
            &route_tag,
            Some(body.to_string()),
            5000,
        ))],
        result: Vec::new(),
        new_state: Vec::new(),
    })
}

fn handle_format_send(params: LambdaInput) -> FnResult<LambdaOutput> {
    // If we have effect_results, means we just executed effects (second iteration)
    if !params.effect_results.is_empty() {
        // Effects were executed, return success
        return Ok(LambdaOutput {
            effects: vec![],
            result: rmp_serde::to_vec(&SendOutput { success: true })?,
            new_state: Vec::new(),
        });
    }

    // First call: parse input and return effects
    let input: SendInput = rmp_serde::from_slice(&params.payload)?;
    let route_tag = get_route_tag();

    info!(
        "format_send: to_user_id={}, content_len={}",
        input.to_user_id,
        input.content.len()
    );

    let context_token = match pop_context_token(&input.to_user_id) {
        Some(ct) => ct,
        None => {
            return Ok(LambdaOutput {
                effects: vec![Effect::HttpRequest(make_http_effect(
                    "POST",
                    "https://ilinkai.weixin.qq.com/ilink/bot/sendmessage",
                    &input.token,
                    &route_tag,
                    Some(
                        serde_json::json!({
                            "error": "no pending context_token for user"
                        })
                        .to_string(),
                    ),
                    5000,
                ))],
                result: Vec::new(), // Need to loop
                new_state: Vec::new(),
            });
        }
    };

    let body = build_send_message(&input.to_user_id, &input.content, &context_token);

    // Build effects: sendmessage + sendtyping_stop
    let mut effects: Vec<Effect> = Vec::new();

    // Send the message
    effects.push(Effect::HttpRequest(make_http_effect(
        "POST",
        "https://ilinkai.weixin.qq.com/ilink/bot/sendmessage",
        &input.token,
        &route_tag,
        Some(body.to_string()),
        5000,
    )));

    // Stop typing indicator
    if let Some(ticket) = get_typing_ticket(&input.to_user_id) {
        let typing_body = serde_json::json!({
            "ilink_user_id": input.to_user_id,
            "typing_ticket": ticket,
            "status": 2, // stop typing
            "base_info": { "channel_version": CHANNEL_VERSION }
        });
        effects.push(Effect::HttpRequest(make_http_effect(
            "POST",
            "https://ilinkai.weixin.qq.com/ilink/bot/sendtyping",
            &input.token,
            &route_tag,
            Some(typing_body.to_string()),
            5000,
        )));
    }

    Ok(LambdaOutput {
        effects,
        result: Vec::new(), // No result yet, need to loop
        new_state: Vec::new(),
    })
}

// ============================================================================
// Login Handlers (unified loop)
// ============================================================================

/// Unified login handler with loop:
/// - First call: payload=LoginInput, return effect=[fetch_qr]
/// - Second call: effect_results=[qr_response], return result={qr_url, temp_token}
fn handle_login(params: LambdaInput) -> FnResult<LambdaOutput> {
    // Check if we have effect_results (second call with QR response)
    if !params.effect_results.is_empty() {
        let qr_response = &params.effect_results[0];
        let route_tag = get_route_tag();

        if !qr_response.success {
            return Ok(LambdaOutput {
                effects: vec![],
                result: rmp_serde::to_vec(&LoginOutput {
                    status: "error".to_string(),
                    qr_url: None,
                    temp_token: None,
                    token: None,
                    base_url: None,
                    error: qr_response.error.clone(),
                })?,
                new_state: Vec::new(),
            });
        }

        let raw_response = qr_response.response.as_deref().unwrap_or("{}");
        info!("handle_login raw QR response: {}", raw_response);
        let qr_resp: serde_json::Value = match serde_json::from_str(raw_response) {
            Ok(v) => v,
            Err(e) => {
                return Ok(LambdaOutput {
                    effects: vec![],
                    result: rmp_serde::to_vec(&LoginOutput {
                        status: "error".to_string(),
                        qr_url: None,
                        temp_token: None,
                        token: None,
                        base_url: None,
                        error: Some(format!("failed to parse QR response: {}", e)),
                    })?,
                    new_state: Vec::new(),
                });
            }
        };

        let qrcode_id = qr_resp.get("qrcode").and_then(|v| v.as_str()).unwrap_or("");
        let qrcode_content = qr_resp
            .get("qrcode_img_content")
            .and_then(|v| v.as_str())
            .unwrap_or(qrcode_id);
        info!(
            "handle_login qrcode_id len={}, qrcode_content len={}",
            qrcode_id.len(),
            qrcode_content.len()
        );

        return Ok(LambdaOutput {
            effects: vec![],
            result: rmp_serde::to_vec(&LoginOutput {
                status: "need_qr".to_string(),
                qr_url: Some(qrcode_content.to_string()),
                temp_token: Some(qrcode_id.to_string()),
                token: None,
                base_url: None,
                error: None,
            })?,
            new_state: route_tag.into_bytes(),
        });
    }

    // First call: parse input and return HTTP effect to fetch QR code
    let input: LoginInput = rmp_serde::from_slice(&params.payload)?;
    let config: WeixinConfig = rmp_serde::from_slice(&input.config).unwrap_or_default();

    // Set route tag from config
    set_route_tag(&config.route_tag);

    // If token already exists, return logged_in status directly
    if !config.token.is_empty() {
        let output = LoginOutput {
            status: "logged_in".to_string(),
            qr_url: None,
            temp_token: None,
            token: Some(config.token),
            base_url: Some(config.base_url),
            error: None,
        };
        return Ok(LambdaOutput {
            effects: vec![],
            result: rmp_serde::to_vec(&output)?,
            new_state: Vec::new(),
        });
    }

    // First call: return HTTP effect to fetch QR code (GET with query params)
    let url = "https://ilinkai.weixin.qq.com/ilink/bot/get_bot_qrcode?bot_type=3";
    Ok(LambdaOutput {
        effects: vec![Effect::HttpRequest(make_http_effect(
            "GET",
            url,
            "",
            &config.route_tag,
            None,
            10000,
        ))],
        result: Vec::new(),
        new_state: config.route_tag.into_bytes(),
    })
}

/// Unified check_login handler with loop:
/// - First call: payload=CheckLoginInput, return effect=[check_status]
/// - Second call: effect_results=[status_response], return result={confirmed/pending}
fn handle_check_login(params: LambdaInput) -> FnResult<LambdaOutput> {
    let route_tag = get_route_tag();

    // Check if we have effect_results (second call with status response)
    if !params.effect_results.is_empty() {
        let status_response = &params.effect_results[0];
        if !status_response.success {
            return Ok(LambdaOutput {
                effects: vec![],
                result: rmp_serde::to_vec(&CheckLoginOutput {
                    status: "error".to_string(),
                    token: None,
                    base_url: None,
                    error: status_response.error.clone(),
                })?,
                new_state: Vec::new(),
            });
        }

        let raw_response = status_response.response.as_deref().unwrap_or("{}");
        info!(
            "check_login response: success={}, raw={}",
            status_response.success, raw_response
        );
        let result = parse_qr_status(raw_response);

        return Ok(LambdaOutput {
            effects: vec![],
            result: rmp_serde::to_vec(&CheckLoginOutput {
                status: result.status,
                token: result.token,
                base_url: result.base_url,
                error: result.error,
            })?,
            new_state: Vec::new(),
        });
    }

    // First call: parse input and return HTTP effect to check QR status (GET with query params)
    let input: CheckLoginInput = rmp_serde::from_slice(&params.payload)?;
    let url = format!(
        "https://ilinkai.weixin.qq.com/ilink/bot/get_qrcode_status?qrcode={}",
        input.temp_token
    );
    info!(
        "check_login URL built, temp_token len={}",
        input.temp_token.len()
    );

    Ok(LambdaOutput {
        effects: vec![Effect::HttpRequest(make_http_effect(
            "GET", &url, "", &route_tag, None, 35000,
        ))],
        result: Vec::new(),
        new_state: input.temp_token.into_bytes(),
    })
}
