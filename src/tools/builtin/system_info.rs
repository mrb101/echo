use async_trait::async_trait;

use crate::providers::types::{ToolCall, ToolDefinition, ToolResult};
use crate::tools::types::Tool;

pub struct SystemInfoTool;

#[async_trait]
impl Tool for SystemInfoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "system_info".to_string(),
            description: "Get basic system information: OS, hostname, current working directory, and home directory.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn execute(&self, call: &ToolCall) -> ToolResult {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let home = std::env::var("HOME").unwrap_or_else(|_| "unknown".to_string());

        let info = format!(
            "OS: {}\nHostname: {}\nCurrent directory: {}\nHome directory: {}",
            std::env::consts::OS,
            hostname,
            cwd,
            home
        );

        ToolResult {
            call_id: call.id.clone(),
            content: info,
            is_error: false,
        }
    }

    fn requires_approval(&self) -> bool {
        false // Read-only system info
    }
}
