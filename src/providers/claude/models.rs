use serde::{Deserialize, Serialize};

// --- Request types ---

#[derive(Debug, Serialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub max_tokens: u32,
    pub messages: Vec<ClaudeMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ClaudeTool>>,
}

#[derive(Debug, Serialize)]
pub struct ClaudeTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct ClaudeMessage {
    pub role: String,
    pub content: ClaudeContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ClaudeContent {
    Text(String),
    Blocks(Vec<ClaudeContentBlock>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ClaudeContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ClaudeImageSource },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Serialize)]
pub struct ClaudeImageSource {
    #[serde(rename = "type")]
    pub source_type: String, // always "base64"
    pub media_type: String,
    pub data: String, // base64-encoded
}

// --- Response types (non-streaming) ---

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ClaudeResponse {
    pub id: String,
    pub content: Vec<ClaudeResponseBlock>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub usage: Option<ClaudeUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClaudeResponseBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Deserialize)]
pub struct ClaudeUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
}

// --- Models API types ---

#[derive(Debug, Deserialize)]
pub struct ClaudeModelsResponse {
    pub data: Vec<ClaudeModelInfo>,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeModelInfo {
    pub id: String,
    #[serde(default)]
    pub display_name: String,
}

// --- Error types ---

#[derive(Debug, Deserialize)]
pub struct ClaudeErrorResponse {
    pub error: ClaudeErrorDetail,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeErrorDetail {
    pub message: String,
}

// --- Streaming event types ---

/// Represents the different SSE data payloads from Claude's streaming API.
/// Each variant corresponds to a different `event:` type in the SSE stream.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
pub enum ClaudeStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: ClaudeStreamMessage },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: u32,
        content_block: ClaudeResponseBlock,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: u32, delta: ClaudeDelta },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: u32 },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: ClaudeMessageDelta,
        usage: Option<ClaudeStreamUsage>,
    },
    #[serde(rename = "message_stop")]
    MessageStop {},
    #[serde(rename = "ping")]
    Ping {},
    #[serde(rename = "error")]
    Error { error: ClaudeErrorDetail },
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ClaudeStreamMessage {
    pub id: String,
    pub model: String,
    pub usage: Option<ClaudeUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClaudeDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ClaudeMessageDelta {
    pub stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeStreamUsage {
    pub output_tokens: Option<i64>,
}
