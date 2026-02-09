use futures::StreamExt;
use tokio::sync::mpsc;

use super::models::{ClaudeDelta, ClaudeStreamEvent};
use crate::providers::types::StreamEvent;

pub async fn parse_sse_stream(response: reqwest::Response, tx: mpsc::Sender<StreamEvent>) {
    let mut stream = response.bytes_stream();
    let mut byte_buf: Vec<u8> = Vec::new();
    let mut buffer = String::new();
    let mut tokens_in: Option<i64> = None;
    let mut tokens_out: Option<i64> = None;

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
                    ClaudeStreamEvent::ContentBlockDelta { delta, .. } => {
                        if let ClaudeDelta::TextDelta { text } = delta {
                            if tx.send(StreamEvent::Token(text)).await.is_err() {
                                return; // receiver dropped
                            }
                        }
                    }
                    ClaudeStreamEvent::MessageDelta { usage, .. } => {
                        if let Some(usage) = usage {
                            tokens_out = usage.output_tokens;
                        }
                    }
                    ClaudeStreamEvent::MessageStop {} => {
                        let _ = tx
                            .send(StreamEvent::Done {
                                tokens_in,
                                tokens_out,
                            })
                            .await;
                        return;
                    }
                    ClaudeStreamEvent::Error { error } => {
                        let _ = tx.send(StreamEvent::Error(error.message)).await;
                        return;
                    }
                    // Ignore: ContentBlockStart, ContentBlockStop, Ping
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
        })
        .await;
}
