use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::models::Role;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Authentication failed: {0}")]
    AuthError(String),

    #[error("Rate limited: retry after {retry_after_secs:?}s")]
    RateLimited { retry_after_secs: Option<u64> },

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

// --- Tool types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
}

// --- Chat types ---

#[derive(Debug, Clone)]
pub struct ImageAttachment {
    pub mime_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    #[serde(skip)]
    pub images: Vec<ImageAttachment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_results: Vec<ToolResult>,
}

#[derive(Clone)]
pub struct ChatRequest {
    pub api_key: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub base_url: Option<String>,
    pub temperature: Option<f32>,
    pub system_prompt: Option<String>,
    pub max_tokens: Option<u32>,
    pub tools: Vec<ToolDefinition>,
}

impl std::fmt::Debug for ChatRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatRequest")
            .field("api_key", &"***")
            .field("model", &self.model)
            .field("messages", &self.messages)
            .field("base_url", &self.base_url)
            .field("temperature", &self.temperature)
            .field("system_prompt", &self.system_prompt)
            .field("max_tokens", &self.max_tokens)
            .field("tools", &format!("[{} tools]", self.tools.len()))
            .finish()
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum StreamEvent {
    Token(String),
    ToolCallStart {
        id: String,
        name: String,
    },
    ToolCallDelta {
        id: String,
        arguments_chunk: String,
    },
    ToolCallComplete {
        call: ToolCall,
    },
    Done {
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
        stop_reason: Option<StopReason>,
    },
    Error(String),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub tool_calls: Vec<ToolCall>,
    pub stop_reason: Option<StopReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub features: Vec<Feature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Feature {
    Chat,
    Vision,
    Streaming,
    FunctionCalling,
    ExtendedThinking,
    PdfInput,
}
