use async_trait::async_trait;

use crate::providers::types::{ToolCall, ToolDefinition, ToolResult};
use crate::tools::types::Tool;

pub struct WebFetchTool;

const MAX_BODY_BYTES: usize = 100_000;

#[async_trait]
impl Tool for WebFetchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "web_fetch".to_string(),
            description: "Fetch the content of a URL via HTTP GET. Returns the response body as text, with HTML tags stripped.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to fetch"
                    }
                },
                "required": ["url"]
            }),
        }
    }

    async fn execute(&self, call: &ToolCall) -> ToolResult {
        let url = match call.arguments.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => {
                return ToolResult {
                    call_id: call.id.clone(),
                    content: "Missing required parameter: url".to_string(),
                    is_error: true,
                };
            }
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build();

        let client = match client {
            Ok(c) => c,
            Err(e) => {
                return ToolResult {
                    call_id: call.id.clone(),
                    content: format!("Failed to create HTTP client: {}", e),
                    is_error: true,
                };
            }
        };

        match client.get(url).send().await {
            Ok(response) => {
                let status = response.status();
                match response.text().await {
                    Ok(body) => {
                        let cleaned = strip_html_tags(&body);
                        let truncated = if cleaned.len() > MAX_BODY_BYTES {
                            format!(
                                "{}...\n\n[Truncated: response was {} bytes]",
                                &cleaned[..MAX_BODY_BYTES],
                                cleaned.len()
                            )
                        } else {
                            cleaned
                        };
                        ToolResult {
                            call_id: call.id.clone(),
                            content: format!("HTTP {} {}\n\n{}", status.as_u16(), status.canonical_reason().unwrap_or(""), truncated),
                            is_error: !status.is_success(),
                        }
                    }
                    Err(e) => ToolResult {
                        call_id: call.id.clone(),
                        content: format!("Failed to read response body: {}", e),
                        is_error: true,
                    },
                }
            }
            Err(e) => ToolResult {
                call_id: call.id.clone(),
                content: format!("HTTP request failed: {}", e),
                is_error: true,
            },
        }
    }

    fn requires_approval(&self) -> bool {
        false // Read-only HTTP GET
    }
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;

    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        if !in_tag && chars[i] == '<' {
            // Check for script/style tags
            let remaining: String = lower_chars[i..].iter().collect();
            if remaining.starts_with("<script") {
                in_script = true;
            } else if remaining.starts_with("</script") {
                in_script = false;
            } else if remaining.starts_with("<style") {
                in_style = true;
            } else if remaining.starts_with("</style") {
                in_style = false;
            }
            in_tag = true;
        } else if in_tag && chars[i] == '>' {
            in_tag = false;
        } else if !in_tag && !in_script && !in_style {
            result.push(chars[i]);
        }
        i += 1;
    }

    // Collapse multiple whitespace
    let mut collapsed = String::with_capacity(result.len());
    let mut last_was_whitespace = false;
    for ch in result.chars() {
        if ch.is_whitespace() {
            if !last_was_whitespace {
                collapsed.push(if ch == '\n' { '\n' } else { ' ' });
            }
            last_was_whitespace = true;
        } else {
            collapsed.push(ch);
            last_was_whitespace = false;
        }
    }

    collapsed.trim().to_string()
}
