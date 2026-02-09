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
            .finish()
    }
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Token(String),
    Done {
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
    },
    Error(String),
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
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
