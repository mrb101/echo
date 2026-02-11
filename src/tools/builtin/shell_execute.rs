use async_trait::async_trait;

use crate::providers::types::{ToolCall, ToolDefinition, ToolResult};
use crate::tools::types::Tool;

pub struct ShellExecuteTool;

const TIMEOUT_SECS: u64 = 30;
const MAX_OUTPUT_BYTES: usize = 50_000;

#[async_trait]
impl Tool for ShellExecuteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "shell_execute".to_string(),
            description: "Execute a shell command and return its output. Commands run with a 30-second timeout.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall) -> ToolResult {
        let command = match call.arguments.get("command").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => {
                return ToolResult {
                    call_id: call.id.clone(),
                    content: "Missing required parameter: command".to_string(),
                    is_error: true,
                };
            }
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(TIMEOUT_SECS),
            tokio::process::Command::new("sh")
                .arg("-c")
                .arg(command)
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                let mut result_text = String::new();
                if !stdout.is_empty() {
                    result_text.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !result_text.is_empty() {
                        result_text.push_str("\n--- stderr ---\n");
                    }
                    result_text.push_str(&stderr);
                }

                if result_text.is_empty() {
                    result_text = format!("Command completed with exit code {}", output.status.code().unwrap_or(-1));
                }

                // Truncate if too long
                if result_text.len() > MAX_OUTPUT_BYTES {
                    result_text.truncate(MAX_OUTPUT_BYTES);
                    result_text.push_str("\n...[output truncated]");
                }

                let exit_code = output.status.code().unwrap_or(-1);
                if !output.status.success() {
                    result_text = format!("Exit code: {}\n{}", exit_code, result_text);
                }

                ToolResult {
                    call_id: call.id.clone(),
                    content: result_text,
                    is_error: !output.status.success(),
                }
            }
            Ok(Err(e)) => ToolResult {
                call_id: call.id.clone(),
                content: format!("Failed to execute command: {}", e),
                is_error: true,
            },
            Err(_) => ToolResult {
                call_id: call.id.clone(),
                content: format!("Command timed out after {} seconds", TIMEOUT_SECS),
                is_error: true,
            },
        }
    }

    fn requires_approval(&self) -> bool {
        true // Shell commands require user approval
    }
}
