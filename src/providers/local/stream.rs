use std::collections::HashMap;

use futures::StreamExt;
use tokio::sync::mpsc;

use super::models::OpenAiStreamChunk;
use crate::providers::types::{StopReason, StreamEvent, ToolCall};

struct ToolCallAccumulator {
    id: String,
    name: String,
    arguments: String,
}

pub async fn parse_sse_stream(response: reqwest::Response, tx: mpsc::Sender<StreamEvent>) {
    let mut stream = response.bytes_stream();
    let mut byte_buf: Vec<u8> = Vec::new();
    let mut buffer = String::new();
    let mut tool_accumulators: HashMap<u32, ToolCallAccumulator> = HashMap::new();
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

            for line in event_text.lines() {
                let payload = if let Some(p) = line.strip_prefix("data: ") {
                    p
                } else if let Some(p) = line.strip_prefix("data:") {
                    p
                } else {
                    continue;
                };

                // OpenAI signals end of stream with [DONE]
                if payload.trim() == "[DONE]" {
                    // Emit any accumulated tool calls
                    for (_, acc) in tool_accumulators.drain() {
                        let arguments: serde_json::Value = serde_json::from_str(&acc.arguments)
                            .unwrap_or(serde_json::Value::Object(Default::default()));
                        let _ = tx
                            .send(StreamEvent::ToolCallComplete {
                                call: ToolCall {
                                    id: acc.id,
                                    name: acc.name,
                                    arguments,
                                },
                            })
                            .await;
                    }
                    let stop_reason = if has_tool_calls {
                        Some(StopReason::ToolUse)
                    } else {
                        Some(StopReason::EndTurn)
                    };
                    let _ = tx
                        .send(StreamEvent::Done {
                            tokens_in: None,
                            tokens_out: None,
                            stop_reason,
                        })
                        .await;
                    return;
                }

                match serde_json::from_str::<OpenAiStreamChunk>(payload) {
                    Ok(chunk) => {
                        if let Some(choice) = chunk.choices.first() {
                            // Handle text content
                            if let Some(content) = &choice.delta.content {
                                if !content.is_empty()
                                    && tx.send(StreamEvent::Token(content.clone())).await.is_err()
                                {
                                    return; // receiver dropped
                                }
                            }

                            // Handle tool calls
                            if let Some(tool_calls) = &choice.delta.tool_calls {
                                for tc in tool_calls {
                                    let acc =
                                        tool_accumulators.entry(tc.index).or_insert_with(|| {
                                            has_tool_calls = true;
                                            ToolCallAccumulator {
                                                id: tc.id.clone().unwrap_or_default(),
                                                name: String::new(),
                                                arguments: String::new(),
                                            }
                                        });

                                    if let Some(id) = &tc.id {
                                        if !id.is_empty() {
                                            acc.id.clone_from(id);
                                        }
                                    }
                                    if let Some(func) = &tc.function {
                                        if let Some(name) = &func.name {
                                            if !name.is_empty() {
                                                if acc.name.is_empty() {
                                                    let _ = tx
                                                        .send(StreamEvent::ToolCallStart {
                                                            id: acc.id.clone(),
                                                            name: name.clone(),
                                                        })
                                                        .await;
                                                }
                                                acc.name.clone_from(name);
                                            }
                                        }
                                        if let Some(args) = &func.arguments {
                                            acc.arguments.push_str(args);
                                            if !args.is_empty() {
                                                let _ = tx
                                                    .send(StreamEvent::ToolCallDelta {
                                                        id: acc.id.clone(),
                                                        arguments_chunk: args.clone(),
                                                    })
                                                    .await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse OpenAI SSE data: {}", e);
                    }
                }
            }
        }
    }

    // If the stream ended without a [DONE] signal, send Done anyway
    let stop_reason = if has_tool_calls {
        Some(StopReason::ToolUse)
    } else {
        Some(StopReason::EndTurn)
    };
    let _ = tx
        .send(StreamEvent::Done {
            tokens_in: None,
            tokens_out: None,
            stop_reason,
        })
        .await;
}
