use async_trait::async_trait;

use crate::providers::types::{ToolCall, ToolDefinition, ToolResult};

#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, call: &ToolCall) -> ToolResult;
    fn requires_approval(&self) -> bool;
}
