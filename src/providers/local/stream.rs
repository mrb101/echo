use futures::StreamExt;
use tokio::sync::mpsc;

use super::models::OpenAiStreamChunk;
use crate::providers::types::StreamEvent;

pub async fn parse_sse_stream(response: reqwest::Response, tx: mpsc::Sender<StreamEvent>) {
    let mut stream = response.bytes_stream();
    let mut byte_buf: Vec<u8> = Vec::new();
    let mut buffer = String::new();

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
                    let _ = tx
                        .send(StreamEvent::Done {
                            tokens_in: None,
                            tokens_out: None,
                        })
                        .await;
                    return;
                }

                match serde_json::from_str::<OpenAiStreamChunk>(payload) {
                    Ok(chunk) => {
                        if let Some(choice) = chunk.choices.first() {
                            if let Some(content) = &choice.delta.content {
                                if !content.is_empty() {
                                    if tx.send(StreamEvent::Token(content.clone())).await.is_err()
                                    {
                                        return; // receiver dropped
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
    let _ = tx
        .send(StreamEvent::Done {
            tokens_in: None,
            tokens_out: None,
        })
        .await;
}
