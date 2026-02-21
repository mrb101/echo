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

        // Validate path - prevent writes to sensitive system directories
        let resolved = if path.exists() {
            match path.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    return ToolResult {
                        call_id: call.id.clone(),
                        content: format!("Cannot resolve path '{}': {}", path.display(), e),
                        is_error: true,
                    };
                }
            }
        } else {
            // For new files, canonicalize the parent and join the filename
            match path.parent().and_then(|p| {
                let parent = if p.as_os_str().is_empty() {
                    std::path::Path::new(".")
                } else {
                    p
                };
                parent.canonicalize().ok()
            }) {
                Some(parent) => {
                    if let Some(filename) = path.file_name() {
                        parent.join(filename)
                    } else {
                        return ToolResult {
                            call_id: call.id.clone(),
                            content: format!("Invalid path: no filename in '{}'", path.display()),
                            is_error: true,
                        };
                    }
                }
                None => {
                    return ToolResult {
                        call_id: call.id.clone(),
                        content: format!("Cannot resolve parent directory of '{}'", path.display()),
                        is_error: true,
                    };
                }
            }
        };

        const BLOCKED_PREFIXES: &[&str] = &[
            "/etc", "/usr", "/bin", "/sbin", "/boot", "/proc", "/sys", "/dev",
        ];
        let resolved_str = resolved.to_string_lossy();
        for prefix in BLOCKED_PREFIXES {
            if resolved_str == *prefix || resolved_str.starts_with(&format!("{}/", prefix)) {
                return ToolResult {
                    call_id: call.id.clone(),
                    content: format!(
                        "Blocked: writing to '{}' is not allowed",
                        resolved.display()
                    ),
                    is_error: true,
                };
            }
        }

        match tokio::fs::write(&resolved, content).await {
            Ok(()) => ToolResult {
                call_id: call.id.clone(),
                content: format!(
                    "Successfully wrote {} bytes to {}",
                    content.len(),
                    resolved.display()
                ),
                is_error: false,
            },
            Err(e) => ToolResult {
                call_id: call.id.clone(),
                content: format!("Failed to write to '{}': {}", resolved.display(), e),
                is_error: true,
            },
        }
    }

    fn requires_approval(&self) -> bool {
        true // Writing files requires user approval
    }
}
