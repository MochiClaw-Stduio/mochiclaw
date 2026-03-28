//! Message Parsing and Building
//!
//! Functions for parsing WeChat API responses and building request payloads.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use mochiclaw_sdk::{host::{rand_bytes, rand_u32}, message::InboundMessage};
use serde::Deserialize;
use std::collections::HashMap;

use crate::constants::*;
use crate::crypto::{encrypt_aes_ecb, md5_hex};
use crate::types::*;

// ============================================================================
// JSON Helpers
// ============================================================================

fn json_get_str(obj: &serde_json::Value, key: &str) -> String {
    obj.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn json_get_i64_as_str(obj: &serde_json::Value, key: &str) -> String {
    obj.get(key)
        .and_then(|v| v.as_i64())
        .map(|n| n.to_string())
        .unwrap_or_default()
}

fn json_get_i32(obj: &serde_json::Value, key: &str) -> i32 {
    obj.get(key).and_then(|v| v.as_i64()).unwrap_or(0) as i32
}

// ============================================================================
// Message Parsing
// ============================================================================

/// Parse poll response JSON into PollResult
pub fn parse_messages(raw_json: &str) -> PollResult {
    let api_resp: WeixinApiResponse = match serde_json::from_str(raw_json) {
        Ok(r) => r,
        Err(e) => {
            return PollResult {
                messages: vec![],
                get_updates_buf: String::new(),
                error: Some(format!("failed to parse response: {}", e)),
            };
        }
    };

    // Check for errors
    if let Some(ret) = api_resp.ret {
        if ret != 0 {
            return PollResult {
                messages: vec![],
                get_updates_buf: api_resp.get_updates_buf,
                error: Some(format!("API error: ret={}", ret)),
            };
        }
    }
    if let Some(errcode) = api_resp.errcode {
        if errcode != 0 {
            return PollResult {
                messages: vec![],
                get_updates_buf: api_resp.get_updates_buf,
                error: Some(format!("API error: errcode={}", errcode)),
            };
        }
    }

    let mut messages = Vec::new();
    let mut skipped_empty_content = 0;
    let mut skipped_bot_msg = 0;
    let mut skipped_no_from = 0;
    let mut skipped_no_msg_id = 0;

    for msg in &api_resp.msgs {
        // Skip bot's own messages
        if json_get_i32(msg, "message_type") == MESSAGE_TYPE_BOT {
            skipped_bot_msg += 1;
            continue;
        }

        let from_user_id = json_get_str(msg, "from_user_id");
        if from_user_id.is_empty() {
            skipped_no_from += 1;
            continue;
        }

        // Try message_id first (it's a number), then seq as fallback (like Python does)
        let mut message_id = json_get_i64_as_str(msg, "message_id");
        if message_id.is_empty() {
            message_id = json_get_i64_as_str(msg, "seq");
        }
        if message_id.is_empty() {
            skipped_no_msg_id += 1;
            continue;
        }

        let content = extract_content(msg);
        if content.is_empty() {
            skipped_empty_content += 1;
            continue;
        }

        // context_token is required for sending replies - store in metadata
        let context_token = json_get_str(msg, "context_token");
        let mut metadata = HashMap::new();
        if !context_token.is_empty() {
            metadata.insert("context_token".to_string(), context_token);
        }

        messages.push(InboundMessage {
            channel: String::new(),
            sender_id: from_user_id.clone(),
            chat_id: from_user_id.clone(),
            content,
            media: vec![],
            metadata,
            session_key_override: None,
        });
    }

    // Include debug info in error if there were skipped messages
    let debug_info = if skipped_bot_msg > 0
        || skipped_empty_content > 0
        || skipped_no_from > 0
        || skipped_no_msg_id > 0
    {
        Some(format!(
            "skipped: bot={}, no_from={}, no_id={}, empty={}",
            skipped_bot_msg, skipped_no_from, skipped_no_msg_id, skipped_empty_content
        ))
    } else {
        None
    };

    PollResult {
        messages,
        get_updates_buf: api_resp.get_updates_buf,
        error: debug_info,
    }
}

/// Extract text content from a message item_list
pub fn extract_content(msg: &serde_json::Value) -> String {
    let item_list = msg.get("item_list").and_then(|v| v.as_array());
    let Some(items) = item_list else {
        return String::new();
    };

    let mut parts = Vec::new();
    for item in items {
        let item_type = json_get_i32(item, "type");

        if item_type == ITEM_TEXT {
            let text_item = item.get("text_item").and_then(|v| v.as_object());
            if let Some(text_obj) = text_item {
                let text = text_obj.get("text").and_then(|v| v.as_str()).unwrap_or("");
                parts.push(text.to_string());
            }
        }
        // For now, just extract text. Media handling can be added later.
    }

    parts.join("\n")
}

/// Parse QR status response
pub fn parse_qr_status(raw_json: &str) -> QrStatusResult {
    let resp: QrStatusResponseInternal = match serde_json::from_str(raw_json) {
        Ok(r) => r,
        Err(e) => {
            return QrStatusResult {
                status: "error".to_string(),
                token: None,
                base_url: None,
                error: Some(format!("failed to parse response: {}", e)),
            };
        }
    };

    if resp.status == "confirmed" {
        QrStatusResult {
            status: "confirmed".to_string(),
            token: Some(resp.bot_token),
            base_url: Some(resp.baseurl),
            error: None,
        }
    } else {
        QrStatusResult {
            status: resp.status,
            token: None,
            base_url: None,
            error: None,
        }
    }
}

/// Parse send message response
pub fn parse_send_response(raw_json: &str) -> Result<(), String> {
    let resp: WeixinApiResponse = serde_json::from_str(raw_json).map_err(|e| e.to_string())?;

    if let Some(errcode) = resp.errcode {
        if errcode != 0 {
            return Err(format!("send failed: errcode={}", errcode));
        }
    }
    if let Some(ret) = resp.ret {
        if ret != 0 {
            return Err(format!("send failed: ret={}", ret));
        }
    }
    Ok(())
}

/// Parse getConfig response to extract typing_ticket
pub fn parse_get_config_response(raw_json: &str) -> Result<String, String> {
    #[derive(Debug, Deserialize)]
    struct GetConfigResp {
        ret: Option<i32>,
        #[serde(default)]
        errmsg: String,
        #[serde(default)]
        typing_ticket: String,
    }

    let resp: GetConfigResp = serde_json::from_str(raw_json).map_err(|e| e.to_string())?;

    if let Some(ret) = resp.ret {
        if ret != 0 {
            return Err(format!(
                "getConfig failed: ret={}, errmsg={}",
                ret, resp.errmsg
            ));
        }
    }

    if resp.typing_ticket.is_empty() {
        return Err("no typing_ticket in response".to_string());
    }

    Ok(resp.typing_ticket)
}

// ============================================================================
// Message Building
// ============================================================================

/// Build a send message request body
pub fn build_send_message(
    to_user_id: &str,
    content: &str,
    context_token: &str,
) -> serde_json::Value {
    let client_id = format!("mochiclaw-{:x}", rand_u32());

    let mut item_list = Vec::new();
    if !content.is_empty() {
        item_list.push(serde_json::json!({
            "type": ITEM_TEXT,
            "text_item": { "text": content }
        }));
    }

    let mut msg = serde_json::json!({
        "from_user_id": "",
        "to_user_id": to_user_id,
        "client_id": client_id,
        "message_type": MESSAGE_TYPE_BOT,
        "message_state": MESSAGE_STATE_FINISH,
    });

    if !item_list.is_empty() {
        msg["item_list"] = serde_json::json!(item_list);
    }
    if !context_token.is_empty() {
        msg["context_token"] = serde_json::json!(context_token);
    }

    serde_json::json!({
        "msg": msg,
        "base_info": { "channel_version": CHANNEL_VERSION }
    })
}

/// Build media upload request
pub fn build_media_upload(
    to_user_id: &str,
    file_data: &[u8],
    _file_name: &str,
    media_type: i32,
) -> Result<(String, String, Vec<u8>), String> {
    // Generate random AES key
    let mut aes_key = [0u8; 16];
    rand_bytes(&mut aes_key);
    let aes_key_hex = hex::encode(aes_key);
    let aes_key_b64 = BASE64.encode(&aes_key);

    // Calculate sizes
    let raw_size = file_data.len();
    let padded_size = ((raw_size + 1 + 15) / 16) * 16;

    // Encrypt the file
    let encrypted = encrypt_aes_ecb(file_data, &aes_key);

    // Generate file key
    let mut file_key_bytes = [0u8; 16];
    rand_bytes(&mut file_key_bytes);
    let file_key = hex::encode(file_key_bytes);

    // Build upload request
    let upload_req = serde_json::json!({
        "filekey": file_key,
        "media_type": media_type,
        "to_user_id": to_user_id,
        "rawsize": raw_size,
        "rawfilemd5": md5_hex(file_data),
        "filesize": padded_size,
        "no_need_thumb": true,
        "aeskey": aes_key_hex,
    });

    Ok((
        serde_json::to_string(&upload_req).map_err(|e| e.to_string())?,
        aes_key_b64,
        encrypted,
    ))
}

/// Build a media message request body
pub fn build_media_message(
    to_user_id: &str,
    content: &str,
    context_token: &str,
    media_type: i32,
    download_param: &str,
    aes_key_b64: &str,
    file_name: &str,
    file_size: usize,
) -> serde_json::Value {
    let client_id = format!("mochiclaw-{:x}", rand_u32());

    let (item_type, item_key) = match media_type {
        UPLOAD_MEDIA_IMAGE => (ITEM_IMAGE, "image_item"),
        UPLOAD_MEDIA_VIDEO => (ITEM_VIDEO, "video_item"),
        _ => (ITEM_FILE, "file_item"),
    };

    let mut media_item = serde_json::json!({
        "media": {
            "encrypt_query_param": download_param,
            "aes_key": BASE64.encode(aes_key_b64.as_bytes()),
            "encrypt_type": 1,
        }
    });

    if item_type == ITEM_IMAGE {
        media_item["mid_size"] = serde_json::json!(file_size);
    } else if item_type == ITEM_VIDEO {
        media_item["video_size"] = serde_json::json!(file_size);
    } else if item_type == ITEM_FILE {
        media_item["file_name"] = serde_json::json!(file_name);
        media_item["len"] = serde_json::json!(format!("{}", file_size));
    }

    let mut msg = serde_json::json!({
        "from_user_id": "",
        "to_user_id": to_user_id,
        "client_id": client_id,
        "message_type": MESSAGE_TYPE_BOT,
        "message_state": MESSAGE_STATE_FINISH,
        "item_list": [{
            "type": item_type,
            item_key: media_item
        }],
    });

    if !context_token.is_empty() {
        msg["context_token"] = serde_json::json!(context_token);
    }
    if !content.is_empty() {
        // Add text as separate item
        if let Some(arr) = msg["item_list"].as_array_mut() {
            arr.insert(
                0,
                serde_json::json!({
                    "type": ITEM_TEXT,
                    "text_item": { "text": content }
                }),
            );
        }
    }

    serde_json::json!({
        "msg": msg,
        "base_info": { "channel_version": CHANNEL_VERSION }
    })
}
