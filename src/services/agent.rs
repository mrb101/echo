use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::models::ProviderId;
use crate::providers::types::{
    ChatMessage, ChatRequest, StopReason, StreamEvent, ToolCall, ToolResult,
};
use crate::providers::ProviderRouter;
use crate::tools::ToolRegistry;

#[derive(Debug, Clone)]
pub enum ApprovalDecision {
    Allow,
    Deny,
    AllowAlways, // Auto-approve this tool going forward
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    TextToken(String),
    ToolCallReceived(ToolCall),
    ToolExecuting {
        call_id: String,
        tool_name: String,
    },
    ToolCompleted {
        call_id: String,
        result: ToolResult,
        duration_ms: u64,
    },
    AwaitingApproval {
        call: ToolCall,
    },
    Done {
        full_content: String,
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
    },
    Error(String),
}

pub struct AgentLoopParams {
    pub request: ChatRequest,
    pub provider_id: ProviderId,
    pub tools: Arc<ToolRegistry>,
    pub router: Arc<ProviderRouter>,
    pub cancel_token: CancellationToken,
    pub max_iterations: u32,
    pub approval_rx: mpsc::Receiver<ApprovalDecision>,
    pub auto_approve_read_tools: bool,
}

/// Send an event to the UI; if the receiver is dropped, abort the agent loop.
macro_rules! send_or_return {
    ($tx:expr, $event:expr) => {
        if $tx.send($event).await.is_err() {
            tracing::warn!("Agent event receiver dropped, stopping agent loop");
            return;
        }
    };
}

pub async fn run_agent_loop(mut params: AgentLoopParams, event_tx: mpsc::Sender<AgentEvent>) {
    let mut iteration: u32 = 0;
    let mut total_tokens_in: Option<i64> = None;
    let mut total_tokens_out: Option<i64> = None;
    let mut full_text = String::new();
    let mut auto_approved_tools: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    // Pre-populate auto-approved read tools
    if params.auto_approve_read_tools {
        for def in params.tools.definitions() {
            if !params.tools.requires_approval(&def.name) {
                auto_approved_tools.insert(def.name);
            }
        }
    }

    loop {
        if params.cancel_token.is_cancelled() {
            let _ = event_tx
                .send(AgentEvent::Error("Cancelled".to_string()))
                .await;
            return;
        }

        if iteration >= params.max_iterations {
            let _ = event_tx
                .send(AgentEvent::Error(format!(
                    "Reached maximum iterations ({})",
                    params.max_iterations
                )))
                .await;
            return;
        }

        iteration += 1;

        // Send request to provider via streaming
        let (stream_tx, mut stream_rx) = mpsc::channel::<StreamEvent>(64);
        let router = params.router.clone();
        let provider_id = params.provider_id;
        let request = params.request.clone();

        let stream_handle = tokio::spawn(async move {
            if let Err(e) = router
                .stream_message(&provider_id, request, stream_tx.clone())
                .await
            {
                let _ = stream_tx.send(StreamEvent::Error(e.to_string())).await;
            }
        });

        // Accumulate streaming response
        let mut iteration_text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut iter_tokens_in: Option<i64> = None;
        let mut iter_tokens_out: Option<i64> = None;
        let mut stop_reason: Option<StopReason> = None;

        loop {
            tokio::select! {
                _ = params.cancel_token.cancelled() => {
                    stream_handle.abort();
                    let _ = event_tx.send(AgentEvent::Done {
                        full_content: full_text,
                        tokens_in: total_tokens_in,
                        tokens_out: total_tokens_out,
                    }).await;
                    return;
                }
                event = stream_rx.recv() => {
                    match event {
                        Some(StreamEvent::Token(token)) => {
                            iteration_text.push_str(&token);
                            send_or_return!(event_tx, AgentEvent::TextToken(token));
                        }
                        Some(StreamEvent::ToolCallStart { .. }) => {
                            // Informational — we wait for ToolCallComplete
                        }
                        Some(StreamEvent::ToolCallDelta { .. }) => {
                            // Accumulation handled by provider stream parser
                        }
                        Some(StreamEvent::ToolCallComplete { call }) => {
                            send_or_return!(event_tx, AgentEvent::ToolCallReceived(call.clone()));
                            tool_calls.push(call);
                        }
                        Some(StreamEvent::Done { tokens_in, tokens_out, stop_reason: sr }) => {
                            iter_tokens_in = tokens_in;
                            iter_tokens_out = tokens_out;
                            stop_reason = sr;
                            break;
                        }
                        Some(StreamEvent::Error(error)) => {
                            let _ = event_tx.send(AgentEvent::Error(error)).await;
                            return;
                        }
                        None => {
                            // Stream ended unexpectedly
                            break;
                        }
                    }
                }
            }
        }

        // Accumulate tokens
        if let Some(ti) = iter_tokens_in {
            *total_tokens_in.get_or_insert(0) += ti;
        }
        if let Some(to) = iter_tokens_out {
            *total_tokens_out.get_or_insert(0) += to;
        }
        full_text.push_str(&iteration_text);

        // If no tool calls, we're done
        if tool_calls.is_empty() || stop_reason != Some(StopReason::ToolUse) {
            let _ = event_tx
                .send(AgentEvent::Done {
                    full_content: full_text,
                    tokens_in: total_tokens_in,
                    tokens_out: total_tokens_out,
                })
                .await;
            return;
        }

        // Execute tool calls
        let mut tool_results: Vec<ToolResult> = Vec::new();

        for call in &tool_calls {
            // Check if approval is needed
            let needs_approval = params.tools.requires_approval(&call.name)
                && !auto_approved_tools.contains(&call.name);

            if needs_approval {
                send_or_return!(
                    event_tx,
                    AgentEvent::AwaitingApproval { call: call.clone() }
                );

                // Wait for approval decision
                let decision = tokio::select! {
                    _ = params.cancel_token.cancelled() => {
                        let _ = event_tx.send(AgentEvent::Done {
                            full_content: full_text,
                            tokens_in: total_tokens_in,
                            tokens_out: total_tokens_out,
                        }).await;
                        return;
                    }
                    decision = params.approval_rx.recv() => {
                        match decision {
                            Some(d) => d,
                            None => {
                                let _ = event_tx.send(AgentEvent::Error("Approval channel closed".to_string())).await;
                                return;
                            }
                        }
                    }
                };

                match decision {
                    ApprovalDecision::Deny => {
                        tool_results.push(ToolResult {
                            call_id: call.id.clone(),
                            content: "Tool call denied by user".to_string(),
                            is_error: true,
                        });
                        continue;
                    }
                    ApprovalDecision::AllowAlways => {
                        auto_approved_tools.insert(call.name.clone());
                    }
                    ApprovalDecision::Allow => {}
                }
            }

            // Execute the tool
            send_or_return!(
                event_tx,
                AgentEvent::ToolExecuting {
                    call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                }
            );

            let start = Instant::now();
            let result = params.tools.execute(call).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            send_or_return!(
                event_tx,
                AgentEvent::ToolCompleted {
                    call_id: call.id.clone(),
                    result: result.clone(),
                    duration_ms,
                }
            );

            tool_results.push(result);
        }

        // Append assistant message (with tool calls) and tool results to conversation
        let assistant_msg = ChatMessage {
            role: crate::models::Role::Assistant,
            content: iteration_text,
            images: Vec::new(),
            tool_calls,
            tool_results: Vec::new(),
        };
        params.request.messages.push(assistant_msg);

        let tool_result_msg = ChatMessage {
            role: crate::models::Role::User,
            content: String::new(),
            images: Vec::new(),
            tool_calls: Vec::new(),
            tool_results,
        };
        params.request.messages.push(tool_result_msg);

        // Loop back for the next iteration
    }
}
