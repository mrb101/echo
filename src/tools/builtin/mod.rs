pub mod file_read;
pub mod file_write;
pub mod shell_execute;
pub mod system_info;
pub mod web_fetch;

use std::sync::Arc;

use super::registry::ToolRegistry;
use super::types::Tool;

pub fn register_all(registry: &mut ToolRegistry) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(file_read::FileReadTool),
        Arc::new(file_write::FileWriteTool),
        Arc::new(shell_execute::ShellExecuteTool),
        Arc::new(web_fetch::WebFetchTool),
        Arc::new(system_info::SystemInfoTool),
    ];
    for tool in tools {
        registry.register(tool);
    }
}
