use futures::StreamExt;
use tokio::sync::mpsc;

use super::models::GeminiResponse;
use crate::providers::types::{StreamEvent, StopReason, ToolCall};

pub async fn parse_sse_stream(response: reqwest::Response, tx: mpsc::Sender<StreamEvent>) {
    let mut stream = response.bytes_stream();
    let mut byte_buf: Vec<u8> = Vec::new();
    let mut buffer = String::new();
    let mut last_tokens_in: Option<i64> = None;
    let mut last_tokens_out: Option<i64> = None;
    let mut has_tool_calls = false;

    while let Some(chunk_result) = stream.next().await {
        let bytes = match chunk_result {
            Ok(b) => b,
            Err(e) => {
                let _ = tx
                    .send(StreamEvent::Error(format!("Stream error: {}", e)))
                    .await;
                return;
            }
        };

        byte_buf.extend_from_slice(&bytes);

        // Decode as much valid UTF-8 as possible from the byte buffer
        let decoded = match std::str::from_utf8(&byte_buf) {
            Ok(s) => {
                let decoded = s.to_string();
                byte_buf.clear();
                decoded
            }
            Err(e) => {
                let valid_up_to = e.valid_up_to();
                if valid_up_to == 0 {
                    // No valid UTF-8 yet — wait for more data
                    continue;
                }
                // Safety: valid_up_to is guaranteed to be valid UTF-8
                let decoded = std::str::from_utf8(&byte_buf[..valid_up_to])
                    .unwrap()
                    .to_string();
                byte_buf.drain(..valid_up_to);
                decoded
            }
        };

        // Normalize CRLF to LF (Gemini API uses \r\n line endings)
        let chunk = decoded.replace("\r\n", "\n");
        buffer.push_str(&chunk);

        // Process complete SSE events from the buffer
        while let Some(event_end) = buffer.find("\n\n") {
            let event_text = buffer[..event_end].to_string();
            buffer.drain(..event_end + 2);

            // Extract data from SSE event
            let mut data = String::new();
            for line in event_text.lines() {
                if let Some(payload) = line.strip_prefix("data: ") {
                    data.push_str(payload);
                } else if let Some(payload) = line.strip_prefix("data:") {
                    data.push_str(payload);
                }
            }

            if data.is_empty() {
                continue;
            }

            // Parse the JSON payload
            match serde_json::from_str::<GeminiResponse>(&data) {
                Ok(response) => {
                    // Extract text and function calls from response
                    if let Some(candidates) = &response.candidates {
                        if let Some(candidate) = candidates.first() {
                            if let Some(content) = &candidate.content {
                                for part in &content.parts {
                                    if let Some(text) = &part.text {
                                        if tx.send(StreamEvent::Token(text.clone())).await.is_err()
                                        {
                                            return; // receiver dropped
                                        }
                                    }
                                    if let Some(fc) = &part.function_call {
                                        has_tool_calls = true;
                                        let call = ToolCall {
                                            id: uuid::Uuid::new_v4().to_string(),
                                            name: fc.name.clone(),
                                            arguments: fc.args.clone(),
                                        };
                                        if tx
                                            .send(StreamEvent::ToolCallComplete { call })
                                            .await
                                            .is_err()
                                        {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Track usage metadata (last chunk usually has the totals)
                    if let Some(usage) = &response.usage_metadata {
                        if usage.prompt_token_count.is_some() {
                            last_tokens_in = usage.prompt_token_count;
                        }
                        if usage.candidates_token_count.is_some() {
                            last_tokens_out = usage.candidates_token_count;
                        }
                    }

                    // Check for errors in response
                    if let Some(error) = &response.error {
                        let msg = error
                            .message
                            .clone()
                            .unwrap_or_else(|| "Unknown error".to_string());
                        let _ = tx.send(StreamEvent::Error(msg)).await;
                        return;
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse SSE data: {}", e);
                    // Don't abort on parse errors - partial events may occur
                }
            }
        }
    }

    // Send done event with accumulated usage
    let stop_reason = if has_tool_calls {
        Some(StopReason::ToolUse)
    } else {
        Some(StopReason::EndTurn)
    };
    let _ = tx
        .send(StreamEvent::Done {
            tokens_in: last_tokens_in,
            tokens_out: last_tokens_out,
            stop_reason,
        })
        .await;
}
