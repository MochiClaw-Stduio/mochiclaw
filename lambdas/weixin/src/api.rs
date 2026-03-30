//! Weixin API Client
//!
//! HTTP client and API methods for WeChat iLink HTTP API.
//! Uses the durable execution model with Context for suspend/resume.

use base64::Engine;
use std::collections::HashMap;

use mochiclaw_macro::mochi_main;
use mochiclaw_sdk::lambda::{
    Action, CheckLoginInput, CheckLoginOutput, DigestOutput, Effect, HttpEffect, LoginInput,
    LoginOutput, PreparePollInput, SendInput, SendOutput, SetTypingInput,
};

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
// HTTP Helpers
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

#[mochi_main]
pub fn main_handler(
    ctx: &mut mochiclaw_sdk::lambda::Context,
    action: Action,
    payload: &[u8],
) -> Result<Vec<u8>, mochiclaw_sdk::lambda::SuspendSignal> {
    match action {
        Action::PreparePoll => handle_prepare_poll(ctx, payload),
        Action::FormatSend => handle_format_send(ctx, payload),
        Action::SetTyping => handle_set_typing(ctx, payload),
        Action::Login => handle_login(ctx, payload),
        Action::CheckLogin => handle_check_login(ctx, payload),
        _ => Ok(Vec::new()),
    }
}

// ============================================================================
// Channel Handlers
// ============================================================================

fn handle_prepare_poll(
    ctx: &mut mochiclaw_sdk::lambda::Context,
    payload: &[u8],
) -> Result<Vec<u8>, mochiclaw_sdk::lambda::SuspendSignal> {
    let route_tag = get_route_tag();
    let input: PreparePollInput = rmp_serde::from_slice(payload).map_err(|_| {
        mochiclaw_sdk::lambda::SuspendSignal::new(
            "prepare_poll",
            Effect::HttpRequest(HttpEffect {
                method: String::new(),
                url: String::new(),
                headers: HashMap::new(),
                body: Some("failed to parse PreparePollInput".to_string()),
                timeout_ms: 0,
            }),
        )
    })?;

    // Get state from history if exists (for incremental polling)
    let state: String = ctx
        .step("prepare_poll_state", |_ctx| {
            Ok(String::new()) // Default empty state
        })
        .unwrap_or_default();

    let body = serde_json::json!({
        "get_updates_buf": state,
        "base_info": { "channel_version": CHANNEL_VERSION },
        "timeout_ms": 35000
    });

    let req = make_http_effect(
        "POST",
        "https://ilinkai.weixin.qq.com/ilink/bot/getupdates",
        &input.token,
        &route_tag,
        Some(body.to_string()),
        35000,
    );

    // HTTP call - may suspend on first call, returns cached on resumption
    let http_result = ctx.http("prepare_poll", req)?;

    if !http_result.success {
        return Ok(rmp_serde::to_vec(&DigestOutput {
            messages: Vec::new(),
        })
        .unwrap_or_default());
    }

    let raw_response = http_result.response.as_deref().unwrap_or("{}");
    let parsed = parse_messages(raw_response);

    // Cache context_tokens from messages
    for msg in &parsed.messages {
        if let Some(token) = msg.metadata.get("context_token")
            && !token.is_empty()
        {
            cache_context_token(&msg.sender_id, token);
        }
    }

    // IMPORTANT: Update state with new get_updates_buf for next poll
    // This step will be cached so subsequent polls use the updated state
    let _new_state: String = ctx
        .step("prepare_poll_state", |_ctx| {
            Ok(parsed.get_updates_buf.clone())
        })
        .unwrap_or_default();

    // Return result
    Ok(rmp_serde::to_vec(&DigestOutput {
        messages: parsed.messages,
    })
    .unwrap_or_default())
}

fn handle_set_typing(
    ctx: &mut mochiclaw_sdk::lambda::Context,
    payload: &[u8],
) -> Result<Vec<u8>, mochiclaw_sdk::lambda::SuspendSignal> {
    let route_tag = get_route_tag();
    let input: SetTypingInput = rmp_serde::from_slice(payload).map_err(|_| {
        mochiclaw_sdk::lambda::SuspendSignal::new(
            "set_typing",
            Effect::HttpRequest(HttpEffect {
                method: String::new(),
                url: String::new(),
                headers: HashMap::new(),
                body: Some("failed to parse SetTypingInput".to_string()),
                timeout_ms: 0,
            }),
        )
    })?;

    let status = if input.typing { 1 } else { 2 };

    // Step 1: Try to get cached typing ticket
    let typing_ticket = ctx
        .step(&format!("typing_ticket_{}", input.chat_id), |_ctx| {
            // If cached ticket exists, return it
            if let Some(ticket) = get_typing_ticket(&input.chat_id) {
                return Ok(Some(ticket));
            }
            Ok(None)
        })
        .unwrap_or(None);

    if let Some(ticket) = typing_ticket {
        // Have cached ticket, directly send typing
        let body = serde_json::json!({
            "ilink_user_id": input.chat_id,
            "typing_ticket": ticket,
            "status": status,
            "base_info": { "channel_version": CHANNEL_VERSION }
        });

        let req = make_http_effect(
            "POST",
            "https://ilinkai.weixin.qq.com/ilink/bot/sendtyping",
            &input.token,
            &route_tag,
            Some(body.to_string()),
            5000,
        );

        let _http_result = ctx.http("send_typing", req)?;
        // Result is cached, no need to parse
        return Ok(Vec::new());
    }

    // Step 1: No cached ticket, need to fetch via getconfig first
    let body = serde_json::json!({
        "ilink_user_id": input.chat_id,
        "context_token": "",
        "base_info": { "channel_version": CHANNEL_VERSION }
    });

    let req = make_http_effect(
        "POST",
        "https://ilinkai.weixin.qq.com/ilink/bot/getconfig",
        &input.token,
        &route_tag,
        Some(body.to_string()),
        5000,
    );

    let http_result = ctx.http("get_config", req)?;

    if !http_result.success {
        return Ok(Vec::new());
    }

    // Parse response and cache ticket
    let raw_response = http_result.response.as_deref().unwrap_or("{}");
    let ticket = parse_get_config_response(raw_response).unwrap_or_default();
    if !ticket.is_empty() {
        cache_typing_ticket(&input.chat_id, &ticket);
    }

    // Step 2: Now send typing with the ticket we just got
    let body = serde_json::json!({
        "ilink_user_id": input.chat_id,
        "typing_ticket": ticket,
        "status": status,
        "base_info": { "channel_version": CHANNEL_VERSION }
    });

    let req = make_http_effect(
        "POST",
        "https://ilinkai.weixin.qq.com/ilink/bot/sendtyping",
        &input.token,
        &route_tag,
        Some(body.to_string()),
        5000,
    );

    let _http_result = ctx.http("send_typing", req)?;

    Ok(Vec::new())
}

fn handle_format_send(
    ctx: &mut mochiclaw_sdk::lambda::Context,
    payload: &[u8],
) -> Result<Vec<u8>, mochiclaw_sdk::lambda::SuspendSignal> {
    let input: SendInput = rmp_serde::from_slice(payload).map_err(|_| {
        mochiclaw_sdk::lambda::SuspendSignal::new(
            "format_send",
            Effect::HttpRequest(HttpEffect {
                method: String::new(),
                url: String::new(),
                headers: HashMap::new(),
                body: Some("failed to parse SendInput".to_string()),
                timeout_ms: 0,
            }),
        )
    })?;

    let route_tag = get_route_tag();

    // Get context_token (one-time use)
    let context_token: Option<String> = ctx.step("pop_context_token", |_ctx| {
        Ok(pop_context_token(&input.to_user_id))
    })?;

    if context_token.is_none() {
        // No pending context_token - send error message
        let req = make_http_effect(
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
        );
        ctx.http("send_message", req)?;
        return Ok(rmp_serde::to_vec(&SendOutput { success: false }).unwrap_or_default());
    }

    let context_token = context_token.unwrap();

    // Step 1: Stop typing indicator if ticket exists (before sending message)
    if let Some(ticket) = get_typing_ticket(&input.to_user_id) {
        let typing_body = serde_json::json!({
            "ilink_user_id": input.to_user_id,
            "typing_ticket": ticket,
            "status": 2, // stop typing
            "base_info": { "channel_version": CHANNEL_VERSION }
        });
        let req = make_http_effect(
            "POST",
            "https://ilinkai.weixin.qq.com/ilink/bot/sendtyping",
            &input.token,
            &route_tag,
            Some(typing_body.to_string()),
            5000,
        );
        ctx.http("stop_typing", req)?;
    }

    // Step 2: Send the message
    let body = build_send_message(&input.to_user_id, &input.content, &context_token);
    let req = make_http_effect(
        "POST",
        "https://ilinkai.weixin.qq.com/ilink/bot/sendmessage",
        &input.token,
        &route_tag,
        Some(body.to_string()),
        5000,
    );

    let http_result = ctx.http("send_message", req)?;

    if !http_result.success {
        return Ok(rmp_serde::to_vec(&SendOutput { success: false }).unwrap_or_default());
    }

    Ok(rmp_serde::to_vec(&SendOutput { success: true }).unwrap_or_default())
}

// ============================================================================
// Login Handlers
// ============================================================================

fn handle_login(
    ctx: &mut mochiclaw_sdk::lambda::Context,
    payload: &[u8],
) -> Result<Vec<u8>, mochiclaw_sdk::lambda::SuspendSignal> {
    let input: LoginInput = rmp_serde::from_slice(payload).map_err(|_| {
        mochiclaw_sdk::lambda::SuspendSignal::new(
            "login",
            Effect::HttpRequest(HttpEffect {
                method: String::new(),
                url: String::new(),
                headers: HashMap::new(),
                body: Some("failed to parse LoginInput".to_string()),
                timeout_ms: 0,
            }),
        )
    })?;

    let config: WeixinConfig = rmp_serde::from_slice(&input.config).unwrap_or_default();

    // Set route tag from config
    set_route_tag(&config.route_tag);

    // If token already exists, return logged_in status directly
    if !config.token.is_empty() {
        return Ok(rmp_serde::to_vec(&LoginOutput {
            status: "logged_in".to_string(),
            qr_url: None,
            temp_token: None,
            token: Some(config.token),
            base_url: Some(config.base_url),
            error: None,
        })
        .unwrap_or_default());
    }

    // Step 1: Fetch QR code
    let url = "https://ilinkai.weixin.qq.com/ilink/bot/get_bot_qrcode?bot_type=3";
    let req = make_http_effect("GET", url, "", &config.route_tag, None, 10000);

    let http_result = ctx.http("fetch_qr", req)?;

    if !http_result.success {
        return Ok(rmp_serde::to_vec(&LoginOutput {
            status: "error".to_string(),
            qr_url: None,
            temp_token: None,
            token: None,
            base_url: None,
            error: http_result.error,
        })
        .unwrap_or_default());
    }

    let raw_response = http_result.response.as_deref().unwrap_or("{}");
    let qr_resp: serde_json::Value = match serde_json::from_str(raw_response) {
        Ok(v) => v,
        Err(e) => {
            return Ok(rmp_serde::to_vec(&LoginOutput {
                status: "error".to_string(),
                qr_url: None,
                temp_token: None,
                token: None,
                base_url: None,
                error: Some(format!("failed to parse QR response: {}", e)),
            })
            .unwrap_or_default());
        }
    };

    let qrcode_id = qr_resp.get("qrcode").and_then(|v| v.as_str()).unwrap_or("");
    let qrcode_content = qr_resp
        .get("qrcode_img_content")
        .and_then(|v| v.as_str())
        .unwrap_or(qrcode_id);

    Ok(rmp_serde::to_vec(&LoginOutput {
        status: "need_qr".to_string(),
        qr_url: Some(qrcode_content.to_string()),
        temp_token: Some(qrcode_id.to_string()),
        token: None,
        base_url: None,
        error: None,
    })
    .unwrap_or_default())
}

fn handle_check_login(
    ctx: &mut mochiclaw_sdk::lambda::Context,
    payload: &[u8],
) -> Result<Vec<u8>, mochiclaw_sdk::lambda::SuspendSignal> {
    let route_tag = get_route_tag();
    let input: CheckLoginInput = rmp_serde::from_slice(payload).map_err(|_| {
        mochiclaw_sdk::lambda::SuspendSignal::new(
            "check_login",
            Effect::HttpRequest(HttpEffect {
                method: String::new(),
                url: String::new(),
                headers: HashMap::new(),
                body: Some("failed to parse CheckLoginInput".to_string()),
                timeout_ms: 0,
            }),
        )
    })?;

    let url = format!(
        "https://ilinkai.weixin.qq.com/ilink/bot/get_qrcode_status?qrcode={}",
        input.temp_token
    );

    let req = make_http_effect("GET", &url, "", &route_tag, None, 35000);

    let http_result = ctx.http("check_login", req)?;

    if !http_result.success {
        return Ok(rmp_serde::to_vec(&CheckLoginOutput {
            status: "error".to_string(),
            token: None,
            base_url: None,
            error: http_result.error,
        })
        .unwrap_or_default());
    }

    let raw_response = http_result.response.as_deref().unwrap_or("{}");
    let result = parse_qr_status(raw_response);

    Ok(rmp_serde::to_vec(&CheckLoginOutput {
        status: result.status,
        token: result.token,
        base_url: result.base_url,
        error: result.error,
    })
    .unwrap_or_default())
}
