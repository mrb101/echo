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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiTool>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiTool {
    #[serde(rename = "type")]
    pub tool_type: String, // "function"
    pub function: OpenAiFunction,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String, // "function"
    pub function: OpenAiToolCallFunction,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiToolCallFunction {
    pub name: String,
    pub arguments: String, // JSON string
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
    #[allow(dead_code)]
    pub finish_reason: Option<String>,
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
    pub tool_calls: Option<Vec<OpenAiStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiStreamToolCall {
    pub index: u32,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: Option<OpenAiStreamToolCallFunction>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiStreamToolCallFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
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
