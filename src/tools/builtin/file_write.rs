use async_trait::async_trait;

use crate::providers::types::{ToolCall, ToolDefinition, ToolResult};
use crate::tools::types::Tool;

pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_write".to_string(),
            description: "Write content to a file at the given path. Creates the file if it doesn't exist, or overwrites it if it does.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The file path to write to"
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write to the file"
                    }
                },
                "required": ["path", "content"]
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

        let content = match call.arguments.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => {
                return ToolResult {
                    call_id: call.id.clone(),
                    content: "Missing required parameter: content".to_string(),
                    is_error: true,
                };
            }
        };

        let path = std::path::Path::new(path);

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    return ToolResult {
                        call_id: call.id.clone(),
                        content: format!("Failed to create directories: {}", e),
                        is_error: true,
                    };
                }
            }
        }

        match tokio::fs::write(path, content).await {
            Ok(()) => ToolResult {
                call_id: call.id.clone(),
                content: format!("Successfully wrote {} bytes to {}", content.len(), path.display()),
                is_error: false,
            },
            Err(e) => ToolResult {
                call_id: call.id.clone(),
                content: format!("Failed to write to '{}': {}", path.display(), e),
                is_error: true,
            },
        }
    }

    fn requires_approval(&self) -> bool {
        true // Writing files requires user approval
    }
}
