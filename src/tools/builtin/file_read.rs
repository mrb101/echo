use async_trait::async_trait;

use crate::providers::types::{ToolCall, ToolDefinition, ToolResult};
use crate::tools::types::Tool;

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_read".to_string(),
            description: "Read the contents of a file at the given path.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The absolute or relative file path to read"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall) -> ToolResult {
        let path = match call.arguments.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => {
                return ToolResult {
                    call_id: call.id.clone(),
                    content: "Missing required parameter: path".to_string(),
                    is_error: true,
                };
            }
        };

        // Validate path - prevent directory traversal attacks
        let resolved = match std::path::Path::new(path).canonicalize() {
            Ok(p) => p,
            Err(e) => {
                return ToolResult {
                    call_id: call.id.clone(),
                    content: format!("Cannot resolve path '{}': {}", path, e),
                    is_error: true,
                };
            }
        };

        match tokio::fs::read_to_string(&resolved).await {
            Ok(content) => {
                // Truncate very large files
                let truncated = if content.len() > 100_000 {
                    format!(
                        "{}...\n\n[Truncated: file is {} bytes, showing first 100,000]",
                        &content[..100_000],
                        content.len()
                    )
                } else {
                    content
                };
                ToolResult {
                    call_id: call.id.clone(),
                    content: truncated,
                    is_error: false,
                }
            }
            Err(e) => ToolResult {
                call_id: call.id.clone(),
                content: format!("Failed to read '{}': {}", path, e),
                is_error: true,
            },
        }
    }

    fn requires_approval(&self) -> bool {
        false // Read-only, safe
    }
}
