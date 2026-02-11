use futures::StreamExt;
use tokio::sync::mpsc;

use super::models::{ClaudeDelta, ClaudeResponseBlock, ClaudeStreamEvent};
use crate::providers::types::{StopReason, StreamEvent, ToolCall};

pub async fn parse_sse_stream(response: reqwest::Response, tx: mpsc::Sender<StreamEvent>) {
    let mut stream = response.bytes_stream();
    let mut byte_buf: Vec<u8> = Vec::new();
    let mut buffer = String::new();
    let mut tokens_in: Option<i64> = None;
    let mut tokens_out: Option<i64> = None;
    let mut stop_reason: Option<StopReason> = None;

    // Tool use tracking
    let mut current_tool_id: Option<String> = None;
    let mut current_tool_name: Option<String> = None;
    let mut current_tool_json: String = String::new();

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
                    continue;
                }
                let decoded = std::str::from_utf8(&byte_buf[..valid_up_to])
                    .unwrap()
                    .to_string();
                byte_buf.drain(..valid_up_to);
                decoded
            }
        };

        // Normalize CRLF to LF
        let chunk = decoded.replace("\r\n", "\n");
        buffer.push_str(&chunk);

        // Process complete SSE events (delimited by double newline)
        while let Some(event_end) = buffer.find("\n\n") {
            let event_text = buffer[..event_end].to_string();
            buffer.drain(..event_end + 2);

            // Claude SSE has both `event:` and `data:` lines
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

            match serde_json::from_str::<ClaudeStreamEvent>(&data) {
                Ok(event) => match event {
                    ClaudeStreamEvent::MessageStart { message } => {
                        if let Some(usage) = message.usage {
                            tokens_in = usage.input_tokens;
                        }
                    }
                    ClaudeStreamEvent::ContentBlockStart {
                        content_block,
                        ..
                    } => {
                        match content_block {
                            ClaudeResponseBlock::ToolUse { id, name, .. } => {
                                current_tool_id = Some(id.clone());
                                current_tool_name = Some(name.clone());
                                current_tool_json.clear();
                                if tx
                                    .send(StreamEvent::ToolCallStart {
                                        id,
                                        name,
                                    })
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }
                            _ => {
                                // Text block start — nothing special needed
                            }
                        }
                    }
                    ClaudeStreamEvent::ContentBlockDelta { delta, .. } => {
                        match delta {
                            ClaudeDelta::TextDelta { text } => {
                                if tx.send(StreamEvent::Token(text)).await.is_err() {
                                    return; // receiver dropped
                                }
                            }
                            ClaudeDelta::InputJsonDelta { partial_json } => {
                                current_tool_json.push_str(&partial_json);
                                if let Some(id) = &current_tool_id {
                                    if tx
                                        .send(StreamEvent::ToolCallDelta {
                                            id: id.clone(),
                                            arguments_chunk: partial_json,
                                        })
                                        .await
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                            }
                            ClaudeDelta::Other => {}
                        }
                    }
                    ClaudeStreamEvent::ContentBlockStop { .. } => {
                        // If we were accumulating a tool call, emit ToolCallComplete
                        if let (Some(id), Some(name)) =
                            (current_tool_id.take(), current_tool_name.take())
                        {
                            let arguments: serde_json::Value =
                                serde_json::from_str(&current_tool_json)
                                    .unwrap_or(serde_json::Value::Object(Default::default()));
                            current_tool_json.clear();
                            if tx
                                .send(StreamEvent::ToolCallComplete {
                                    call: ToolCall {
                                        id,
                                        name,
                                        arguments,
                                    },
                                })
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                    ClaudeStreamEvent::MessageDelta {
                        delta,
                        usage: delta_usage,
                    } => {
                        if let Some(usage) = delta_usage {
                            tokens_out = usage.output_tokens;
                        }
                        // Map stop_reason
                        if let Some(reason) = &delta.stop_reason {
                            stop_reason = match reason.as_str() {
                                "end_turn" => Some(StopReason::EndTurn),
                                "tool_use" => Some(StopReason::ToolUse),
                                "max_tokens" => Some(StopReason::MaxTokens),
                                _ => None,
                            };
                        }
                    }
                    ClaudeStreamEvent::MessageStop {} => {
                        let _ = tx
                            .send(StreamEvent::Done {
                                tokens_in,
                                tokens_out,
                                stop_reason,
                            })
                            .await;
                        return;
                    }
                    ClaudeStreamEvent::Error { error } => {
                        let _ = tx.send(StreamEvent::Error(error.message)).await;
                        return;
                    }
                    // Ignore: Ping
                    _ => {}
                },
                Err(e) => {
                    tracing::warn!("Failed to parse Claude SSE data: {}", e);
                }
            }
        }
    }

    // If the stream ended without a message_stop event, send Done anyway
    let _ = tx
        .send(StreamEvent::Done {
            tokens_in,
            tokens_out,
            stop_reason,
        })
        .await;
}
