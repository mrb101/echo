use std::net::IpAddr;
use std::sync::LazyLock;

use async_trait::async_trait;
use url::Url;

use crate::providers::types::{ToolCall, ToolDefinition, ToolResult};
use crate::tools::types::Tool;

pub struct WebFetchTool;

const MAX_BODY_BYTES: usize = 100_000;

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("Failed to create HTTP client")
});

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()           // 127.0.0.0/8
                || v4.is_private()     // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || v4.is_link_local()  // 169.254.0.0/16
                || v4.is_unspecified() // 0.0.0.0
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()           // ::1
                || v6.is_unspecified() // ::
                // fd00::/8 (unique local)
                || (v6.segments()[0] & 0xff00) == 0xfd00
                // fe80::/10 (link-local)
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

async fn validate_url(raw_url: &str) -> Result<String, String> {
    let parsed = Url::parse(raw_url).map_err(|e| format!("Invalid URL: {}", e))?;

    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "Blocked scheme '{}': only http and https are allowed",
                scheme
            ))
        }
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?;

    // Resolve DNS and check all addresses
    let port = parsed.port_or_known_default().unwrap_or(80);
    let addr_str = format!("{}:{}", host, port);
    let addrs: Vec<_> = tokio::net::lookup_host(&addr_str)
        .await
        .map_err(|e| format!("DNS resolution failed for '{}': {}", host, e))?
        .collect();

    if addrs.is_empty() {
        return Err(format!(
            "DNS resolution returned no addresses for '{}'",
            host
        ));
    }

    for addr in &addrs {
        if is_private_ip(addr.ip()) {
            return Err(format!(
                "Blocked: '{}' resolves to private/loopback address {}",
                host,
                addr.ip()
            ));
        }
    }

    Ok(parsed.to_string())
}

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
        let raw_url = match call.arguments.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => {
                return ToolResult {
                    call_id: call.id.clone(),
                    content: "Missing required parameter: url".to_string(),
                    is_error: true,
                };
            }
        };

        let url = match validate_url(raw_url).await {
            Ok(u) => u,
            Err(e) => {
                return ToolResult {
                    call_id: call.id.clone(),
                    content: e,
                    is_error: true,
                };
            }
        };

        match HTTP_CLIENT.get(&url).send().await {
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
                            content: format!(
                                "HTTP {} {}\n\n{}",
                                status.as_u16(),
                                status.canonical_reason().unwrap_or(""),
                                truncated
                            ),
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
    let bytes = html.as_bytes();
    let len = bytes.len();
    let mut result = String::with_capacity(len / 2);
    let mut i = 0;
    let mut in_tag = false;
    let mut skip_content = false; // inside <script> or <style>

    while i < len {
        if !in_tag && bytes[i] == b'<' {
            // Check for script/style open and close tags
            if let Some(rest) = html.get(i..) {
                if rest.len() >= 7 && rest[..7].eq_ignore_ascii_case("<script") {
                    skip_content = true;
                } else if rest.len() >= 8 && rest[..8].eq_ignore_ascii_case("</script") {
                    skip_content = false;
                } else if rest.len() >= 6 && rest[..6].eq_ignore_ascii_case("<style") {
                    skip_content = true;
                } else if rest.len() >= 7 && rest[..7].eq_ignore_ascii_case("</style") {
                    skip_content = false;
                }
            }
            in_tag = true;
        } else if in_tag && bytes[i] == b'>' {
            in_tag = false;
        } else if !in_tag && !skip_content {
            result.push(bytes[i] as char);
        }
        i += 1;
    }

    // Collapse whitespace in a second pass (reuse the buffer)
    let mut collapsed = String::with_capacity(result.len());
    let mut last_was_whitespace = false;
    for ch in result.bytes() {
        if (ch as char).is_ascii_whitespace() {
            if !last_was_whitespace {
                collapsed.push(if ch == b'\n' { '\n' } else { ' ' });
            }
            last_was_whitespace = true;
        } else {
            collapsed.push(ch as char);
            last_was_whitespace = false;
        }
    }

    // Trim in-place: find start/end indices, then slice
    collapsed.trim().to_string()
}
