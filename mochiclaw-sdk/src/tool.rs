//! Tool definition for agent tool calling

use extism_convert::{FromBytes, ToBytes, Msgpack};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Request structure for executing a tool
#[derive(Debug, Clone, Serialize, Deserialize, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct ToolExecutionRequest {
    pub name: String,
    pub arguments: HashMap<String, serde_json::Value>,
}

/// Response structure for tool execution
#[derive(Debug, Clone, Serialize, Deserialize, FromBytes, ToBytes)]
#[encoding(Msgpack)]
pub struct ToolExecutionResponse {
    pub result: String,
    pub error: Option<String>,
}
