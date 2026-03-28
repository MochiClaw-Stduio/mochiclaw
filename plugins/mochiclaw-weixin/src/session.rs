//! Session State Management - Stateless via Host KV Store
//!
//! All state is stored in the host's shared KV store instead of thread-local storage.
//! Key structure: plugin="weixin", user_id=<actual user or "_global">, key=<state key>
//!
//! This enables multiple Pool instances to share state correctly.

use mochiclaw_sdk::host::kv::{kv_get, kv_set, kv_remove};

/// Plugin name for KV store
const PLUGIN_NAME: &str = "weixin";

/// Special user ID for per-plugin-instance (non-user-specific) state
const GLOBAL_USER: &str = "_global";

/// KV key for route_tag
const KEY_ROUTE_TAG: &str = "route_tag";

/// KV key prefix for context_token
const KEY_CONTEXT_TOKEN: &str = "context_token";

/// KV key prefix for typing_ticket
const KEY_TYPING_TICKET: &str = "typing_ticket";

/// Get the current route tag (stored during login, used for all API calls)
pub fn get_route_tag() -> String {
    kv_get::<String>(PLUGIN_NAME, GLOBAL_USER, KEY_ROUTE_TAG)
        .unwrap_or_default()
}

/// Set the route tag for API requests (called during login)
pub fn set_route_tag(tag: &str) {
    if !tag.is_empty() {
        let _ = kv_set(PLUGIN_NAME, GLOBAL_USER, KEY_ROUTE_TAG, &tag);
    }
}

/// Cache the latest context_token for a user.
/// This is called from poll() when receiving messages.
pub fn cache_context_token(user_id: &str, token: &str) {
    if !token.is_empty() {
        let key = format!("{}_{}", KEY_CONTEXT_TOKEN, user_id);
        let _ = kv_set(PLUGIN_NAME, user_id, &key, &token);
    }
}

/// Get the cached context_token for a user and remove it (one-time use).
/// Called from send_text() - each context_token can only be used once.
pub fn pop_context_token(user_id: &str) -> Option<String> {
    let key = format!("{}_{}", KEY_CONTEXT_TOKEN, user_id);
    let token: Option<String> = kv_get(PLUGIN_NAME, user_id, &key);
    if token.is_some() {
        let _ = kv_remove(PLUGIN_NAME, user_id, &key);
    }
    token
}

/// Cache a typing ticket for a user.
/// Called from get_config() after receiving typing_ticket from API.
pub fn cache_typing_ticket(user_id: &str, ticket: &str) {
    if !ticket.is_empty() {
        let key = format!("{}_{}", KEY_TYPING_TICKET, user_id);
        let _ = kv_set(PLUGIN_NAME, user_id, &key, &ticket);
    }
}

/// Get the cached typing ticket for a user.
pub fn get_typing_ticket(user_id: &str) -> Option<String> {
    let key = format!("{}_{}", KEY_TYPING_TICKET, user_id);
    kv_get::<String>(PLUGIN_NAME, user_id, &key)
}
