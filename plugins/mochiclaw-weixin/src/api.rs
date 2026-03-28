//! Weixin API Client
//!
//! HTTP client and API methods for WeChat iLink HTTP API.

use base64::Engine;
use mochiclaw_sdk::{plugin_fn, FnResult, debug, error, info};
use mochiclaw_sdk::channel::{LoginParams, LoginResponse, PollParams, PollResponse, QrStatusParams, QrStatusResponse, SendResponse, SendTextParams, SetTypingParams};
use mochiclaw_sdk::host::http::HttpClient;
use std::collections::HashMap;

use crate::constants::*;
use crate::messages::{
    build_media_message, build_media_upload, build_send_message, parse_get_config_response,
    parse_messages, parse_qr_status, parse_send_response,
};
use crate::session::{
    cache_context_token, cache_typing_ticket, get_route_tag, pop_context_token, set_route_tag,
};
use crate::types::*;

// ============================================================================
// HTTP Helpers
// ============================================================================

pub fn make_headers(token: &str, route_tag: &str) -> HashMap<String, String> {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use mochiclaw_sdk::host::rand_u32;

    let mut headers = HashMap::new();
    // Generate random X-WECHAT-UIN
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
    // SKRouteTag may be required by some WeChat configurations
    if !route_tag.is_empty() {
        headers.insert("SKRouteTag".to_string(), route_tag.to_string());
    }
    headers
}

fn api_get(
    endpoint: &str,
    token: &str,
    params: Option<HashMap<String, String>>,
    route_tag: &str,
) -> Result<String, String> {
    let mut url = format!("https://ilinkai.weixin.qq.com/{}", endpoint);
    if let Some(p) = params {
        let query: Vec<String> = p.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        if !query.is_empty() {
            url = format!("{}?{}", url, query.join("&"));
        }
    }

    let headers = make_headers(token, route_tag);

    let mut client = HttpClient::get(&url);
    for (k, v) in headers {
        client = client.header(&k, &v);
    }

    let resp = client.send().map_err(|e| e.to_string())?;
    if resp.status != 200 {
        return Err(format!(
            "HTTP {}: {}",
            resp.status,
            String::from_utf8_lossy(resp.bytes())
        ));
    }
    Ok(String::from_utf8_lossy(resp.bytes()).to_string())
}

fn api_post(
    endpoint: &str,
    token: &str,
    body: serde_json::Value,
    route_tag: &str,
) -> Result<String, String> {
    let url = format!("https://ilinkai.weixin.qq.com/{}", endpoint);

    let headers = make_headers(token, route_tag);

    let mut client = HttpClient::post(&url);
    for (k, v) in headers {
        client = client.header(&k, &v);
    }

    let body_str = body.to_string();
    debug!(
        "api_post: endpoint={}, body_len={}",
        endpoint,
        body_str.len()
    );

    let resp = client
        .body(body_str.into_bytes())
        .send()
        .map_err(|e| e.to_string())?;
    if resp.status != 200 {
        return Err(format!(
            "HTTP {}: {}",
            resp.status,
            String::from_utf8_lossy(resp.bytes())
        ));
    }
    let resp_text = String::from_utf8_lossy(resp.bytes()).to_string();
    debug!(
        "api_post: endpoint={}, resp_len={}",
        endpoint,
        resp.bytes().len()
    );
    Ok(resp_text)
}

// ============================================================================
// Plugin Functions
// ============================================================================

/// Perform QR code login flow
/// Returns QR code URL for display, or token if already logged in
#[plugin_fn]
pub fn login(params: LoginParams) -> FnResult<LoginResponse> {
    let config: WeixinConfig = match rmp_serde::from_slice(&params.config) {
        Ok(c) => c,
        Err(_) => WeixinConfig::default(),
    };

    // Store route_tag for use in API calls
    set_route_tag(&config.route_tag);

    // If we already have a token, return it
    if !config.token.is_empty() {
        return Ok(LoginResponse {
            status: "logged_in".to_string(),
            qr_url: None,
            temp_token: None,
            token: Some(config.token),
            base_url: Some(config.base_url),
            error: None,
        });
    }

    // Fetch QR code
    let resp_text = match api_get(
        "ilink/bot/get_bot_qrcode",
        "",
        Some({
            let mut p = HashMap::new();
            p.insert("bot_type".to_string(), "3".to_string());
            p
        }),
        &get_route_tag(),
    ) {
        Ok(s) => s,
        Err(e) => {
            return Ok(LoginResponse {
                status: "error".to_string(),
                qr_url: None,
                temp_token: None,
                token: None,
                base_url: None,
                error: Some(e),
            });
        }
    };

    let qr_resp: serde_json::Value = match serde_json::from_str(&resp_text) {
        Ok(v) => v,
        Err(e) => {
            return Ok(LoginResponse {
                status: "error".to_string(),
                qr_url: None,
                temp_token: None,
                token: None,
                base_url: None,
                error: Some(e.to_string()),
            });
        }
    };

    let qrcode_id = qr_resp.get("qrcode").and_then(|v| v.as_str()).unwrap_or("");
    let qrcode_content = qr_resp
        .get("qrcode_img_content")
        .and_then(|v| v.as_str())
        .unwrap_or(qrcode_id);

    Ok(LoginResponse {
        status: "need_qr".to_string(),
        qr_url: Some(qrcode_content.to_string()),
        temp_token: Some(qrcode_id.to_string()),
        token: None,
        base_url: None,
        error: None,
    })
}

/// Check QR code scan status
#[plugin_fn]
pub fn check_login(params: QrStatusParams) -> FnResult<QrStatusResponse> {
    let resp_text = match api_get(
        "ilink/bot/get_qrcode_status",
        "",
        Some({
            let mut p = HashMap::new();
            p.insert("qrcode".to_string(), params.temp_token);
            p
        }),
        &get_route_tag(),
    ) {
        Ok(s) => s,
        Err(e) => {
            return Ok(QrStatusResponse {
                status: "error".to_string(),
                token: None,
                base_url: None,
                error: Some(e),
            });
        }
    };

    let result = parse_qr_status(&resp_text);

    Ok(QrStatusResponse {
        status: result.status,
        token: result.token,
        base_url: result.base_url,
        error: result.error,
    })
}

/// Poll for new messages
#[plugin_fn]
pub fn poll(params: PollParams) -> FnResult<PollResponse> {
    let route_tag = get_route_tag();

    let body = serde_json::json!({
        "get_updates_buf": params.get_updates_buf,
        "base_info": { "channel_version": CHANNEL_VERSION },
        "timeout_ms": 35000
    });

    let resp_text = match api_post("ilink/bot/getupdates", &params.token, body, &route_tag) {
        Ok(s) => s,
        Err(e) => {
            return Ok(PollResponse {
                messages: vec![],
                get_updates_buf: String::new(),
                error: Some(e),
            });
        }
    };

    let result = parse_messages(&resp_text);

    // Cache context_token for each user when receiving messages (use sender_id as key)
    for msg in &result.messages {
        if let Some(token) = msg.metadata.get("context_token") {
            if !token.is_empty() {
                cache_context_token(&msg.sender_id, token);
            }
        }
    }

    Ok(PollResponse {
        messages: result.messages,
        get_updates_buf: result.get_updates_buf,
        error: result.error,
    })
}

/// Send a text message
#[plugin_fn]
pub fn send_text(params: SendTextParams) -> FnResult<SendResponse> {
    let route_tag = get_route_tag();

    info!(
        "send_text: to_user_id={}, content_len={}",
        params.to_user_id,
        params.content.len()
    );

    // Get the cached context_token for this user.
    let context_token = match pop_context_token(&params.to_user_id) {
        Some(ct) => ct,
        None => {
            error!(
                "send_text: no pending context_token for user {}, cannot send",
                params.to_user_id
            );
            return Ok(SendResponse {
                success: false,
                error: Some("no pending context_token for user".to_string()),
            });
        }
    };

    info!(
        "send_text: popped context_token for {}, token={}...",
        params.to_user_id,
        &context_token[..context_token.len().min(20)]
    );

    let body = build_send_message(&params.to_user_id, &params.content, &context_token);
    let body_str = body.to_string();
    debug!("send_text: request body: {}", body_str);

    let resp_text = match api_post("ilink/bot/sendmessage", &params.token, body, &route_tag) {
        Ok(s) => s,
        Err(e) => {
            error!("send_text: HTTP error: {}", e);
            return Ok(SendResponse {
                success: false,
                error: Some(e),
            });
        }
    };

    info!(
        "send_text: WeChat API response ({} bytes): {}",
        resp_text.len(),
        resp_text
    );
    let result = parse_send_response(&resp_text);

    Ok(SendResponse {
        success: result.is_ok(),
        error: result.err(),
    })
}

/// Get upload URL for media - returns the upload parameters
#[plugin_fn]
pub fn get_upload_url(params: GetUploadUrlParams) -> FnResult<UploadResponse> {
    let file_data = match base64::engine::general_purpose::STANDARD.decode(&params.file_data_base64)
    {
        Ok(d) => d,
        Err(e) => {
            return Ok(UploadResponse {
                upload_param: String::new(),
                aes_key: format!("error: {}", e),
                error: None,
            });
        }
    };

    let med_type = match params.media_type.as_str() {
        "image" => UPLOAD_MEDIA_IMAGE,
        "video" => UPLOAD_MEDIA_VIDEO,
        _ => UPLOAD_MEDIA_FILE,
    };

    let (upload_req_json, aes_key_b64, _encrypted) =
        match build_media_upload(&params.to_user_id, &file_data, &params.file_name, med_type) {
            Ok(r) => r,
            Err(e) => {
                return Ok(UploadResponse {
                    upload_param: String::new(),
                    aes_key: format!("error: {}", e),
                    error: None,
                });
            }
        };

    let upload_body: serde_json::Value = match serde_json::from_str(&upload_req_json) {
        Ok(v) => v,
        Err(e) => {
            return Ok(UploadResponse {
                upload_param: String::new(),
                aes_key: format!("error: {}", e),
                error: None,
            });
        }
    };

    let route_tag = get_route_tag();

    let upload_resp_text = match api_post(
        "ilink/bot/getuploadurl",
        &params.token,
        upload_body,
        &route_tag,
    ) {
        Ok(s) => s,
        Err(e) => {
            return Ok(UploadResponse {
                upload_param: String::new(),
                aes_key: format!("error: {}", e),
                error: None,
            });
        }
    };

    let upload_resp: serde_json::Value = match serde_json::from_str(&upload_resp_text) {
        Ok(v) => v,
        Err(e) => {
            return Ok(UploadResponse {
                upload_param: String::new(),
                aes_key: format!("error: {}", e),
                error: None,
            });
        }
    };

    let upload_param = upload_resp
        .get("upload_param")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    Ok(UploadResponse {
        upload_param: upload_param.to_string(),
        aes_key: aes_key_b64,
        error: None,
    })
}

/// Send a media message
#[plugin_fn]
pub fn send_media(params: SendMediaParams) -> FnResult<SendResponse> {
    let med_type = match params.media_type.as_str() {
        "image" => UPLOAD_MEDIA_IMAGE,
        "video" => UPLOAD_MEDIA_VIDEO,
        _ => UPLOAD_MEDIA_FILE,
    };

    let body = build_media_message(
        &params.to_user_id,
        &params.content,
        &params.context_token,
        med_type,
        &params.download_param,
        &params.aes_key,
        &params.file_name,
        params.file_size,
    );

    let route_tag = get_route_tag();

    let resp_text = match api_post("ilink/bot/sendmessage", &params.token, body, &route_tag) {
        Ok(s) => s,
        Err(e) => {
            return Ok(SendResponse {
                success: false,
                error: Some(e),
            });
        }
    };

    let result = parse_send_response(&resp_text);

    Ok(SendResponse {
        success: result.is_ok(),
        error: result.err(),
    })
}

/// Set typing indicator (generic interface for agent).
/// typing=true means start typing, typing=false means stop typing.
#[plugin_fn]
pub fn set_typing(params: SetTypingParams) -> FnResult<()> {
    let route_tag = get_route_tag();
    let status = if params.typing { 1 } else { 2 };

    // Get typing_ticket from cache, or fetch via get_config if not cached
    let typing_ticket = match crate::session::get_typing_ticket(&params.chat_id) {
        Some(ticket) => ticket,
        None => match fetch_typing_ticket(&params.token, &params.chat_id, &route_tag) {
            Ok(ticket) => ticket,
            Err(e) => {
                error!("set_typing: failed to get typing_ticket: {}", e);
                return Ok(());
            }
        },
    };

    let full_body = serde_json::json!({
        "ilink_user_id": params.chat_id,
        "typing_ticket": typing_ticket,
        "status": status,
        "base_info": { "channel_version": CHANNEL_VERSION }
    });

    info!(
        "set_typing: chat_id={}, typing={}",
        params.chat_id, params.typing
    );

    match api_post("ilink/bot/sendtyping", &params.token, full_body, &route_tag) {
        Ok(_resp_text) => Ok(()),
        Err(e) => {
            error!("set_typing: HTTP error: {}", e);
            // Typing is best-effort, don't fail
            Ok(())
        }
    }
}

/// Internal helper to fetch typing_ticket via get_config API
fn fetch_typing_ticket(
    token: &str,
    ilink_user_id: &str,
    route_tag: &str,
) -> Result<String, String> {
    let body = serde_json::json!({
        "ilink_user_id": ilink_user_id,
        "context_token": "",
        "base_info": { "channel_version": CHANNEL_VERSION }
    });

    let resp_text = api_post("ilink/bot/getconfig", token, body, route_tag)?;

    match parse_get_config_response(&resp_text) {
        Ok(typing_ticket) => {
            // Cache the typing ticket for future use
            cache_typing_ticket(ilink_user_id, &typing_ticket);
            Ok(typing_ticket)
        }
        Err(e) => Err(e),
    }
}

/// Get plugin name
#[plugin_fn]
pub fn get_name() -> FnResult<String> {
    Ok("weixin".to_string())
}
