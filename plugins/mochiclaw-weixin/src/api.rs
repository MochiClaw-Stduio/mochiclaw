//! Weixin API Client
//!
//! HTTP client and API methods for WeChat iLink HTTP API.

use base64::Engine;
use extism_manifest::HttpRequest;
use extism_pdk::http::{HttpResponse, request};
use extism_pdk::*;
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

fn make_http_request(req: HttpRequest, body: Option<&str>) -> Result<HttpResponse, String> {
    match body {
        Some(b) => request(&req, Some(b)).map_err(|e| e.to_string()),
        None => request(&req, Option::<()>::None).map_err(|e| e.to_string()),
    }
}

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
) -> Result<String, String> {
    api_get_with_route_tag(endpoint, token, params, "")
}

fn api_get_with_route_tag(
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

    let mut headers = make_headers(token, route_tag);
    headers.insert("Content-Type".to_string(), "application/json".to_string());

    let mut req = HttpRequest::new(&url).with_method("GET");
    for (k, v) in headers {
        req = req.with_header(&k, &v);
    }

    let resp = make_http_request(req, None)?;
    if resp.status_code() != 200 {
        return Err(format!(
            "HTTP {}: {}",
            resp.status_code(),
            String::from_utf8_lossy(&resp.body())
        ));
    }
    Ok(String::from_utf8_lossy(&resp.body()).to_string())
}

fn api_post(
    endpoint: &str,
    token: &str,
    body: serde_json::Value,
    route_tag: &str,
) -> Result<String, String> {
    let url = format!("https://ilinkai.weixin.qq.com/{}", endpoint);

    let headers = make_headers(token, route_tag);

    let mut req = HttpRequest::new(&url).with_method("POST");
    req = req.with_header("Content-Type", "application/json");
    for (k, v) in headers {
        req = req.with_header(&k, &v);
    }

    let body_str = body.to_string();
    debug!(
        "api_post: endpoint={}, body_len={}",
        endpoint,
        body_str.len()
    );
    let resp = make_http_request(req, Some(&body_str))?;
    if resp.status_code() != 200 {
        return Err(format!(
            "HTTP {}: {}",
            resp.status_code(),
            String::from_utf8_lossy(&resp.body())
        ));
    }
    let resp_text = String::from_utf8_lossy(&resp.body()).to_string();
    debug!(
        "api_post: endpoint={}, resp_len={}",
        endpoint,
        resp.body().len()
    );
    Ok(resp_text)
}

// ============================================================================
// Plugin Functions
// ============================================================================

/// Perform QR code login flow
/// Returns QR code URL for display, or token if already logged in
#[plugin_fn]
pub fn login(params_json: String) -> FnResult<String> {
    let params: LoginParams = match serde_json::from_str(&params_json) {
        Ok(p) => p,
        Err(e) => {
            let resp = LoginResponse {
                status: "error".to_string(),
                qr_url: None,
                temp_token: None,
                token: None,
                base_url: None,
                error: Some(format!("invalid params: {}", e)),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    let config: WeixinConfig = serde_json::from_str(&params.config_json).unwrap_or_default();

    // Store route_tag for use in API calls
    set_route_tag(&config.route_tag);

    // If we already have a token, return it
    if !config.token.is_empty() {
        let resp = LoginResponse {
            status: "logged_in".to_string(),
            qr_url: None,
            temp_token: None,
            token: Some(config.token),
            base_url: Some(config.base_url),
            error: None,
        };
        return Ok(serde_json::to_string(&resp).unwrap_or_default());
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
    ) {
        Ok(s) => s,
        Err(e) => {
            let resp = LoginResponse {
                status: "error".to_string(),
                qr_url: None,
                temp_token: None,
                token: None,
                base_url: None,
                error: Some(e),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    let qr_resp: serde_json::Value = match serde_json::from_str(&resp_text) {
        Ok(v) => v,
        Err(e) => {
            let resp = LoginResponse {
                status: "error".to_string(),
                qr_url: None,
                temp_token: None,
                token: None,
                base_url: None,
                error: Some(e.to_string()),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    let qrcode_id = qr_resp.get("qrcode").and_then(|v| v.as_str()).unwrap_or("");
    let qrcode_content = qr_resp
        .get("qrcode_img_content")
        .and_then(|v| v.as_str())
        .unwrap_or(qrcode_id);

    let resp = LoginResponse {
        status: "need_qr".to_string(),
        qr_url: Some(qrcode_content.to_string()),
        temp_token: Some(qrcode_id.to_string()),
        token: None,
        base_url: None,
        error: None,
    };
    Ok(serde_json::to_string(&resp).unwrap_or_default())
}

/// Check QR code scan status
#[plugin_fn]
pub fn check_login(params_json: String) -> FnResult<String> {
    let params: QrStatusParams = match serde_json::from_str(&params_json) {
        Ok(p) => p,
        Err(e) => {
            let resp = QrStatusResponse {
                status: "error".to_string(),
                token: None,
                base_url: None,
                error: Some(format!("invalid params: {}", e)),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    let resp_text = match api_get(
        "ilink/bot/get_qrcode_status",
        "",
        Some({
            let mut p = HashMap::new();
            p.insert("qrcode".to_string(), params.temp_token);
            p
        }),
    ) {
        Ok(s) => s,
        Err(e) => {
            let resp = QrStatusResponse {
                status: "error".to_string(),
                token: None,
                base_url: None,
                error: Some(e),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    let result = parse_qr_status(&resp_text);

    let resp = QrStatusResponse {
        status: result.status,
        token: result.token,
        base_url: result.base_url,
        error: result.error,
    };
    Ok(serde_json::to_string(&resp).unwrap_or_default())
}

/// Poll for new messages
#[plugin_fn]
pub fn poll(params_json: String) -> FnResult<String> {
    let params: PollParams = match serde_json::from_str(&params_json) {
        Ok(p) => p,
        Err(e) => {
            let resp = PollResponse {
                messages: vec![],
                get_updates_buf: String::new(),
                error: Some(format!("invalid params: {}", e)),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    let route_tag = get_route_tag();

    let body = serde_json::json!({
        "get_updates_buf": params.get_updates_buf,
        "base_info": { "channel_version": CHANNEL_VERSION },
        "timeout_ms": 35000
    });

    let resp_text = match api_post("ilink/bot/getupdates", &params.token, body, &route_tag) {
        Ok(s) => s,
        Err(e) => {
            let resp = PollResponse {
                messages: vec![],
                get_updates_buf: String::new(),
                error: Some(e),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
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

    let resp = PollResponse {
        messages: result.messages,
        get_updates_buf: result.get_updates_buf,
        error: result.error,
    };
    Ok(serde_json::to_string(&resp).unwrap_or_default())
}

/// Send a text message
#[plugin_fn]
pub fn send_text(params_json: String) -> FnResult<String> {
    let params: SendTextParams = match serde_json::from_str(&params_json) {
        Ok(p) => p,
        Err(e) => {
            let resp = SendResponse {
                success: false,
                error: Some(format!("invalid params: {}", e)),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

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
            let resp = SendResponse {
                success: false,
                error: Some("no pending context_token for user".to_string()),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
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
            let resp = SendResponse {
                success: false,
                error: Some(e),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    info!(
        "send_text: WeChat API response ({} bytes): {}",
        resp_text.len(),
        resp_text
    );
    let result = parse_send_response(&resp_text);

    let resp = SendResponse {
        success: result.is_ok(),
        error: result.err(),
    };
    Ok(serde_json::to_string(&resp).unwrap_or_default())
}

/// Get upload URL for media - returns the upload parameters
#[plugin_fn]
pub fn get_upload_url(params_json: String) -> FnResult<String> {
    let params: GetUploadUrlParams = match serde_json::from_str(&params_json) {
        Ok(p) => p,
        Err(e) => {
            let resp = UploadResponse {
                upload_param: String::new(),
                aes_key: format!("error: {}", e),
                error: None,
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    let file_data = match base64::engine::general_purpose::STANDARD.decode(&params.file_data_base64)
    {
        Ok(d) => d,
        Err(e) => {
            let resp = UploadResponse {
                upload_param: String::new(),
                aes_key: format!("error: {}", e),
                error: None,
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
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
                let resp = UploadResponse {
                    upload_param: String::new(),
                    aes_key: format!("error: {}", e),
                    error: None,
                };
                return Ok(serde_json::to_string(&resp).unwrap_or_default());
            }
        };

    let upload_body: serde_json::Value = match serde_json::from_str(&upload_req_json) {
        Ok(v) => v,
        Err(e) => {
            let resp = UploadResponse {
                upload_param: String::new(),
                aes_key: format!("error: {}", e),
                error: None,
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
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
            let resp = UploadResponse {
                upload_param: String::new(),
                aes_key: format!("error: {}", e),
                error: None,
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    let upload_resp: serde_json::Value = match serde_json::from_str(&upload_resp_text) {
        Ok(v) => v,
        Err(e) => {
            let resp = UploadResponse {
                upload_param: String::new(),
                aes_key: format!("error: {}", e),
                error: None,
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    let upload_param = upload_resp
        .get("upload_param")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    Ok(serde_json::json!({
        "upload_param": upload_param,
        "aes_key": aes_key_b64,
    })
    .to_string())
}

/// Send a media message
#[plugin_fn]
pub fn send_media(params_json: String) -> FnResult<String> {
    let params: SendMediaParams = match serde_json::from_str(&params_json) {
        Ok(p) => p,
        Err(e) => {
            let resp = SendResponse {
                success: false,
                error: Some(format!("invalid params: {}", e)),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

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
            let resp = SendResponse {
                success: false,
                error: Some(e),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    let result = parse_send_response(&resp_text);

    let resp = SendResponse {
        success: result.is_ok(),
        error: result.err(),
    };
    Ok(serde_json::to_string(&resp).unwrap_or_default())
}

/// Get config including typing_ticket for a user
#[plugin_fn]
pub fn get_config(params_json: String) -> FnResult<String> {
    let params: GetConfigParams = match serde_json::from_str(&params_json) {
        Ok(p) => p,
        Err(e) => {
            let resp = GetConfigResponse {
                success: false,
                typing_ticket: String::new(),
                error: Some(format!("invalid params: {}", e)),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    let route_tag = get_route_tag();

    let body = serde_json::json!({
        "ilink_user_id": params.ilink_user_id,
        "context_token": params.context_token,
        "base_info": { "channel_version": CHANNEL_VERSION }
    });

    let resp_text = match api_post("ilink/bot/getconfig", &params.token, body, &route_tag) {
        Ok(s) => s,
        Err(e) => {
            let resp = GetConfigResponse {
                success: false,
                typing_ticket: String::new(),
                error: Some(e),
            };
            return Ok(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    match parse_get_config_response(&resp_text) {
        Ok(typing_ticket) => {
            // Cache the typing ticket for this user
            cache_typing_ticket(&params.ilink_user_id, &typing_ticket);
            let resp = GetConfigResponse {
                success: true,
                typing_ticket,
                error: None,
            };
            Ok(serde_json::to_string(&resp).unwrap_or_default())
        }
        Err(e) => {
            let resp = GetConfigResponse {
                success: false,
                typing_ticket: String::new(),
                error: Some(e),
            };
            Ok(serde_json::to_string(&resp).unwrap_or_default())
        }
    }
}

/// Send typing indicator to a user
#[plugin_fn]
pub fn send_typing(params_json: String) -> FnResult<String> {
    let params: SendTypingParams = match serde_json::from_str(&params_json) {
        Ok(p) => p,
        Err(e) => {
            error!("send_typing: invalid params: {}", e);
            return Ok("{}".to_string());
        }
    };

    let route_tag = get_route_tag();

    let full_body = serde_json::json!({
        "ilink_user_id": params.ilink_user_id,
        "typing_ticket": params.typing_ticket,
        "status": params.status,
        "base_info": { "channel_version": CHANNEL_VERSION }
    });

    info!(
        "send_typing: ilink_user_id={}, status={}",
        params.ilink_user_id, params.status
    );

    match api_post("ilink/bot/sendtyping", &params.token, full_body, &route_tag) {
        Ok(_resp_text) => Ok("{}".to_string()),
        Err(e) => {
            error!("send_typing: HTTP error: {}", e);
            // Typing is best-effort, don't fail
            Ok("{}".to_string())
        }
    }
}

/// Parse a raw update JSON into InboundMessage JSON (legacy function)
#[plugin_fn]
pub fn parse_update(raw_json: String) -> FnResult<String> {
    let result = parse_messages(&raw_json);

    let messages: Vec<serde_json::Value> = result
        .messages
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "channel": "weixin",
                "sender_id": m.sender_id,
                "chat_id": m.chat_id,
                "content": m.content,
                "media": m.media,
            })
        })
        .collect();

    Ok(serde_json::to_string(&messages).unwrap_or_default())
}

/// Get plugin name
#[plugin_fn]
pub fn get_name() -> FnResult<String> {
    Ok("weixin".to_string())
}
