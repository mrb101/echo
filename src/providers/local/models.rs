use serde::{Deserialize, Serialize};

// --- Request types ---

#[derive(Debug, Serialize)]
pub struct OpenAiRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    pub content: String,
}

// --- Response types (non-streaming) ---

#[derive(Debug, Deserialize)]
pub struct OpenAiResponse {
    pub choices: Vec<OpenAiChoice>,
    #[allow(dead_code)]
    pub model: String,
    pub usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChoice {
    pub message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiUsage {
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
}

// --- Model list ---

#[derive(Debug, Deserialize)]
pub struct OpenAiModelList {
    pub data: Vec<OpenAiModel>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiModel {
    pub id: String,
}

// --- Streaming types ---

#[derive(Debug, Deserialize)]
pub struct OpenAiStreamChunk {
    pub choices: Vec<OpenAiStreamChoice>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiStreamChoice {
    pub delta: OpenAiDelta,
    #[allow(dead_code)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiDelta {
    pub content: Option<String>,
}

// --- Error types ---

#[derive(Debug, Deserialize)]
pub struct OpenAiErrorResponse {
    pub error: OpenAiErrorDetail,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiErrorDetail {
    pub message: String,
}
