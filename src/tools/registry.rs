use std::collections::HashMap;
use std::sync::Arc;

use crate::providers::types::{ToolCall, ToolDefinition, ToolResult};

use super::types::Tool;

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let def = tool.definition();
        self.tools.insert(def.name, tool);
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    pub async fn execute(&self, call: &ToolCall) -> ToolResult {
        match self.tools.get(&call.name) {
            Some(tool) => tool.execute(call).await,
            None => ToolResult {
                call_id: call.id.clone(),
                content: format!("Unknown tool: {}", call.name),
                is_error: true,
            },
        }
    }

    pub fn requires_approval(&self, tool_name: &str) -> bool {
        self.tools
            .get(tool_name)
            .map(|t| t.requires_approval())
            .unwrap_or(true) // Unknown tools require approval
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}
