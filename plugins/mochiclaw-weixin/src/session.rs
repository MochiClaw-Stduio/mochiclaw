//! Session State Management
//!
//! Thread-local storage for per-user context tokens, typing tickets,
//! route tags, and poll synchronization.

use std::cell::RefCell;
use std::collections::HashMap;

// Thread-local context token per user: user_id -> latest context_token
// Used for send_text to know which token to include in the API call.
thread_local! {
    static CONTEXT_TOKENS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

// Thread-local route_tag for API requests
thread_local! {
    static ROUTE_TAG: RefCell<String> = RefCell::new(String::new());
}

// Prevent concurrent poll calls (long-poll blocks for ~35s, but agent.rs calls poll every 2s)
thread_local! {
    static POLL_IN_PROGRESS: RefCell<bool> = RefCell::new(false);
}

// Thread-local typing ticket cache: user_id -> typing_ticket
thread_local! {
    static TYPING_TICKETS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Get the current route tag
pub fn get_route_tag() -> String {
    ROUTE_TAG.with(|tag| tag.borrow().clone())
}

/// Set the route tag for API requests
pub fn set_route_tag(tag: &str) {
    ROUTE_TAG.with(|t| {
        *t.borrow_mut() = tag.to_string();
    });
}

/// Cache the latest context_token for a user.
pub fn cache_context_token(user_id: &str, token: &str) {
    if !token.is_empty() {
        CONTEXT_TOKENS.with(|cache| {
            cache
                .borrow_mut()
                .insert(user_id.to_string(), token.to_string());
        });
    }
}

/// Get the cached context_token for a user and remove it.
pub fn pop_context_token(user_id: &str) -> Option<String> {
    CONTEXT_TOKENS.with(|cache| cache.borrow().get(user_id).cloned())
}

/// Cache a typing ticket for a user.
pub fn cache_typing_ticket(user_id: &str, ticket: &str) {
    if !ticket.is_empty() {
        TYPING_TICKETS.with(|cache| {
            cache
                .borrow_mut()
                .insert(user_id.to_string(), ticket.to_string());
        });
    }
}

/// Get the cached typing ticket for a user.
pub fn get_typing_ticket(user_id: &str) -> Option<String> {
    TYPING_TICKETS.with(|cache| cache.borrow().get(user_id).cloned())
}

/// Check if a poll is already in progress
pub fn is_poll_in_progress() -> bool {
    POLL_IN_PROGRESS.with(|p| *p.borrow())
}

/// Set poll in progress flag
pub fn set_poll_in_progress(value: bool) {
    POLL_IN_PROGRESS.with(|p| *p.borrow_mut() = value);
}
