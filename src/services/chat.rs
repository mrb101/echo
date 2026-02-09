use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::models::{Account, ProviderId};
use crate::providers::{ChatMessage, ChatRequest, ProviderRouter, StreamEvent};
use crate::services::settings::AppSettings;

/// Parameters needed to dispatch a chat request to an AI provider.
pub struct ChatDispatchParams {
    pub request: ChatRequest,
    pub provider: ProviderId,
    pub conversation_id: String,
    pub account_id: String,
    pub model_name: String,
}

/// Result from a non-streaming AI call, ready to be turned into an AppCmd.
pub struct ChatResult {
    pub conversation_id: String,
    pub content: String,
    pub model: String,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub account_id: String,
}

/// Result from streaming: either a token update, completion, or error.
pub enum StreamResult {
    Token {
        conversation_id: String,
        message_id: String,
        accumulated: String,
    },
    Done {
        conversation_id: String,
        message_id: String,
        full_content: String,
        model: String,
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
        account_id: String,
    },
    Error {
        conversation_id: String,
        message_id: String,
        error: String,
    },
}

/// Build a `ChatRequest` from the resolved parameters.
pub fn build_request(
    api_key: String,
    model: &str,
    chat_messages: Vec<ChatMessage>,
    account: &Account,
    settings: &AppSettings,
    system_prompt: Option<String>,
) -> ChatRequest {
    let temperature = if (settings.temperature - 1.0).abs() < f32::EPSILON {
        None
    } else {
        Some(settings.temperature)
    };

    ChatRequest {
        api_key,
        model: model.to_string(),
        messages: chat_messages,
        base_url: account.api_base_url.clone(),
        temperature,
        system_prompt,
        max_tokens: None,
    }
}

/// Convert `Message` list to `ChatMessage` list for the provider API.
pub fn messages_to_chat_messages(messages: &[crate::models::Message]) -> Vec<ChatMessage> {
    messages
        .iter()
        .map(|m| ChatMessage {
            role: m.role,
            content: m.content.clone(),
            images: Vec::new(),
        })
        .collect()
}

/// Run a non-streaming AI request. Returns a `ChatResult` on success.
pub async fn send_non_streaming(
    router: Arc<ProviderRouter>,
    params: ChatDispatchParams,
) -> Result<ChatResult, String> {
    match router
        .send_message(&params.provider, params.request)
        .await
    {
        Ok(response) => Ok(ChatResult {
            conversation_id: params.conversation_id,
            content: response.content,
            model: response.model,
            tokens_in: response.tokens_in,
            tokens_out: response.tokens_out,
            account_id: params.account_id,
        }),
        Err(e) => Err(format!("AI error: {}", e)),
    }
}

/// Run a streaming AI request, sending `StreamResult` events through a callback.
///
/// `on_event` is called for each streaming event. It returns `false` to stop processing
/// (not used currently but available for future use).
pub async fn run_streaming<F>(
    router: Arc<ProviderRouter>,
    params: ChatDispatchParams,
    cancel_token: CancellationToken,
    message_id: String,
    mut on_event: F,
) where
    F: FnMut(StreamResult) + Send,
{
    let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamEvent>(64);

    let provider = params.provider;
    let request = params.request;
    let conv_id = params.conversation_id;
    let acc_id = params.account_id;
    let model = params.model_name;

    let _stream_handle = tokio::spawn(async move {
        if let Err(e) = router.stream_message(&provider, request, tx.clone()).await {
            let _ = tx.send(StreamEvent::Error(e.to_string())).await;
        }
    });

    let mut accumulated = String::new();

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                if !accumulated.is_empty() {
                    on_event(StreamResult::Done {
                        conversation_id: conv_id,
                        message_id,
                        full_content: accumulated,
                        model,
                        tokens_in: None,
                        tokens_out: None,
                        account_id: acc_id,
                    });
                } else {
                    on_event(StreamResult::Error {
                        conversation_id: conv_id,
                        message_id,
                        error: "Generation stopped".to_string(),
                    });
                }
                return;
            }
            event = rx.recv() => {
                match event {
                    Some(StreamEvent::Token(token)) => {
                        accumulated.push_str(&token);
                        on_event(StreamResult::Token {
                            conversation_id: conv_id.clone(),
                            message_id: message_id.clone(),
                            accumulated: accumulated.clone(),
                        });
                    }
                    Some(StreamEvent::Done { tokens_in, tokens_out }) => {
                        on_event(StreamResult::Done {
                            conversation_id: conv_id,
                            message_id,
                            full_content: accumulated,
                            model,
                            tokens_in,
                            tokens_out,
                            account_id: acc_id,
                        });
                        return;
                    }
                    Some(StreamEvent::Error(error)) => {
                        on_event(StreamResult::Error {
                            conversation_id: conv_id,
                            message_id,
                            error,
                        });
                        return;
                    }
                    None => {
                        if !accumulated.is_empty() {
                            on_event(StreamResult::Done {
                                conversation_id: conv_id,
                                message_id,
                                full_content: accumulated,
                                model,
                                tokens_in: None,
                                tokens_out: None,
                                account_id: acc_id,
                            });
                        } else {
                            on_event(StreamResult::Error {
                                conversation_id: conv_id,
                                message_id,
                                error: "Stream ended unexpectedly".to_string(),
                            });
                        }
                        return;
                    }
                }
            }
        }
    }
}

/// Generate a new streaming message ID.
pub fn new_message_id() -> String {
    Uuid::new_v4().to_string()
}
